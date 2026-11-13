//! Keyword tags (T-10-04): a free-form tag set assignable to a Sheet — independent of the
//! existing `Filter` model (T-1-06), which is a *saved query*, not a tag a sheet carries
//! directly. Per inventory §15's confirmed `editKeywords:`/`showKeywordManager:` UI surface
//! (wired in T-10-05).
//!
//! `Sheet::keywords` is a plain `Vec<String>`, not a dedicated wrapper type — it serializes/
//! derives exactly like every other field that way. These free functions are the only place the
//! "no duplicates" invariant is enforced, so callers never need to re-check it themselves.

/// Assigns `keyword` to `keywords`, doing nothing if it's already present.
pub fn assign(keywords: &mut Vec<String>, keyword: &str) {
    if !has(keywords, keyword) {
        keywords.push(keyword.to_string());
    }
}

/// Removes `keyword` from `keywords`, if present.
pub fn unassign(keywords: &mut Vec<String>, keyword: &str) {
    keywords.retain(|k| k != keyword);
}

/// Whether `keywords` contains `keyword` (exact match — Keywords are user-defined labels, not a
/// substring search like `Filter::keyword`).
pub fn has(keywords: &[String], keyword: &str) -> bool {
    keywords.iter().any(|k| k == keyword)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assign_and_query() {
        let mut keywords = Vec::new();
        assert!(!has(&keywords, "todo"));

        assign(&mut keywords, "todo");
        assert!(has(&keywords, "todo"));
        assert_eq!(keywords, vec!["todo".to_string()]);

        // Assigning the same keyword twice doesn't duplicate it.
        assign(&mut keywords, "todo");
        assert_eq!(keywords.len(), 1);

        assign(&mut keywords, "draft");
        assert!(has(&keywords, "draft"));
        assert_eq!(keywords.len(), 2);

        unassign(&mut keywords, "todo");
        assert!(!has(&keywords, "todo"));
        assert!(has(&keywords, "draft"));
        assert_eq!(keywords.len(), 1);
    }
}
