//! Google Drive sync (all platforms): a true OAuth2/API connection.
//!
//! Design (Ariadne's `gdrive.rs` as reference, adapted to Parrot's directory-package store):
//! Drive-section projects sync as one zip per project inside a user-visible `Parrot`
//! folder in the user's My Drive (`drive.file` scope — only files this app creates).
//! The local library folder stays the offline cache: Drive-section projects live on local
//! disk like everything else (distinguished by `section=googledrive` in `.project-meta`)
//! and sync up/down on save plus a 30s remote poll. Newer side wins; a diverged both-sides
//! edit keeps both via a `(conflict copy)` project. Remote deletions move the local project
//! to Trash (recoverable from the library's `.removed/` after the next save).
//!
//! Identity is by project title (the Drive filename stem). Renaming a Drive project on one
//! device reads as delete+create elsewhere — documented limitation (no stable UUIDs yet).
//! Token cache is a file in the platform config dir; the OS keychain is a follow-up (RV-26).
//! Per-file tree sync instead of zips is a follow-up (RV-27).
//!
//! [`GOOGLE_CLIENT_ID`] / [`GOOGLE_CLIENT_SECRET`] come from compile-time env (release builds
//! export the repo's Actions secrets). Without them the feature compiles but reports
//! "not configured". All network I/O runs on worker threads; the UI thread only plans from
//! the in-memory tree and applies byte-level outcomes.

use parrot_core::Project;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// User-visible sync folder in My Drive.
pub const DRIVE_FOLDER_NAME: &str = "Parrot";
/// Least-privilege scope: only files this app creates.
pub const DRIVE_SCOPE: &str = "https://www.googleapis.com/auth/drive.file";
/// Env (compile-time) carrying the OAuth client ID.
pub const ENV_CLIENT_ID: &str = "GOOGLE_CLIENT_ID";
/// Env (compile-time) carrying the OAuth client secret.
pub const ENV_CLIENT_SECRET: &str = "GOOGLE_CLIENT_SECRET";
/// Token cache filename in the platform config dir.
pub const TOKEN_FILE_NAME: &str = "google-tokens.json";
/// Sync-state sidecar filename in the library root.
pub const STATE_FILE_NAME: &str = ".parrot-gdrive-state.json";
/// How often the UI checks the remote for changes.
pub const POLL_INTERVAL_SECS: u64 = 30;
/// Quiet period after an edit before an upload starts.
pub const UPLOAD_QUIET_SECS: u64 = 5;
/// Suffix for a downloaded conflict copy's project title.
pub const CONFLICT_SUFFIX: &str = " (conflict copy)";

const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const DRIVE_FILES_URL: &str = "https://www.googleapis.com/drive/v3/files";
const DRIVE_UPLOAD_URL: &str = "https://www.googleapis.com/upload/drive/v3/files";
const FOLDER_MIME: &str = "application/vnd.google-apps.folder";

/// Compile-time OAuth client ID, when the release build exported it.
pub fn client_id() -> Option<String> {
    std::option_env!("GOOGLE_CLIENT_ID")
        .map(str::to_string)
        .filter(|s| !s.trim().is_empty())
}

/// Compile-time OAuth client secret, when the release build exported it.
pub fn client_secret() -> Option<String> {
    std::option_env!("GOOGLE_CLIENT_SECRET")
        .map(str::to_string)
        .filter(|s| !s.trim().is_empty())
}

/// Whether Drive can even attempt a connection in this build.
pub fn is_configured() -> bool {
    client_id().is_some()
}

/// OAuth tokens. `expires_at_unix` is seconds since the epoch (UTC).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Tokens {
    pub refresh_token: String,
    pub access_token: String,
    pub expires_at_unix: u64,
}

impl Tokens {
    fn expired(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        now + 60 >= self.expires_at_unix
    }
}

/// Where the token cache lives, if the platform's config dir is known.
pub fn token_file() -> Option<PathBuf> {
    let env = |key: &str| {
        std::env::var_os(key)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let dir = if cfg!(windows) {
        env("APPDATA")?.join("Parrot")
    } else if cfg!(target_os = "macos") {
        env("HOME")?.join("Library/Application Support/Parrot")
    } else {
        env("XDG_CONFIG_HOME")
            .or_else(|| env("HOME").map(|h| h.join(".config")))?
            .join("parrot")
    };
    Some(dir.join(TOKEN_FILE_NAME))
}

/// Loads cached tokens, if present and parseable.
pub fn load_tokens() -> Option<Tokens> {
    let path = token_file()?;
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Persists tokens (best-effort file permissions: 0600 on Unix).
pub fn save_tokens(tokens: &Tokens) -> std::io::Result<()> {
    let path = token_file()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no config dir"))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = serde_json::to_string_pretty(tokens)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(&path, text)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Deletes the cached tokens (Disconnect). Missing file is fine.
pub fn clear_tokens() {
    if let Some(path) = token_file() {
        let _ = std::fs::remove_file(path);
    }
}

/// Random PKCE code verifier: 64 unreserved characters.
pub fn code_verifier() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
    let mut buf = [0u8; 64];
    if getrandom::getrandom(&mut buf).is_err() {
        // Degraded fallback when the OS RNG is unavailable: still unique per
        // process/instant, never relied on for secrecy beyond this flow.
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        (std::process::id(), std::time::SystemTime::now()).hash(&mut h);
        let mut seed = h.finish();
        for b in &mut buf {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            *b = ALPHABET[(seed >> 33) as usize % ALPHABET.len()];
        }
        return buf.iter().map(|&b| b as char).collect();
    }
    buf.iter()
        .map(|b| ALPHABET[*b as usize % ALPHABET.len()] as char)
        .collect()
}

/// PKCE S256 code challenge for a verifier.
pub fn code_challenge(verifier: &str) -> String {
    use base64::Engine as _;
    use sha2::Digest as _;
    let digest = sha2::Sha256::digest(verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

/// The browser URL starting the installed-app OAuth flow for `redirect_uri`.
pub fn auth_url(redirect_uri: &str, verifier: &str, state: &str) -> Option<String> {
    let client_id = client_id()?;
    let mut s = url::form_urlencoded::Serializer::new(String::new());
    s.append_pair("client_id", &client_id);
    s.append_pair("redirect_uri", redirect_uri);
    s.append_pair("response_type", "code");
    s.append_pair("scope", DRIVE_SCOPE);
    s.append_pair("code_challenge", &code_challenge(verifier));
    s.append_pair("code_challenge_method", "S256");
    s.append_pair("access_type", "offline");
    s.append_pair("prompt", "consent");
    s.append_pair("state", state);
    Some(format!("{AUTH_URL}?{}", s.finish()))
}

/// Binds the OAuth loopback listener on an ephemeral port.
pub fn bind_loopback() -> std::io::Result<std::net::TcpListener> {
    std::net::TcpListener::bind("127.0.0.1:0")
}

/// Redirect URI for a bound loopback listener.
pub fn loopback_redirect_uri(listener: &std::net::TcpListener) -> Option<String> {
    listener
        .local_addr()
        .ok()
        .map(|a| format!("http://127.0.0.1:{}/", a.port()))
}

/// Blocks until the browser hits the loopback listener, then returns the `code`.
/// Polls a nonblocking accept for up to five minutes so an abandoned Connect
/// attempt never hangs the worker thread forever. Replies with a minimal page
/// so the browser tab can close itself.
pub fn wait_for_auth_code(listener: &std::net::TcpListener) -> anyhow::Result<String> {
    listener.set_nonblocking(true)?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(300);
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream.set_nonblocking(false).ok();
                let mut buf = vec![0u8; 8192];
                use std::io::Read as _;
                let n = stream.read(&mut buf)?;
                let request = String::from_utf8_lossy(&buf[..n]).into_owned();
                let code = parse_auth_response(&request).ok_or_else(|| {
                    anyhow::anyhow!("OAuth redirect carried no authorization code")
                })?;
                let body =
                    "<html><body>Parrot is connected — you can close this tab.</body></html>";
                let reply = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                use std::io::Write as _;
                let _ = stream.write_all(reply.as_bytes());
                return Ok(code);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if std::time::Instant::now() >= deadline {
                    return Err(anyhow::anyhow!("timed out waiting for the browser"));
                }
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            Err(e) => return Err(e.into()),
        }
    }
}

/// Extracts the `code` query param from a raw HTTP request. Pure: unit-tested.
pub fn parse_auth_response(request: &str) -> Option<String> {
    let line = request.lines().next()?;
    let path = line.split_whitespace().nth(1)?;
    let query = path.split_once('?')?.1;
    url::form_urlencoded::parse(query.as_bytes())
        .find(|(k, _)| k == "code")
        .map(|(_, v)| v.into_owned())
        .filter(|s| !s.is_empty())
}

#[derive(Debug, serde::Deserialize)]
struct TokenReply {
    #[serde(default)]
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    #[serde(default)]
    expires_in: u64,
    #[serde(default)]
    error: String,
    #[serde(default)]
    error_description: String,
}

fn tokens_from_reply(reply: TokenReply, old_refresh: &str) -> anyhow::Result<Tokens> {
    if !reply.error.is_empty() {
        return Err(anyhow::anyhow!(
            "Google token endpoint: {} {}",
            reply.error,
            reply.error_description
        ));
    }
    if reply.access_token.is_empty() {
        return Err(anyhow::anyhow!(
            "Google token endpoint returned no access token"
        ));
    }
    let refresh = if reply.refresh_token.is_empty() {
        old_refresh.to_string()
    } else {
        reply.refresh_token
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Ok(Tokens {
        refresh_token: refresh,
        access_token: reply.access_token,
        expires_at_unix: now + reply.expires_in.max(60),
    })
}

/// Exchanges an authorization `code` for tokens (blocking network).
pub fn exchange_code(code: &str, verifier: &str, redirect_uri: &str) -> anyhow::Result<Tokens> {
    let id = client_id().ok_or_else(|| anyhow::anyhow!("Google Drive is not configured"))?;
    let secret = client_secret().unwrap_or_default();
    let body = format!(
        "code={}&client_id={}&client_secret={}&redirect_uri={}&grant_type=authorization_code&code_verifier={}",
        url::form_urlencoded::byte_serialize(code.as_bytes()).collect::<String>(),
        url::form_urlencoded::byte_serialize(id.as_bytes()).collect::<String>(),
        url::form_urlencoded::byte_serialize(secret.as_bytes()).collect::<String>(),
        url::form_urlencoded::byte_serialize(redirect_uri.as_bytes()).collect::<String>(),
        url::form_urlencoded::byte_serialize(verifier.as_bytes()).collect::<String>(),
    );
    let reply: TokenReply = minreq::post(TOKEN_URL)
        .with_header("Content-Type", "application/x-www-form-urlencoded")
        .with_body(body)
        .send()
        .map_err(|e| anyhow::anyhow!("token exchange failed: {e}"))?
        .json()
        .map_err(|e| anyhow::anyhow!("token exchange returned bad JSON: {e}"))?;
    tokens_from_reply(reply, "")
}

/// Refreshes tokens (blocking network). Returns the replacement pair.
pub fn refresh_tokens(tokens: &Tokens) -> anyhow::Result<Tokens> {
    let id = client_id().ok_or_else(|| anyhow::anyhow!("Google Drive is not configured"))?;
    let secret = client_secret().unwrap_or_default();
    let body = format!(
        "client_id={}&client_secret={}&refresh_token={}&grant_type=refresh_token",
        url::form_urlencoded::byte_serialize(id.as_bytes()).collect::<String>(),
        url::form_urlencoded::byte_serialize(secret.as_bytes()).collect::<String>(),
        url::form_urlencoded::byte_serialize(tokens.refresh_token.as_bytes()).collect::<String>(),
    );
    let reply: TokenReply = minreq::post(TOKEN_URL)
        .with_header("Content-Type", "application/x-www-form-urlencoded")
        .with_body(body)
        .send()
        .map_err(|e| anyhow::anyhow!("token refresh failed: {e}"))?
        .json()
        .map_err(|e| anyhow::anyhow!("token refresh returned bad JSON: {e}"))?;
    tokens_from_reply(reply, &tokens.refresh_token)
}

/// One file in the Drive sync folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteFile {
    pub id: String,
    pub name: String,
    pub modified_time: String,
}

/// Minimal Drive backend surface the sync protocol needs. `LiveBackend` talks to
/// Google over HTTPS; tests use `FakeBackend`.
pub trait DriveBackend {
    type Error: std::fmt::Display;
    fn ensure_folder(&mut self, name: &str) -> Result<String, Self::Error>;
    fn list(&mut self, folder_id: &str) -> Result<Vec<RemoteFile>, Self::Error>;
    fn upload_new(
        &mut self,
        folder_id: &str,
        name: &str,
        bytes: &[u8],
    ) -> Result<RemoteFile, Self::Error>;
    fn update(&mut self, file_id: &str, bytes: &[u8]) -> Result<RemoteFile, Self::Error>;
    fn download(&mut self, file_id: &str) -> Result<Vec<u8>, Self::Error>;
    fn trash(&mut self, file_id: &str) -> Result<(), Self::Error>;
}

/// Live Google Drive backend (blocking HTTPS via minreq + platform TLS).
pub struct LiveBackend {
    tokens: Tokens,
}

impl LiveBackend {
    pub fn new(tokens: Tokens) -> Self {
        Self { tokens }
    }

    pub fn tokens(&self) -> &Tokens {
        &self.tokens
    }

    fn fresh_token(&mut self) -> anyhow::Result<String> {
        if self.tokens.expired() {
            self.tokens = refresh_tokens(&self.tokens)?;
            let _ = save_tokens(&self.tokens);
        }
        Ok(self.tokens.access_token.clone())
    }

    fn auth(&mut self, req: minreq::Request) -> anyhow::Result<minreq::Request> {
        let token = self.fresh_token()?;
        Ok(req
            .with_header("Authorization", format!("Bearer {token}"))
            .with_timeout(60))
    }

    /// Runs `f` once; on a 401 refreshes the token and retries once.
    fn with_retry<T>(
        &mut self,
        mut f: impl FnMut(&mut Self) -> anyhow::Result<minreq::Response>,
        parse: impl Fn(minreq::Response) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        let resp = f(self)?;
        if resp.status_code == 401 {
            self.tokens = refresh_tokens(&self.tokens)?;
            let _ = save_tokens(&self.tokens);
            return parse(f(self)?);
        }
        parse(resp)
    }
}

#[derive(Debug, serde::Deserialize)]
struct FileResource {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default, rename = "modifiedTime")]
    modified_time: String,
}

#[derive(Debug, serde::Deserialize)]
struct ListReply {
    #[serde(default)]
    files: Vec<FileResource>,
}

fn check(resp: minreq::Response) -> anyhow::Result<minreq::Response> {
    if (200..300).contains(&resp.status_code) {
        Ok(resp)
    } else {
        Err(anyhow::anyhow!(
            "Google Drive API: HTTP {}",
            resp.status_code
        ))
    }
}

impl DriveBackend for LiveBackend {
    type Error = anyhow::Error;

    fn ensure_folder(&mut self, name: &str) -> Result<String, Self::Error> {
        let q = format!("mimeType = '{FOLDER_MIME}' and name = '{name}' and trashed = false");
        let url = format!(
            "{DRIVE_FILES_URL}?q={}&fields={}",
            url::form_urlencoded::byte_serialize(q.as_bytes()).collect::<String>(),
            url::form_urlencoded::byte_serialize("files(id,name)".as_bytes()).collect::<String>(),
        );
        let found: Option<String> = self.with_retry(
            |b| {
                check(
                    b.auth(minreq::get(&url))?
                        .send()
                        .map_err(|e| anyhow::anyhow!("{e}"))?,
                )
            },
            |resp| {
                let reply: ListReply = resp
                    .json()
                    .map_err(|e| anyhow::anyhow!("folder lookup returned bad JSON: {e}"))?;
                Ok(reply.files.into_iter().next().map(|f| f.id))
            },
        )?;
        if let Some(id) = found {
            return Ok(id);
        }
        let meta = serde_json::json!({ "name": name, "mimeType": FOLDER_MIME }).to_string();
        self.with_retry(
            |b| {
                check(
                    b.auth(minreq::post(DRIVE_FILES_URL))?
                        .with_header("Content-Type", "application/json; charset=utf-8")
                        .with_body(meta.clone())
                        .send()
                        .map_err(|e| anyhow::anyhow!("{e}"))?,
                )
            },
            |resp| {
                let f: FileResource = resp
                    .json()
                    .map_err(|e| anyhow::anyhow!("folder create returned bad JSON: {e}"))?;
                if f.id.is_empty() {
                    return Err(anyhow::anyhow!("folder create returned no id"));
                }
                Ok(f.id)
            },
        )
    }

    fn list(&mut self, folder_id: &str) -> Result<Vec<RemoteFile>, Self::Error> {
        let q = format!("'{folder_id}' in parents and trashed = false");
        let url = format!(
            "{DRIVE_FILES_URL}?q={}&fields={}&pageSize=1000",
            url::form_urlencoded::byte_serialize(q.as_bytes()).collect::<String>(),
            url::form_urlencoded::byte_serialize("files(id,name,modifiedTime)".as_bytes())
                .collect::<String>(),
        );
        self.with_retry(
            |b| {
                check(
                    b.auth(minreq::get(&url))?
                        .send()
                        .map_err(|e| anyhow::anyhow!("{e}"))?,
                )
            },
            |resp| {
                let reply: ListReply = resp
                    .json()
                    .map_err(|e| anyhow::anyhow!("file list returned bad JSON: {e}"))?;
                Ok(reply
                    .files
                    .into_iter()
                    .filter(|f| !f.id.is_empty())
                    .map(|f| RemoteFile {
                        id: f.id,
                        name: f.name,
                        modified_time: f.modified_time,
                    })
                    .collect())
            },
        )
    }

    fn upload_new(
        &mut self,
        folder_id: &str,
        name: &str,
        bytes: &[u8],
    ) -> Result<RemoteFile, Self::Error> {
        let meta = serde_json::json!({ "name": name, "parents": [folder_id] }).to_string();
        let (body, content_type) = multipart(&meta, bytes);
        let url = format!("{DRIVE_UPLOAD_URL}?uploadType=multipart&fields=id,name,modifiedTime");
        self.with_retry(
            |b| {
                check(
                    b.auth(minreq::post(&url))?
                        .with_header("Content-Type", content_type.clone())
                        .with_body(body.clone())
                        .send()
                        .map_err(|e| anyhow::anyhow!("{e}"))?,
                )
            },
            |resp| {
                let f: FileResource = resp
                    .json()
                    .map_err(|e| anyhow::anyhow!("upload returned bad JSON: {e}"))?;
                Ok(RemoteFile {
                    id: f.id,
                    name: f.name,
                    modified_time: f.modified_time,
                })
            },
        )
    }

    fn update(&mut self, file_id: &str, bytes: &[u8]) -> Result<RemoteFile, Self::Error> {
        let url =
            format!("{DRIVE_UPLOAD_URL}/{file_id}?uploadType=media&fields=id,name,modifiedTime");
        self.with_retry(
            |b| {
                check(
                    b.auth(minreq::patch(url.clone()))?
                        .with_header("Content-Type", "application/zip")
                        .with_body(bytes.to_vec())
                        .send()
                        .map_err(|e| anyhow::anyhow!("{e}"))?,
                )
            },
            |resp| {
                let f: FileResource = resp
                    .json()
                    .map_err(|e| anyhow::anyhow!("update returned bad JSON: {e}"))?;
                Ok(RemoteFile {
                    id: f.id,
                    name: f.name,
                    modified_time: f.modified_time,
                })
            },
        )
    }

    fn download(&mut self, file_id: &str) -> Result<Vec<u8>, Self::Error> {
        let url = format!("{DRIVE_FILES_URL}/{file_id}?alt=media");
        self.with_retry(
            |b| {
                check(
                    b.auth(minreq::get(&url))?
                        .send()
                        .map_err(|e| anyhow::anyhow!("{e}"))?,
                )
            },
            |resp| Ok(resp.into_bytes()),
        )
    }

    fn trash(&mut self, file_id: &str) -> Result<(), Self::Error> {
        let url = format!("{DRIVE_FILES_URL}/{file_id}");
        self.with_retry(
            |b| {
                check(
                    b.auth(minreq::patch(url.clone()))?
                        .with_header("Content-Type", "application/json; charset=utf-8")
                        .with_body(r#"{"trashed":true}"#.to_string())
                        .send()
                        .map_err(|e| anyhow::anyhow!("{e}"))?,
                )
            },
            |_| Ok(()),
        )
    }
}

/// Builds a `multipart/related` body: JSON metadata + zip bytes.
fn multipart(meta_json: &str, bytes: &[u8]) -> (Vec<u8>, String) {
    let boundary = "parrot-drive-boundary";
    let mut body = Vec::with_capacity(meta_json.len() + bytes.len() + 256);
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(b"Content-Type: application/json; charset=utf-8\r\n\r\n");
    body.extend_from_slice(meta_json.as_bytes());
    body.extend_from_slice(format!("\r\n--{boundary}\r\n").as_bytes());
    body.extend_from_slice(b"Content-Type: application/zip\r\n\r\n");
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (body, format!("multipart/related; boundary={boundary}"))
}

/// In-memory fake backend for headless protocol tests (and offline `cargo test`).
#[derive(Default)]
pub struct FakeBackend {
    pub files: BTreeMap<String, FakeFile>,
    next_id: u64,
}

#[derive(Clone)]
pub struct FakeFile {
    pub name: String,
    pub bytes: Vec<u8>,
    pub modified_time: String,
    pub trashed: bool,
}

impl FakeBackend {
    fn stamp(&mut self) -> String {
        self.next_id += 1;
        format!("2026-01-01T00:00:{:02}Z", self.next_id % 60)
    }
}

impl DriveBackend for FakeBackend {
    type Error = anyhow::Error;

    fn ensure_folder(&mut self, _name: &str) -> Result<String, Self::Error> {
        Ok("fake-folder".to_string())
    }

    fn list(&mut self, _folder_id: &str) -> Result<Vec<RemoteFile>, Self::Error> {
        Ok(self
            .files
            .iter()
            .filter(|(_, f)| !f.trashed)
            .map(|(id, f)| RemoteFile {
                id: id.clone(),
                name: f.name.clone(),
                modified_time: f.modified_time.clone(),
            })
            .collect())
    }

    fn upload_new(
        &mut self,
        _folder_id: &str,
        name: &str,
        bytes: &[u8],
    ) -> Result<RemoteFile, Self::Error> {
        self.next_id += 1;
        let id = format!("fake-{}", self.next_id);
        let modified_time = self.stamp();
        self.files.insert(
            id.clone(),
            FakeFile {
                name: name.to_string(),
                bytes: bytes.to_vec(),
                modified_time: modified_time.clone(),
                trashed: false,
            },
        );
        Ok(RemoteFile {
            id,
            name: name.to_string(),
            modified_time,
        })
    }

    fn update(&mut self, file_id: &str, bytes: &[u8]) -> Result<RemoteFile, Self::Error> {
        let modified_time = self.stamp();
        let f = self
            .files
            .get_mut(file_id)
            .ok_or_else(|| anyhow::anyhow!("fake: unknown file {file_id}"))?;
        f.bytes = bytes.to_vec();
        f.modified_time = modified_time.clone();
        Ok(RemoteFile {
            id: file_id.to_string(),
            name: f.name.clone(),
            modified_time,
        })
    }

    fn download(&mut self, file_id: &str) -> Result<Vec<u8>, Self::Error> {
        self.files
            .get(file_id)
            .filter(|f| !f.trashed)
            .map(|f| f.bytes.clone())
            .ok_or_else(|| anyhow::anyhow!("fake: unknown file {file_id}"))
    }

    fn trash(&mut self, file_id: &str) -> Result<(), Self::Error> {
        let f = self
            .files
            .get_mut(file_id)
            .ok_or_else(|| anyhow::anyhow!("fake: unknown file {file_id}"))?;
        f.trashed = true;
        Ok(())
    }
}

/// One synced project's remote bookkeeping, keyed by project title.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DriveEntry {
    pub file_id: String,
    pub hash: u64,
    pub remote_modified: String,
}

/// Whole-library Drive sync state, persisted as `.parrot-gdrive-state.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DriveSyncState {
    #[serde(default)]
    pub entries: BTreeMap<String, DriveEntry>,
}

impl DriveSyncState {
    pub fn load(dir: &Path) -> Self {
        std::fs::read_to_string(dir.join(STATE_FILE_NAME))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        let text = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(dir.join(STATE_FILE_NAME), text)
    }
}

/// FNV-1a content hash for change detection (matches Ariadne's scheme).
pub fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Drive filename for a project title.
pub fn remote_name(title: &str) -> String {
    format!("{}.zip", parrot_container::tree::entry_stem(title))
}

/// Title recovered from a Drive filename (`<stem>.zip`).
pub fn title_from_remote(name: &str) -> Option<String> {
    name.strip_suffix(".zip").map(str::to_string)
}

/// Appends ` (conflict copy)` to a project title.
pub fn with_conflict_suffix(title: &str) -> String {
    format!("{title}{CONFLICT_SUFFIX}")
}

/// A unique project title among `taken` (case-insensitive, like on-disk dedup).
pub fn unique_title(base: &str, taken: &[String]) -> String {
    let lowered: Vec<String> = taken.iter().map(|t| t.to_ascii_lowercase()).collect();
    if !lowered.contains(&base.to_ascii_lowercase()) {
        return base.to_string();
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base} {n}");
        if !lowered.contains(&candidate.to_ascii_lowercase()) {
            return candidate;
        }
        n += 1;
    }
}

/// Zips one on-disk project directory (post-save state) for upload.
/// The zip is rooted at the `<title>.ulproject/` folder itself so an unzip can
/// find and open it back. Skips `.removed/` and atomic-write temp siblings;
/// keeps everything else (`.project-meta`, `.order`, sidecars, sheet packages).
pub fn zip_project_dir(dir: &Path) -> anyhow::Result<Vec<u8>> {
    let prefix = dir
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow::anyhow!("project dir has no name"))?;
    let mut cursor = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut cursor);
        let options: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(current) = stack.pop() {
            let entries: Vec<_> = std::fs::read_dir(&current)?.collect::<Result<_, _>>()?;
            for entry in entries {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();
                if name == ".removed" || name.contains(".tmp-") {
                    continue;
                }
                let rel = path
                    .strip_prefix(dir)
                    .map_err(|e| anyhow::anyhow!("zip walk: {e}"))?;
                let rel = format!("{prefix}/{}", rel.to_string_lossy().replace('\\', "/"));
                if path.is_dir() {
                    zip.add_directory(rel, options)?;
                    stack.push(path);
                } else {
                    zip.start_file(rel, options)?;
                    use std::io::Write as _;
                    let bytes = std::fs::read(&path)?;
                    zip.write_all(&bytes)?;
                }
            }
        }
        zip.finish()?;
    }
    Ok(cursor.into_inner())
}

/// Extracts project zip bytes into a fresh unique temp dir. Caller removes it.
pub fn unzip_to_temp(bytes: &[u8]) -> anyhow::Result<PathBuf> {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nonce = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("parrot-gdrive-{}-{nonce}", std::process::id()));
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    std::fs::create_dir_all(&dir)?;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let Some(path) = file.enclosed_name() else {
            continue;
        };
        let dest = dir.join(path);
        if file.is_dir() {
            std::fs::create_dir_all(&dest)?;
        } else {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out = std::fs::File::create(&dest)?;
            use std::io::Read as _;
            let mut buf = Vec::new();
            file.read_to_end(&mut buf)?;
            use std::io::Write as _;
            out.write_all(&buf)?;
        }
    }
    Ok(dir)
}

/// Outcome of one protocol run, for status notes and trash handling.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SyncReport {
    pub uploaded: usize,
    pub downloaded: usize,
    pub conflict_copies: usize,
    pub remote_deleted: Vec<String>,
    /// Titles materialized from remote bytes (replaced or newly inserted).
    /// The UI thread upserts these project nodes into the live library.
    pub remote_applied: Vec<String>,
    pub changed: bool,
}

/// Runs the full sync protocol (worker threads and tests share this).
/// `library_root` is where the projects live on disk (post-save state, used for
/// zipping uploads). Mutates `library` (adds/updates Drive-section projects) and
/// `state`. Titles in `remote_deleted` need moving to Trash by the caller.
pub fn sync_once<B: DriveBackend<Error = anyhow::Error>>(
    library: &mut parrot_core::Library,
    sync_state: &mut DriveSyncState,
    library_root: &Path,
    backend: &mut B,
) -> anyhow::Result<SyncReport> {
    use parrot_core::{Node, Section};
    let mut report = SyncReport::default();

    let folder = backend.ensure_folder(DRIVE_FOLDER_NAME)?;
    let remote = backend.list(&folder)?;
    let remote_by_name: BTreeMap<&str, &RemoteFile> =
        remote.iter().map(|f| (f.name.as_str(), f)).collect();

    // Zip every local Drive project up front (post-save disk state).
    struct Local {
        index: usize,
        title: String,
        bytes: Vec<u8>,
        hash: u64,
    }
    let mut locals: Vec<Local> = Vec::new();
    for (i, node) in library.children.iter().enumerate() {
        let Node::Project(p) = node else { continue };
        if p.section != Section::GoogleDrive {
            continue;
        }
        let Some(dir) = parrot_container::tree::node_disk_path(library_root, library, &[i]) else {
            continue;
        };
        if !dir.is_dir() {
            continue;
        }
        let bytes = zip_project_dir(&dir)?;
        let hash = hash_bytes(&bytes);
        locals.push(Local {
            index: i,
            title: p.title.clone(),
            bytes,
            hash,
        });
    }

    let mut seen_remote: BTreeMap<String, bool> = BTreeMap::new();
    let mut trash_titles: Vec<String> = Vec::new();

    for local in &locals {
        let name = remote_name(&local.title);
        let entry = sync_state.entries.get(&local.title).cloned();
        let remote_file = remote_by_name.get(name.as_str()).copied();
        match (entry, remote_file) {
            (None, None) => {
                let up = backend.upload_new(&folder, &name, &local.bytes)?;
                sync_state.entries.insert(
                    local.title.clone(),
                    DriveEntry {
                        file_id: up.id,
                        hash: local.hash,
                        remote_modified: up.modified_time,
                    },
                );
                report.uploaded += 1;
                report.changed = true;
            }
            (None, Some(rf)) => {
                // Untracked on both sides with the same name: keep both, local wins.
                let bytes = backend.download(&rf.id)?;
                let copy_title = unique_title(
                    &with_conflict_suffix(&local.title),
                    &library
                        .children
                        .iter()
                        .map(|n| match n {
                            Node::Project(p) => p.title.clone(),
                            Node::Sheet(s) => s.title.clone(),
                            Node::Group(g) => g.title.clone(),
                            Node::Filter(f) => f.title.clone(),
                        })
                        .collect::<Vec<_>>(),
                );
                insert_downloaded(library, &copy_title, &bytes)?;
                report.remote_applied.push(copy_title.clone());
                let up = backend.update(&rf.id, &local.bytes)?;
                sync_state.entries.insert(
                    local.title.clone(),
                    DriveEntry {
                        file_id: rf.id.clone(),
                        hash: local.hash,
                        remote_modified: up.modified_time,
                    },
                );
                report.conflict_copies += 1;
                report.changed = true;
            }
            (Some(_), None) => {
                // Was synced before, now gone remotely: the other side deleted it.
                trash_titles.push(local.title.clone());
                sync_state.entries.remove(&local.title);
                report.changed = true;
            }
            (Some(e), Some(rf)) => {
                let local_changed = local.hash != e.hash;
                let remote_changed = rf.modified_time != e.remote_modified;
                match (local_changed, remote_changed) {
                    (false, false) => {}
                    (true, false) => {
                        let up = backend.update(&rf.id, &local.bytes)?;
                        sync_state.entries.insert(
                            local.title.clone(),
                            DriveEntry {
                                file_id: rf.id.clone(),
                                hash: local.hash,
                                remote_modified: up.modified_time,
                            },
                        );
                        report.uploaded += 1;
                        report.changed = true;
                    }
                    (false, true) => {
                        let bytes = backend.download(&rf.id)?;
                        replace_with_downloaded(library, local.index, &local.title, &bytes)?;
                        report.remote_applied.push(local.title.clone());
                        sync_state.entries.insert(
                            local.title.clone(),
                            DriveEntry {
                                file_id: rf.id.clone(),
                                hash: hash_bytes(&bytes),
                                remote_modified: rf.modified_time.clone(),
                            },
                        );
                        report.downloaded += 1;
                        report.changed = true;
                    }
                    (true, true) => {
                        let bytes = backend.download(&rf.id)?;
                        let copy_title = unique_title(
                            &with_conflict_suffix(&local.title),
                            &library
                                .children
                                .iter()
                                .map(|n| match n {
                                    Node::Project(p) => p.title.clone(),
                                    Node::Sheet(s) => s.title.clone(),
                                    Node::Group(g) => g.title.clone(),
                                    Node::Filter(f) => f.title.clone(),
                                })
                                .collect::<Vec<_>>(),
                        );
                        insert_downloaded(library, &copy_title, &bytes)?;
                        report.remote_applied.push(copy_title.clone());
                        let up = backend.update(&rf.id, &local.bytes)?;
                        sync_state.entries.insert(
                            local.title.clone(),
                            DriveEntry {
                                file_id: rf.id.clone(),
                                hash: local.hash,
                                remote_modified: up.modified_time,
                            },
                        );
                        report.conflict_copies += 1;
                        report.changed = true;
                    }
                }
                seen_remote.insert(rf.id.clone(), true);
            }
        }
        if let Some(rf) = remote_by_name.get(name.as_str()) {
            seen_remote.insert(rf.id.clone(), true);
        }
    }

    // Remote files with no local project: adopt them.
    for rf in &remote {
        if seen_remote.contains_key(&rf.id) {
            continue;
        }
        let Some(stem) = title_from_remote(&rf.name) else {
            continue;
        };
        if locals.iter().any(|l| remote_name(&l.title) == rf.name) {
            continue;
        }
        let titles: Vec<String> = library
            .children
            .iter()
            .map(|n| match n {
                Node::Project(p) => p.title.clone(),
                Node::Sheet(s) => s.title.clone(),
                Node::Group(g) => g.title.clone(),
                Node::Filter(f) => f.title.clone(),
            })
            .collect();
        let title = unique_title(&stem, &titles);
        let bytes = backend.download(&rf.id)?;
        insert_downloaded(library, &title, &bytes)?;
        report.remote_applied.push(title.clone());
        sync_state.entries.insert(
            title,
            DriveEntry {
                file_id: rf.id.clone(),
                hash: hash_bytes(&bytes),
                remote_modified: rf.modified_time.clone(),
            },
        );
        report.downloaded += 1;
        report.changed = true;
    }

    // Local deletes propagate up: synced before, project gone locally, still remote.
    let local_titles: Vec<String> = locals.iter().map(|l| l.title.clone()).collect();
    let mut gone: Vec<(String, String)> = Vec::new();
    for (title, entry) in sync_state.entries.iter() {
        if local_titles.contains(title) {
            continue;
        }
        if trash_titles.contains(title) {
            continue;
        }
        if remote_by_name.values().any(|rf| rf.id == entry.file_id) {
            gone.push((title.clone(), entry.file_id.clone()));
        }
    }
    for (title, file_id) in gone {
        backend.trash(&file_id)?;
        sync_state.entries.remove(&title);
        report.changed = true;
    }

    report.remote_deleted = trash_titles;
    Ok(report)
}

/// Commands the UI thread can hand to the sync worker. Kept minimal so the
/// worker never touches egui state: it only moves bytes and tokens.
pub struct SyncJobResult {
    pub tokens: Tokens,
    pub upserts: Vec<Project>,
    pub remote_deleted: Vec<String>,
    pub state: DriveSyncState,
    pub uploaded: usize,
    pub downloaded: usize,
    pub conflict_copies: usize,
    pub error: Option<String>,
}

/// Runs one sync on a worker thread. Returns the receiver the UI pumps each
/// frame. Network and zipping stay off the UI thread; applying stays on it.
pub fn spawn_sync(
    snapshot: Vec<parrot_core::Node>,
    sync_state: DriveSyncState,
    library_root: PathBuf,
    tokens: Tokens,
) -> std::sync::mpsc::Receiver<SyncJobResult> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut backend = LiveBackend::new(tokens);
        let result = match run_sync_snapshot(snapshot, sync_state, &library_root, &mut backend) {
            Ok((upserts, state, report, remote_deleted)) => SyncJobResult {
                tokens: backend.tokens,
                upserts,
                remote_deleted,
                state,
                uploaded: report.uploaded,
                downloaded: report.downloaded,
                conflict_copies: report.conflict_copies,
                error: None,
            },
            Err(e) => SyncJobResult {
                tokens: backend.tokens,
                upserts: Vec::new(),
                remote_deleted: Vec::new(),
                state: DriveSyncState::default(),
                uploaded: 0,
                downloaded: 0,
                conflict_copies: 0,
                error: Some(e.to_string()),
            },
        };
        let _ = tx.send(result);
    });
    rx
}

/// A pending browser authorization: the verifier plus the listener the
/// browser will redirect back to.
pub struct PendingConnect {
    pub verifier: String,
    pub redirect_uri: String,
    pub listener: std::net::TcpListener,
    pub state: String,
}

/// Binds loopback and builds the browser URL. `None` when unconfigured.
pub fn begin_connect() -> Option<(PendingConnect, String)> {
    let listener = bind_loopback().ok()?;
    let redirect_uri = loopback_redirect_uri(&listener)?;
    let verifier = code_verifier();
    // Anti-CSRF state: random per attempt.
    let state = code_verifier()[..16].to_string();
    let url = auth_url(&redirect_uri, &verifier, &state)?;
    Some((
        PendingConnect {
            verifier,
            redirect_uri,
            listener,
            state,
        },
        url,
    ))
}

/// Waits for the browser, exchanges the code, and persists the tokens — all on
/// a worker thread. Returns the receiver the UI pumps each frame.
pub fn spawn_connect(pending: PendingConnect) -> std::sync::mpsc::Receiver<Result<Tokens, String>> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = (|| -> anyhow::Result<Tokens> {
            let code = wait_for_auth_code(&pending.listener)?;
            let tokens = exchange_code(&code, &pending.verifier, &pending.redirect_uri)?;
            save_tokens(&tokens).map_err(|e| anyhow::anyhow!("could not save tokens: {e}"))?;
            Ok(tokens)
        })();
        let _ = tx.send(result.map_err(|e| e.to_string()));
    });
    rx
}

/// A sendable library skeleton for the sync worker: root nodes with emptied
/// children. Kinds, titles and sections are preserved so on-disk name
/// resolution matches the live library exactly; filters are kept so indices align.
pub fn snapshot_nodes(library: &parrot_core::Library) -> Vec<parrot_core::Node> {
    use parrot_core::Node;
    library
        .children
        .iter()
        .map(|n| match n {
            Node::Sheet(s) => Node::Sheet(parrot_core::Sheet {
                title: s.title.clone(),
                content: parrot_core::SheetContent::new(Vec::new()),
                goal: None,
                keywords: Vec::new(),
                attachments: Vec::new(),
                glued: false,
                draft: false,
            }),
            Node::Group(g) => Node::Group(parrot_core::Group {
                title: g.title.clone(),
                children: Vec::new(),
                goal: None,
            }),
            Node::Project(p) => Node::Project(Project {
                title: p.title.clone(),
                children: Vec::new(),
                goal: None,
                language: p.language.clone(),
                section: p.section,
            }),
            Node::Filter(f) => Node::Filter(f.clone()),
        })
        .collect()
}

/// Worker entry point: runs the full protocol against a snapshot on a background
/// thread and returns everything the UI thread needs to apply. `upserts` are the
/// Drive-section projects materialized from remote bytes (titles in
/// `report.remote_applied`); unchanged local projects are omitted.
pub fn run_sync_snapshot<B: DriveBackend<Error = anyhow::Error>>(
    snapshot: Vec<parrot_core::Node>,
    mut sync_state: DriveSyncState,
    library_root: &Path,
    backend: &mut B,
) -> anyhow::Result<(Vec<Project>, DriveSyncState, SyncReport, Vec<String>)> {
    use parrot_core::Node;
    let mut library = parrot_core::Library { children: snapshot };
    let report = sync_once(&mut library, &mut sync_state, library_root, backend)?;
    let mut upserts = Vec::new();
    for title in &report.remote_applied {
        if let Some(Node::Project(p)) = library
            .children
            .iter()
            .find(|n| matches!(n, Node::Project(p) if &p.title == title))
        {
            upserts.push(p.clone());
        }
    }
    let remote_deleted = report.remote_deleted.clone();
    Ok((upserts, sync_state, report, remote_deleted))
}

/// Materializes downloaded zip bytes as a new Drive-section project.
fn insert_downloaded(
    library: &mut parrot_core::Library,
    title: &str,
    bytes: &[u8],
) -> anyhow::Result<()> {
    use parrot_core::{Node, Section};
    let dir = unzip_to_temp(bytes)?;
    let result = (|| -> anyhow::Result<Node> {
        let mut found: Option<Node> = None;
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.ends_with(".ulproject") {
                let stem = name.strip_suffix(".ulproject").unwrap_or(&name).to_string();
                let mut project = parrot_container::project::open(&entry.path(), &stem)?;
                project.title = title.to_string();
                project.section = Section::GoogleDrive;
                found = Some(Node::Project(project));
                break;
            }
        }
        found.ok_or_else(|| anyhow::anyhow!("downloaded zip holds no project"))
    })();
    let _ = std::fs::remove_dir_all(&dir);
    library.children.push(result?);
    Ok(())
}

/// Replaces the project at `index` with downloaded bytes (same title family).
fn replace_with_downloaded(
    library: &mut parrot_core::Library,
    index: usize,
    title: &str,
    bytes: &[u8],
) -> anyhow::Result<()> {
    use parrot_core::{Node, Section};
    let dir = unzip_to_temp(bytes)?;
    let result = (|| -> anyhow::Result<Project> {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.ends_with(".ulproject") {
                let stem = name.strip_suffix(".ulproject").unwrap_or(&name).to_string();
                let mut project = parrot_container::project::open(&entry.path(), &stem)?;
                project.title = title.to_string();
                project.section = Section::GoogleDrive;
                return Ok(project);
            }
        }
        Err(anyhow::anyhow!("downloaded zip holds no project"))
    })();
    let _ = std::fs::remove_dir_all(&dir);
    let project = result?;
    match library.children.get_mut(index) {
        Some(Node::Project(slot)) => {
            *slot = project;
            Ok(())
        }
        _ => Err(anyhow::anyhow!("project moved during sync")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_challenge_matches_rfc7636_vector() {
        // RFC 7636 §B: verifier dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk
        // challenges to E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM
        // (verified against Python hashlib).
        assert_eq!(
            code_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn verifier_is_long_random_alphanumeric() {
        let a = code_verifier();
        let b = code_verifier();
        assert_eq!(a.len(), 64);
        assert!(a
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-._~".contains(&c)));
        assert_ne!(a, b, "two verifiers must differ");
    }

    #[test]
    fn auth_response_parsing_reads_code() {
        let raw = "GET /?code=4/abc-123&scope=x HTTP/1.1\r\nHost: 127.0.0.1:9\r\n\r\n";
        assert_eq!(parse_auth_response(raw).as_deref(), Some("4/abc-123"));
        assert_eq!(
            parse_auth_response("GET /?error=denied HTTP/1.1\r\n\r\n"),
            None
        );
        assert_eq!(parse_auth_response("garbage"), None);
    }

    #[test]
    fn remote_names_round_trip() {
        assert_eq!(remote_name("My Novel"), "My Novel.zip");
        assert_eq!(remote_name("a/b:c"), "a-b-c.zip");
        assert_eq!(
            title_from_remote("My Novel.zip").as_deref(),
            Some("My Novel")
        );
        assert_eq!(title_from_remote("notes.txt"), None);
        assert_eq!(with_conflict_suffix("X"), "X (conflict copy)");
        assert_eq!(unique_title("A", &["a".to_string()]), "A 2");
        assert_eq!(
            unique_title("A", &["A".to_string(), "A 2".to_string()]),
            "A 3"
        );
    }

    #[test]
    fn hash_is_stable_and_sensitive() {
        assert_eq!(hash_bytes(b"abc"), hash_bytes(b"abc"));
        assert_ne!(hash_bytes(b"abc"), hash_bytes(b"abd"));
    }

    fn drive_project(title: &str) -> parrot_core::Node {
        parrot_core::Node::Project(parrot_core::Project {
            title: title.to_string(),
            children: Vec::new(),
            goal: None,
            language: None,
            section: parrot_core::Section::GoogleDrive,
        })
    }

    fn local_fixture() -> (tempfile::TempDir, parrot_core::Library) {
        let dir = tempfile::tempdir().expect("tempdir");
        let library = parrot_core::Library {
            children: vec![drive_project("Cloud Novel")],
        };
        parrot_container::tree::save_library(dir.path(), &library).expect("save");
        (dir, library)
    }

    fn zip_of(dir: &std::path::Path, title: &str) -> Vec<u8> {
        let library = parrot_container::tree::open_library(dir).expect("open");
        let idx = library
            .children
            .iter()
            .position(|n| matches!(n, parrot_core::Node::Project(p) if p.title == title))
            .expect("project");
        let path = parrot_container::tree::node_disk_path(dir, &library, &[idx]).expect("path");
        zip_project_dir(&path).expect("zip")
    }

    #[test]
    fn zip_round_trips_a_project() {
        let (dir, _) = local_fixture();
        let bytes = zip_of(dir.path(), "Cloud Novel");
        assert!(!bytes.is_empty());
        let out = unzip_to_temp(&bytes).expect("unzip");
        let found: Vec<_> = std::fs::read_dir(&out)
            .expect("read")
            .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
            .collect();
        assert!(
            found.iter().any(|n| n.ends_with(".ulproject")),
            "zip holds the project dir, got {found:?}"
        );
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn first_sync_uploads_everything() {
        let (dir, mut library) = local_fixture();
        let mut state = DriveSyncState::default();
        let mut backend = FakeBackend::default();
        let report = sync_once(&mut library, &mut state, dir.path(), &mut backend).expect("sync");
        assert_eq!(report.uploaded, 1);
        assert!(report.changed);
        assert_eq!(state.entries.len(), 1);
        assert!(backend.files.values().any(|f| f.name == "Cloud Novel.zip"));
    }

    #[test]
    fn clean_second_sync_is_a_no_op() {
        let (dir, mut library) = local_fixture();
        let mut state = DriveSyncState::default();
        let mut backend = FakeBackend::default();
        sync_once(&mut library, &mut state, dir.path(), &mut backend).expect("sync");
        let report = sync_once(&mut library, &mut state, dir.path(), &mut backend).expect("sync");
        assert_eq!(report, SyncReport::default());
    }

    #[test]
    fn remote_only_project_is_adopted() {
        let (dir, mut library) = local_fixture();
        let mut state = DriveSyncState::default();
        let mut backend = FakeBackend::default();
        // Seed the remote with a foreign project zip.
        let other = tempfile::tempdir().expect("tempdir");
        let other_lib = parrot_core::Library {
            children: vec![drive_project("Elsewhere")],
        };
        parrot_container::tree::save_library(other.path(), &other_lib).expect("save");
        let bytes = zip_of(other.path(), "Elsewhere");
        backend
            .upload_new("fake-folder", "Elsewhere.zip", &bytes)
            .expect("seed");
        let report = sync_once(&mut library, &mut state, dir.path(), &mut backend).expect("sync");
        assert_eq!(report.downloaded, 1);
        assert!(library
            .children
            .iter()
            .any(|n| matches!(n, parrot_core::Node::Project(p) if p.title == "Elsewhere")));
    }

    #[test]
    fn diverged_both_sides_keeps_both() {
        let (dir, mut library) = local_fixture();
        let mut state = DriveSyncState::default();
        let mut backend = FakeBackend::default();
        sync_once(&mut library, &mut state, dir.path(), &mut backend).expect("sync");
        // Diverge: new local bytes on disk...
        if let parrot_core::Node::Project(p) = &mut library.children[0] {
            p.children
                .push(parrot_core::Node::Group(parrot_core::Group {
                    title: "New Section".to_string(),
                    children: Vec::new(),
                    goal: None,
                }));
        }
        parrot_container::tree::save_library(dir.path(), &library).expect("save");
        // ...and different remote bytes under the same name.
        let other = tempfile::tempdir().expect("tempdir");
        let mut other_lib = parrot_core::Library {
            children: vec![drive_project("Cloud Novel")],
        };
        if let parrot_core::Node::Project(p) = &mut other_lib.children[0] {
            p.children
                .push(parrot_core::Node::Group(parrot_core::Group {
                    title: "Remote Section".to_string(),
                    children: Vec::new(),
                    goal: None,
                }));
        }
        parrot_container::tree::save_library(other.path(), &other_lib).expect("save");
        let remote_bytes = zip_of(other.path(), "Cloud Novel");
        let file_id = backend
            .list("fake-folder")
            .expect("list")
            .into_iter()
            .find(|f| f.name == "Cloud Novel.zip")
            .expect("remote file")
            .id;
        backend.update(&file_id, &remote_bytes).expect("diverge");
        // Force the remote clock past the recorded one.
        if let Some(f) = backend.files.get_mut(&file_id) {
            f.modified_time = "2026-06-06T06:06:06Z".to_string();
        }
        let report = sync_once(&mut library, &mut state, dir.path(), &mut backend).expect("sync");
        assert_eq!(report.conflict_copies, 1);
        assert!(library.children.iter().any(|n| matches!(n, parrot_core::Node::Project(p) if p.title == "Cloud Novel (conflict copy)")));
    }

    #[test]
    fn remote_deletion_is_reported_for_trash() {
        let (dir, mut library) = local_fixture();
        let mut state = DriveSyncState::default();
        let mut backend = FakeBackend::default();
        sync_once(&mut library, &mut state, dir.path(), &mut backend).expect("sync");
        let file_id = backend
            .list("fake-folder")
            .expect("list")
            .into_iter()
            .find(|f| f.name == "Cloud Novel.zip")
            .expect("remote file")
            .id;
        backend.trash(&file_id).expect("remote delete");
        let report = sync_once(&mut library, &mut state, dir.path(), &mut backend).expect("sync");
        assert_eq!(report.remote_deleted, vec!["Cloud Novel".to_string()]);
        assert!(!state.entries.contains_key("Cloud Novel"));
    }

    #[test]
    fn locally_trashed_projects_are_trashed_remotely() {
        let (dir, mut library) = local_fixture();
        let mut state = DriveSyncState::default();
        let mut backend = FakeBackend::default();
        sync_once(&mut library, &mut state, dir.path(), &mut backend).expect("sync");
        library.children.clear();
        let report = sync_once(&mut library, &mut state, dir.path(), &mut backend).expect("sync");
        assert!(report.changed);
        assert!(state.entries.is_empty());
        assert!(backend.list("fake-folder").expect("list").is_empty());
    }
}
