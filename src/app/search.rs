//! In-content `/` search: char-level match list + jump.

use ratatui::text::Line;

use super::App;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchHit {
    pub line: usize,
    /// Inclusive start char index in plain line text.
    pub start: usize,
    /// Exclusive end char index.
    pub end: usize,
}

pub fn find_hits(body: &[Line<'static>], query: &str) -> Vec<MatchHit> {
    // ponytail: whitespace tokens → each token's real substrings (orderless)
    let tokens: Vec<Vec<char>> = query
        .split_whitespace()
        .filter(|t| !t.is_empty())
        .map(|t| t.to_lowercase().chars().collect())
        .collect();
    if tokens.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (li, line) in body.iter().enumerate() {
        let text = App::line_plain(line);
        let lower: Vec<char> = text
            .chars()
            .map(|c| c.to_lowercase().next().unwrap_or(c))
            .collect();
        let n = lower.len();
        for q in &tokens {
            let m = q.len();
            if m == 0 || m > n {
                continue;
            }
            let mut i = 0;
            while i + m <= n {
                if lower[i..i + m] == q[..] {
                    out.push(MatchHit {
                        line: li,
                        start: i,
                        end: i + m,
                    });
                    i += m; // non-overlapping; simple
                } else {
                    i += 1;
                }
            }
        }
    }
    out.sort_by_key(|h| (h.line, h.start));
    out
}

impl App {
    pub fn match_hits(&self) -> Vec<MatchHit> {
        find_hits(&self.body, &self.vim_query)
    }

    /// Current hit for stronger highlight, if any.
    pub fn current_match(&self) -> Option<MatchHit> {
        let hits = self.match_hits();
        if hits.is_empty() {
            return None;
        }
        Some(hits[self.vim_match_idx % hits.len()])
    }

    pub(crate) fn jump_to_match(&mut self, which: usize) {
        let hits = self.match_hits();
        if hits.is_empty() {
            self.status = format!("no match: /{}", self.vim_query);
            return;
        }
        let idx = which % hits.len();
        self.vim_match_idx = idx;
        let hit = hits[idx];
        self.ensure_line_visible(hit.line);
        // if match above viewport top after ensure, still ok
        if (self.scroll as usize) > hit.line {
            self.scroll = hit.line as u16;
        }
        self.status = format!("/{}  {}/{}", self.vim_query, idx + 1, hits.len());
    }

    pub(crate) fn vim_next(&mut self, dir: isize) {
        let hits = self.match_hits();
        if hits.is_empty() {
            self.status = format!("no match: /{}", self.vim_query);
            return;
        }
        let n = hits.len() as isize;
        let next = (self.vim_match_idx as isize + dir).rem_euclid(n) as usize;
        self.jump_to_match(next);
    }

    pub(crate) fn handle_vim_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crossterm::event::{KeyCode, KeyModifiers};
        match key.code {
            KeyCode::Esc => {
                self.vim_mode = false;
                self.vim_input.clear();
            }
            KeyCode::Enter => {
                self.vim_query = self.vim_input.clone();
                self.vim_mode = false;
                self.vim_match_idx = 0;
                self.jump_to_match(0);
                if self.vim_query.is_empty() {
                    self.status = "search cleared".into();
                }
            }
            KeyCode::Backspace => {
                self.vim_input.pop();
            }
            KeyCode::Char(c)
                if key.modifiers == KeyModifiers::NONE || key.modifiers == KeyModifiers::SHIFT =>
            {
                if !c.is_control() {
                    self.vim_input.push(c);
                }
            }
            _ => {}
        }
        false
    }
}
