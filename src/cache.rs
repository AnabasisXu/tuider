//! Disk cache under `$XDG_CACHE_HOME/tuider` or `~/.cache/tuider`.
//! Used by url/hn plugins; always compiled so helpers are available.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub fn cache_root() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("tuider");
        }
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".cache").join("tuider")
}

fn ensure_dir(p: &Path) -> std::io::Result<()> {
    fs::create_dir_all(p)
}

/// Stable hex key from bytes (FNV-1a 64).
pub fn hash_key(data: &[u8]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in data {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

pub fn get_text(rel: &str, max_age: Option<Duration>) -> Option<String> {
    let path = cache_root().join(rel);
    let meta = fs::metadata(&path).ok()?;
    if let Some(max) = max_age {
        let modified = meta.modified().ok()?;
        let age = SystemTime::now().duration_since(modified).ok()?;
        if age > max {
            return None;
        }
    }
    fs::read_to_string(path).ok()
}

pub fn put_text(rel: &str, text: &str) -> std::io::Result<()> {
    let path = cache_root().join(rel);
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    fs::write(path, text)
}

pub fn page_rel(url: &str) -> String {
    format!("pages/{}.md", hash_key(url.as_bytes()))
}

pub fn hn_item_rel(id: u64) -> String {
    format!("hn/item-{id}.json")
}

pub fn hn_top_rel() -> &'static str {
    "hn/topstories.json"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_stable() {
        assert_eq!(hash_key(b"abc"), hash_key(b"abc"));
        assert_ne!(hash_key(b"a"), hash_key(b"b"));
    }

    #[test]
    fn put_get_roundtrip() {
        let rel = format!("test/{}.txt", std::process::id());
        put_text(&rel, "hello").unwrap();
        assert_eq!(get_text(&rel, None).as_deref(), Some("hello"));
        let _ = fs::remove_file(cache_root().join(&rel));
    }
}
