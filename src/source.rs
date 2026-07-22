//! Local md/txt document source.

use std::path::PathBuf;

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
            let lines = md::render_txt_width(&text, width.max(20));
            LoadResult::plain(lines, format!("{name}  ({} lines)", text.lines().count()))
        }
    }
}
