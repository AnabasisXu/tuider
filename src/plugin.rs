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
    /// Multi-dict / layer cycle (plugin optional). Returns true if layer changed.
    fn cycle_layer(&mut self) -> bool {
        false
    }
}

pub fn ai_compiled() -> bool {
    cfg!(feature = "ai")
}
