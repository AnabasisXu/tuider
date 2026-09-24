//! Scan directories for readable docs (md/txt/mdx/scripts).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// `(display_name, absolute_or_given path)` sorted by display name (case-insensitive).
pub fn scan_docs(root: &Path, recursive: bool) -> Vec<(String, PathBuf)> {
    let mut docs = Vec::new();
    if root.is_file() {
        if is_doc(root) {
            let name = root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| root.to_string_lossy().into_owned());
            docs.push((name, root.to_path_buf()));
        }
        return display_names(docs);
    }
    if root.is_dir() {
        walk(root, root, recursive, &mut docs);
    }
    display_names(docs)
}

fn walk(root: &Path, dir: &Path, recursive: bool, out: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if recursive {
                walk(root, &path, true, out);
            }
        } else if is_doc(&path)
            && let Ok(relative) = path.strip_prefix(root)
        {
            out.push((relative.to_string_lossy().replace('\\', "/"), path));
        }
    }
}

/// True for md, txt, mdx, org, and script/code extensions.
pub fn is_doc(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        e.eq_ignore_ascii_case("md")
            || e.eq_ignore_ascii_case("txt")
            || e.eq_ignore_ascii_case("mdx")
            || e.eq_ignore_ascii_case("org")
            || is_code_ext(e)
    })
}

/// Source / script extensions highlighted by core `code` module.
pub fn is_code_ext(ext: &str) -> bool {
    CODE_EXTS.iter().any(|x| ext.eq_ignore_ascii_case(x))
}

const CODE_EXTS: &[&str] = &[
    "rs", "py", "go", "js", "ts", "tsx", "jsx", "c", "h", "cpp", "hpp", "java", "kt", "swift",
    "rb", "php", "cs", "sh", "bash", "zsh", "fish", "toml", "yaml", "yml", "json", "html", "css",
    "sql", "lua", "vim", "zig",
];

/// Collision-safe display names: stem, or `stem (parent)` when stems collide.
fn display_names(entries: Vec<(String, PathBuf)>) -> Vec<(String, PathBuf)> {
    let mut counts = HashMap::new();
    for (_, path) in &entries {
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            *counts.entry(stem.to_lowercase()).or_insert(0usize) += 1;
        }
    }

    let mut named: Vec<_> = entries
        .into_iter()
        .filter_map(|(relative, path)| {
            let stem = path.file_stem()?.to_string_lossy().into_owned();
            let name = if counts.get(&stem.to_lowercase()).copied().unwrap_or(0) > 1 {
                let parent = Path::new(&relative)
                    .parent()
                    .and_then(Path::file_name)
                    .map_or_else(|| "root".to_string(), |n| n.to_string_lossy().into_owned());
                format!("{stem} ({parent})")
            } else {
                stem
            };
            Some((name, path))
        })
        .collect();
    named.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    named
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tuider-scan-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn empty_dir() {
        let dir = tmp("empty");
        assert!(scan_docs(&dir, false).is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn mixed_extensions_includes_mdx_and_scripts() {
        let dir = tmp("mix");
        fs::write(dir.join("a.md"), "# a").unwrap();
        fs::write(dir.join("b.txt"), "b").unwrap();
        fs::write(dir.join("c.mdx"), "x").unwrap();
        fs::write(dir.join("d.rs"), "fn").unwrap();
        fs::write(dir.join("e.bin"), "\0\0").unwrap();
        let names: Vec<_> = scan_docs(&dir, false).into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, vec!["a", "b", "c", "d"]);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn recursive_flag() {
        let dir = tmp("rec");
        fs::create_dir_all(dir.join("nested")).unwrap();
        fs::write(dir.join("top.md"), "t").unwrap();
        fs::write(dir.join("nested/deep.md"), "d").unwrap();

        let flat: Vec<_> = scan_docs(&dir, false).into_iter().map(|(n, _)| n).collect();
        assert_eq!(flat, vec!["top"]);

        let deep: Vec<_> = scan_docs(&dir, true).into_iter().map(|(n, _)| n).collect();
        assert_eq!(deep, vec!["deep", "top"]);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn stem_collision_disambiguates() {
        let dir = tmp("col");
        fs::create_dir_all(dir.join("a")).unwrap();
        fs::create_dir_all(dir.join("b")).unwrap();
        fs::write(dir.join("a/intro.md"), "1").unwrap();
        fs::write(dir.join("b/intro.md"), "2").unwrap();
        let names: Vec<_> = scan_docs(&dir, true).into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, vec!["intro (a)", "intro (b)"]);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn org_is_doc() {
        assert!(is_doc(Path::new("notes.org")));
        assert!(!is_doc(Path::new("notes.org.bak")));
    }

    #[test]
    fn single_file() {
        let dir = tmp("file");
        let path = dir.join("note.md");
        fs::write(&path, "hi").unwrap();
        let docs = scan_docs(&path, false);
        assert_eq!(docs.len(), 1);
        // display_names uses stem, same as directory entries
        assert_eq!(docs[0].0, "note");
        let _ = fs::remove_dir_all(dir);
    }
}
