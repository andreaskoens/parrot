//! Operating-system integration: native file dialogs (via `rfd`) and handing a file to the
//! shell (open with its default app, show it in Explorer). Behind the [`Os`] trait so the UI's
//! dialog-driven flows are testable headlessly with [`FakeOs`]; nothing here touches the
//! network.

use std::path::{Path, PathBuf};

/// A file-type filter for a dialog: a label and its extensions (without the dot).
pub type FileFilter<'a> = (&'a str, &'a [&'a str]);

pub trait Os {
    /// A folder chosen by the user, starting in `start` when given.
    fn pick_folder(&self, title: &str, start: Option<&Path>) -> Option<PathBuf>;
    /// An existing file chosen by the user.
    fn pick_file(&self, title: &str, filters: &[FileFilter]) -> Option<PathBuf>;
    /// A save destination, pre-filled with `default` (its folder and file name).
    fn save_file(&self, title: &str, default: &Path, filters: &[FileFilter]) -> Option<PathBuf>;
    /// Opens `path` with the system's default app for it.
    fn open_path(&self, path: &Path) -> std::io::Result<()>;
    /// Shows `path` selected in the system file manager.
    fn reveal_path(&self, path: &Path) -> std::io::Result<()>;
}

/// The real implementation: blocking native dialogs, shell launches that don't wait.
pub struct NativeOs;

fn dialog(title: &str, filters: &[FileFilter]) -> rfd::FileDialog {
    filters.iter().fold(
        rfd::FileDialog::new().set_title(title),
        |d, (name, exts)| d.add_filter(*name, exts),
    )
}

impl Os for NativeOs {
    fn pick_folder(&self, title: &str, start: Option<&Path>) -> Option<PathBuf> {
        let mut d = rfd::FileDialog::new().set_title(title);
        if let Some(dir) = start.filter(|p| p.is_dir()) {
            d = d.set_directory(dir);
        }
        d.pick_folder()
    }

    fn pick_file(&self, title: &str, filters: &[FileFilter]) -> Option<PathBuf> {
        dialog(title, filters).pick_file()
    }

    fn save_file(&self, title: &str, default: &Path, filters: &[FileFilter]) -> Option<PathBuf> {
        let mut d = dialog(title, filters);
        if let Some(dir) = default.parent().filter(|p| p.is_dir()) {
            d = d.set_directory(dir);
        }
        if let Some(name) = default.file_name() {
            d = d.set_file_name(name.to_string_lossy());
        }
        d.save_file()
    }

    fn open_path(&self, path: &Path) -> std::io::Result<()> {
        shell_open(path)
    }

    fn reveal_path(&self, path: &Path) -> std::io::Result<()> {
        shell_reveal(path)
    }
}

#[cfg(windows)]
fn shell_open(path: &Path) -> std::io::Result<()> {
    std::process::Command::new("explorer")
        .arg(path)
        .spawn()
        .map(drop)
}

#[cfg(windows)]
fn shell_reveal(path: &Path) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    // Explorer parses `/select,` itself and needs the path quoted after the comma, which
    // `Command::arg`'s whole-argument quoting would break.
    std::process::Command::new("explorer")
        .raw_arg(format!("/select,\"{}\"", path.display()))
        .spawn()
        .map(drop)
}

#[cfg(target_os = "macos")]
fn shell_open(path: &Path) -> std::io::Result<()> {
    std::process::Command::new("open")
        .arg(path)
        .spawn()
        .map(drop)
}

#[cfg(target_os = "macos")]
fn shell_reveal(path: &Path) -> std::io::Result<()> {
    std::process::Command::new("open")
        .arg("-R")
        .arg(path)
        .spawn()
        .map(drop)
}

#[cfg(not(any(windows, target_os = "macos")))]
fn shell_open(path: &Path) -> std::io::Result<()> {
    std::process::Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map(drop)
}

#[cfg(not(any(windows, target_os = "macos")))]
fn shell_reveal(path: &Path) -> std::io::Result<()> {
    shell_open(path.parent().unwrap_or(path))
}

/// Test double: answers dialogs from a queue (`None` = the user cancelled) and records every
/// call, so UI tests can drive dialog flows without a display.
#[derive(Default)]
pub struct FakeOs {
    pub answers: std::cell::RefCell<std::collections::VecDeque<Option<PathBuf>>>,
    pub calls: std::cell::RefCell<Vec<String>>,
}

impl FakeOs {
    pub fn answering(answers: impl IntoIterator<Item = Option<PathBuf>>) -> Self {
        Self {
            answers: std::cell::RefCell::new(answers.into_iter().collect()),
            calls: std::cell::RefCell::default(),
        }
    }

    fn answer(&self, call: String) -> Option<PathBuf> {
        self.calls.borrow_mut().push(call);
        self.answers.borrow_mut().pop_front().flatten()
    }
}

impl Os for FakeOs {
    fn pick_folder(&self, title: &str, _start: Option<&Path>) -> Option<PathBuf> {
        self.answer(format!("pick_folder: {title}"))
    }

    fn pick_file(&self, title: &str, _filters: &[FileFilter]) -> Option<PathBuf> {
        self.answer(format!("pick_file: {title}"))
    }

    fn save_file(&self, title: &str, default: &Path, _filters: &[FileFilter]) -> Option<PathBuf> {
        self.answer(format!("save_file: {title} [{}]", default.display()))
    }

    fn open_path(&self, path: &Path) -> std::io::Result<()> {
        self.calls
            .borrow_mut()
            .push(format!("open: {}", path.display()));
        Ok(())
    }

    fn reveal_path(&self, path: &Path) -> std::io::Result<()> {
        self.calls
            .borrow_mut()
            .push(format!("reveal: {}", path.display()));
        Ok(())
    }
}

/// Lets a test keep a handle on the `FakeOs` it gives the app.
impl<T: Os> Os for std::rc::Rc<T> {
    fn pick_folder(&self, title: &str, start: Option<&Path>) -> Option<PathBuf> {
        (**self).pick_folder(title, start)
    }

    fn pick_file(&self, title: &str, filters: &[FileFilter]) -> Option<PathBuf> {
        (**self).pick_file(title, filters)
    }

    fn save_file(&self, title: &str, default: &Path, filters: &[FileFilter]) -> Option<PathBuf> {
        (**self).save_file(title, default, filters)
    }

    fn open_path(&self, path: &Path) -> std::io::Result<()> {
        (**self).open_path(path)
    }

    fn reveal_path(&self, path: &Path) -> std::io::Result<()> {
        (**self).reveal_path(path)
    }
}
