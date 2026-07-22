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
    args.iter().any(|a| {
        a == "-u" || a == "--url" || a.starts_with("http://") || a.starts_with("https://")
    })
}

fn claims_hn(args: &[String]) -> bool {
    args.iter().any(|a| a == "-hn" || a == "--hn")
}

fn claims_code(args: &[String]) -> bool {
    args.iter().any(|a| a == "--code")
}

fn claims_dict(args: &[String]) -> bool {
    args.iter().any(|a| a == "-g" || a == "--group")
        || args.iter().any(|a| a.ends_with(".mdx") || a.ends_with(".MDX"))
}

/// Static catalog (order = missing-hint priority: url → hn → code → dict).
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
        id: "code",
        crate_name: "tuider-plugin-code",
        so_name: "libtuider_code.so",
        summary: "source file tree",
        claims: claims_code,
    },
    CatalogEntry {
        id: "dict",
        crate_name: "tuider-plugin-dict",
        so_name: "libtuider_dict.so",
        summary: "MDX dictionary",
        claims: claims_dict,
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
    fn claims_url_flags_and_urls() {
        assert!(claims("url", &s(&["-u", "https://x"])));
        assert!(claims("url", &s(&["--url", "http://x"])));
        assert!(claims("url", &s(&["https://example.com"])));
        assert!(claims("url", &s(&["http://example.com"])));
        assert!(!claims("url", &s(&["README.md"])));
        assert!(!claims("url", &s(&["http"]))); // no ://
    }

    #[test]
    fn claims_hn_code_dict() {
        assert!(claims("hn", &s(&["-hn"])));
        assert!(claims("hn", &s(&["--hn"])));
        assert!(!claims("hn", &s(&["-u"])));
        assert!(claims("code", &s(&["--code", "src"])));
        assert!(!claims("code", &s(&["-c"])));
        assert!(claims("dict", &s(&["-g", "en"])));
        assert!(claims("dict", &s(&["--group", "en"])));
        assert!(claims("dict", &s(&["foo.mdx"])));
        assert!(claims("dict", &s(&["FOO.MDX"])));
        assert!(!claims("dict", &s(&["foo.md"])));
    }

    #[test]
    fn missing_hint_order_and_loaded() {
        let none = |_: &str| false;
        assert_eq!(
            missing_plugin_hint(&s(&["-u", "https://x"]), none),
            Some("url")
        );
        assert_eq!(missing_plugin_hint(&s(&["-hn"]), none), Some("hn"));
        assert_eq!(missing_plugin_hint(&s(&["--code"]), none), Some("code"));
        assert_eq!(missing_plugin_hint(&s(&["a.mdx"]), none), Some("dict"));
        // url claims first when both present
        assert_eq!(
            missing_plugin_hint(&s(&["-hn", "-u"]), none),
            Some("url")
        );
        let has_url = |id: &str| id == "url";
        assert_eq!(
            missing_plugin_hint(&s(&["-u", "https://x"]), has_url),
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
