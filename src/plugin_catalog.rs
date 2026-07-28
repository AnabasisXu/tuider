//! Single source of truth for shippable plugin ids, .so names, and host CLI claims.

/// One installable / claimable plugin.
#[derive(Clone, Copy)]
pub struct CatalogEntry {
    pub id: &'static str,
    pub crate_name: &'static str,
    pub so_name: &'static str,
    pub summary: &'static str,
    /// Host fallback when plugin `handles` is absent/false; also drives missing-so hints.
    pub claims: fn(args: &[String]) -> bool,
}

fn claims_url(args: &[String]) -> bool {
    // ponytail: drop -u/--url; bare http(s) only
    args.iter()
        .any(|a| a.starts_with("http://") || a.starts_with("https://"))
}

fn claims_hn(args: &[String]) -> bool {
    args.iter().any(|a| a == "-hn" || a == "--hn")
}

fn claims_dict(args: &[String]) -> bool {
    args.iter().any(|a| a == "-g" || a == "--group")
        || args.iter().any(|a| a.ends_with(".mdx") || a.ends_with(".MDX"))
    // -s alone does not claim; needs -g or .mdx
}

fn claims_epub(args: &[String]) -> bool {
    args.iter().any(|a| {
        a == "-e"
            || a == "--epub"
            || a.ends_with(".epub")
            || a.ends_with(".EPUB")
    })
}

/// Static catalog (order = missing-hint priority: url → hn → dict → epub).
/// Code reading is built into core (not a plugin).
pub const CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        id: "url",
        crate_name: "tuider-plugin-url",
        so_name: "libtuider_url.so",
        summary: "fetch URL → markdown",
        claims: claims_url,
    },
    CatalogEntry {
        id: "hn",
        crate_name: "tuider-plugin-hn",
        so_name: "libtuider_hn.so",
        summary: "Hacker News top stories",
        claims: claims_hn,
    },
    CatalogEntry {
        id: "dict",
        crate_name: "tuider-plugin-dict",
        so_name: "libtuider_dict.so",
        summary: "MDX dictionary",
        claims: claims_dict,
    },
    CatalogEntry {
        id: "epub",
        crate_name: "tuider-plugin-epub",
        so_name: "libtuider_epub.so",
        summary: "EPUB book → chapters",
        claims: claims_epub,
    },
];

pub fn find(id: &str) -> Option<&'static CatalogEntry> {
    CATALOG.iter().find(|e| e.id == id)
}

pub fn claims(id: &str, args: &[String]) -> bool {
    find(id).is_some_and(|e| (e.claims)(args))
}

/// First catalog entry that claims `args` but is not loaded.
///
/// Historical note: old `missing_plugin_hint` used `starts_with("http")` for url.
/// Spec freezes **claims** table (`http://` / `https://`). Hints use the same
/// `claims` predicates so there is one knowledge source.
pub fn missing_plugin_hint(args: &[String], loaded: impl Fn(&str) -> bool) -> Option<&'static str> {
    for e in CATALOG {
        if (e.claims)(args) && !loaded(e.id) {
            return Some(e.id);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(args: &[&str]) -> Vec<String> {
        args.iter().map(|a| (*a).to_string()).collect()
    }

    #[test]
    fn claims_url_bare_https_only() {
        assert!(!claims("url", &s(&["-u"])));
        assert!(!claims("url", &s(&["--url"])));
        assert!(!claims("url", &s(&["--url", "not-a-url"])));
        // -u is ignored; bare URL still claims
        assert!(claims("url", &s(&["-u", "https://x"])));
        assert!(claims("url", &s(&["https://example.com"])));
        assert!(claims("url", &s(&["http://example.com"])));
        assert!(!claims("url", &s(&["README.md"])));
        assert!(!claims("url", &s(&["http"]))); // no ://
    }

    #[test]
    fn claims_hn_dict() {
        assert!(claims("hn", &s(&["-hn"])));
        assert!(claims("hn", &s(&["--hn"])));
        assert!(!claims("hn", &s(&["-u"])));
        assert!(claims("dict", &s(&["-g", "en"])));
        assert!(claims("dict", &s(&["--group", "en"])));
        assert!(claims("dict", &s(&["foo.mdx"])));
        assert!(claims("dict", &s(&["FOO.MDX"])));
        assert!(!claims("dict", &s(&["foo.md"])));
        assert!(!claims("dict", &s(&["src/ai.rs"])));
    }

    #[test]
    fn claims_epub_paths() {
        assert!(claims("epub", &s(&["book.epub"])));
        assert!(claims("epub", &s(&["-e", "x"])));
        assert!(claims("epub", &s(&["--epub", "x"])));
        assert!(claims("epub", &s(&["BOOK.EPUB"])));
        assert!(!claims("epub", &s(&["README.md"])));
        assert_eq!(missing_plugin_hint(&s(&["novel.epub"]), |_: &str| false), Some("epub"));
    }

    #[test]
    fn missing_hint_order_and_loaded() {
        let none = |_: &str| false;
        assert_eq!(
            missing_plugin_hint(&s(&["https://x"]), none),
            Some("url")
        );
        // -u alone no longer claims url
        assert_eq!(missing_plugin_hint(&s(&["-u"]), none), None);
        assert_eq!(missing_plugin_hint(&s(&["-hn"]), none), Some("hn"));
        // code is core — no missing-plugin hint
        assert_eq!(missing_plugin_hint(&s(&["--code"]), none), None);
        assert_eq!(missing_plugin_hint(&s(&["src/ai.rs"]), none), None);
        // bare url still first when both present
        assert_eq!(
            missing_plugin_hint(&s(&["-hn", "https://x"]), none),
            Some("url")
        );
        let has_url = |id: &str| id == "url";
        assert_eq!(
            missing_plugin_hint(&s(&["https://x"]), has_url),
            None
        );
        assert_eq!(missing_plugin_hint(&s(&["README.md"]), none), None);
    }

    #[test]
    fn catalog_ids_unique() {
        let mut ids: Vec<_> = CATALOG.iter().map(|e| e.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), CATALOG.len());
    }
}
