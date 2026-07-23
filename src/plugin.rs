//! Host-side ContentSource (always compiled).
//! Dynamic plugins adapt into this via `loader::HostSource`.

use ratatui::text::Line;

#[derive(Debug, Clone)]
pub struct LinkEntry {
    pub text: String,
    pub url: String,
    /// Approx body line for jump (0-based).
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct HeadingEntry {
    pub level: u8,
    pub text: String,
    pub line: usize,
}

pub struct LoadResult {
    pub lines: Vec<Line<'static>>,
    pub status: String,
    pub links: Vec<LinkEntry>,
    pub headings: Vec<HeadingEntry>,
}

impl LoadResult {
    pub fn plain(lines: Vec<Line<'static>>, status: String) -> Self {
        Self {
            lines,
            status,
            links: Vec::new(),
            headings: Vec::new(),
        }
    }
}

pub trait ContentSource: Send {
    fn title(&self) -> &str;
    fn entries(&self) -> &[String];
    fn load(&mut self, index: usize, width: usize) -> LoadResult;
    /// Path of entry if this is a local file source (for relative links / open dir).
    fn entry_path(&self, _index: usize) -> Option<std::path::PathBuf> {
        None
    }
    /// If `path` is a local doc this source can open, return entry index.
    /// FileTreeSource may append md/txt not yet in the list; others only match existing.
    fn ensure_local_doc(&mut self, _path: &std::path::Path) -> Option<usize> {
        None
    }
    /// Multi-dict / layer cycle (plugin optional). Returns true if layer changed.
    fn cycle_layer(&mut self) -> bool {
        false
    }
    /// Optional plugin action (`article`, …). True → host should reload body.
    fn action(&mut self, _index: usize, _action: &str) -> bool {
        false
    }
    /// True if plugin exports `tuider_source_action` (e.g. HN `a`).
    fn has_action(&self) -> bool {
        false
    }
    /// Dict tools: (dict_title, definition_text) pairs.
    fn lookup_word(&mut self, _word: &str) -> Vec<(String, String)> {
        Vec::new()
    }
    fn search_headwords(&mut self, _prefix: &str, _limit: usize) -> Vec<String> {
        Vec::new()
    }
    fn reverse_lookup(&mut self, _query: &str, _limit: usize) -> Vec<String> {
        Vec::new()
    }
    fn list_dicts(&self) -> Vec<String> {
        Vec::new()
    }
    fn select_dict(&mut self, _index: usize) -> bool {
        false
    }
    /// Plain-ish body for clipboard / AI (no ANSI).
    fn plain_body(&mut self, index: usize) -> String {
        let r = self.load(index, 100);
        r.lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

pub fn ai_compiled() -> bool {
    cfg!(feature = "ai")
}
