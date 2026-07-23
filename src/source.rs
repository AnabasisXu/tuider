//! Local md/txt document source.

use std::path::{Path, PathBuf};

use ratatui::text::Line;

use crate::md;
use crate::plugin::{ContentSource, LoadResult};

pub struct FileTreeSource {
    title: String,
    names: Vec<String>,
    paths: Vec<PathBuf>,
}

impl FileTreeSource {
    pub fn new(docs: Vec<(String, PathBuf)>) -> Self {
        let (names, paths): (Vec<_>, Vec<_>) = docs.into_iter().unzip();
        Self {
            title: "docs".into(),
            names,
            paths,
        }
    }
}

impl ContentSource for FileTreeSource {
    fn title(&self) -> &str {
        &self.title
    }

    fn entries(&self) -> &[String] {
        &self.names
    }

    fn entry_path(&self, index: usize) -> Option<PathBuf> {
        self.paths.get(index).cloned()
    }

    fn ensure_local_doc(&mut self, path: &Path) -> Option<usize> {
        // ponytail: relative f-links stay in-app; never xdg-open .md/.txt
        if !path.is_file() {
            return None;
        }
        let is_doc = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("txt"));
        if !is_doc {
            return None;
        }
        let canon = path.canonicalize().ok();
        for (i, p) in self.paths.iter().enumerate() {
            if p == path {
                return Some(i);
            }
            if let (Some(c), Ok(pc)) = (&canon, p.canonicalize()) {
                if *c == pc {
                    return Some(i);
                }
            }
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        self.names.push(name);
        self.paths.push(canon.unwrap_or_else(|| path.to_path_buf()));
        Some(self.paths.len() - 1)
    }

    fn load(&mut self, index: usize, width: usize) -> LoadResult {
        let Some(path) = self.paths.get(index) else {
            return LoadResult::plain(vec![Line::from("out of range")], "invalid index".into());
        };
        let name = self.names.get(index).map(|s| s.as_str()).unwrap_or("?");
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                return LoadResult::plain(
                    vec![Line::from(format!("error: {e}"))],
                    format!("failed: {name}"),
                );
            }
        };
        let is_md = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("md"));
        if is_md {
            let doc = md::render_md_doc(&text, width.max(20));
            LoadResult {
                status: format!("{name}  ({} lines)", doc.lines.len()),
                lines: doc.lines,
                links: doc.links,
                headings: doc.headings,
            }
        } else {
            // code-like / plain: host outline scanner lives in loader; reuse via md lines
            let lines = md::render_txt_width(&text, width.max(20));
            let headings = crate::loader::outline_from_plain_lines(&lines);
            LoadResult {
                lines,
                status: format!("{name}  ({} lines)", text.lines().count()),
                links: Vec::new(),
                headings,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::ContentSource;

    #[test]
    fn ensure_local_doc_appends_md() {
        let readme = PathBuf::from("README.md");
        assert!(readme.is_file(), "run from repo root");
        let mut src = FileTreeSource::new(vec![("README.md".into(), readme.clone())]);
        assert_eq!(src.entries().len(), 1);
        let status = PathBuf::from("docs/STATUS.md");
        assert!(status.is_file());
        let i = src.ensure_local_doc(&status).expect("append status");
        assert_eq!(i, 1);
        assert_eq!(src.entries().len(), 2);
        // second call is idempotent
        assert_eq!(src.ensure_local_doc(&status), Some(1));
    }
}
