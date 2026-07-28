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

/// `zz` avy: type query → label hits → jump (evil-avy-goto-char-timer spirit).
#[derive(Debug, Clone)]
pub struct AvyState {
    pub query: String,
    /// None = typing query; Some = pick label (may be empty → closed).
    pub labels: Option<Vec<(String, usize, usize)>>,
    pub label_buf: String,
}

/// Case-insensitive contiguous substring hits (non-overlapping).
pub fn find_avy_hits(body: &[Line<'static>], query: &str) -> Vec<MatchHit> {
    let q: Vec<char> = query.to_lowercase().chars().collect();
    if q.is_empty() {
        return Vec::new();
    }
    let m = q.len();
    let mut out = Vec::new();
    for (li, line) in body.iter().enumerate() {
        let text = App::line_plain(line);
        let lower: Vec<char> = text
            .chars()
            .map(|c| c.to_lowercase().next().unwrap_or(c))
            .collect();
        let n = lower.len();
        if m > n {
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
                i += m;
            } else {
                i += 1;
            }
        }
    }
    out
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
        self.caret_line = hit.line.min(self.body.len().saturating_sub(1));
        self.caret_col = hit.start.min(self.line_len(self.caret_line));
        self.center_line_in_view(self.caret_line);
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
                self.vim_hist_idx = None;
            }
            KeyCode::Enter => {
                self.vim_query = self.vim_input.clone();
                let q = self.vim_query.clone();
                self.push_vim_history(&q);
                self.vim_mode = false;
                self.vim_match_idx = 0;
                self.jump_to_match(0);
                if self.vim_query.is_empty() {
                    self.status = "search cleared".into();
                }
            }
            KeyCode::Up => self.vim_history_step(-1),
            KeyCode::Down => self.vim_history_step(1),
            KeyCode::Backspace => {
                self.vim_input.pop();
                self.vim_hist_idx = None;
            }
            KeyCode::Char(c)
                if key.modifiers == KeyModifiers::NONE || key.modifiers == KeyModifiers::SHIFT =>
            {
                if !c.is_control() {
                    self.vim_input.push(c);
                    self.vim_hist_idx = None;
                }
            }
            _ => {}
        }
        false
    }

    fn push_vim_history(&mut self, q: &str) {
        let q = q.trim();
        if q.is_empty() {
            return;
        }
        self.vim_history.retain(|h| h != q);
        self.vim_history.push(q.to_string());
        if self.vim_history.len() > 50 {
            self.vim_history.remove(0);
        }
        self.vim_hist_idx = None;
    }

    /// dir -1 = older (↑), +1 = newer (↓).
    fn vim_history_step(&mut self, dir: isize) {
        if self.vim_history.is_empty() {
            return;
        }
        let n = self.vim_history.len();
        let next = match self.vim_hist_idx {
            None if dir < 0 => Some(n - 1),
            None => return,
            Some(i) => {
                let j = i as isize + dir;
                if j < 0 {
                    Some(0)
                } else if j >= n as isize {
                    self.vim_hist_idx = None;
                    self.vim_input.clear();
                    return;
                } else {
                    Some(j as usize)
                }
            }
        };
        if let Some(i) = next {
            self.vim_hist_idx = Some(i);
            self.vim_input = self.vim_history[i].clone();
        }
    }

    pub fn avy_query_mode(&self) -> bool {
        self.avy.as_ref().is_some_and(|a| a.labels.is_none())
    }

    pub fn avy_query(&self) -> &str {
        self.avy.as_ref().map(|a| a.query.as_str()).unwrap_or("")
    }

    pub fn avy_labels(&self) -> Option<&[(String, usize, usize)]> {
        self.avy.as_ref().and_then(|a| a.labels.as_deref())
    }

    pub fn avy_label_buf(&self) -> &str {
        self.avy.as_ref().map(|a| a.label_buf.as_str()).unwrap_or("")
    }

    pub(crate) fn start_avy(&mut self) {
        if self.body.is_empty() {
            self.status = "zz: empty document".into();
            return;
        }
        self.pending_z = false;
        self.line_jump = None;
        self.line_jump_buf.clear();
        self.avy = Some(AvyState {
            query: String::new(),
            labels: None,
            label_buf: String::new(),
        });
        self.status = "zz — type · Enter labels · Esc".into();
    }

    pub(crate) fn close_avy(&mut self) {
        self.avy = None;
    }

    pub(crate) fn handle_avy_key(&mut self, key: crossterm::event::KeyEvent) {
        use crossterm::event::{KeyCode, KeyModifiers};
        let Some(avy) = self.avy.as_mut() else {
            return;
        };
        // label phase
        if avy.labels.is_some() {
            match key.code {
                KeyCode::Esc => {
                    self.close_avy();
                    self.status = "zz cancelled".into();
                }
                KeyCode::Backspace => {
                    if let Some(a) = self.avy.as_mut() {
                        a.label_buf.pop();
                        self.status = if a.label_buf.is_empty() {
                            format!("zz labels /{} — type label", a.query)
                        } else {
                            format!("zz — {}…", a.label_buf)
                        };
                    }
                }
                KeyCode::Char(c)
                    if key.modifiers == KeyModifiers::NONE
                        || key.modifiers == KeyModifiers::SHIFT =>
                {
                    let c = c.to_ascii_lowercase();
                    if !c.is_ascii_alphabetic() {
                        return;
                    }
                    let Some(a) = self.avy.as_mut() else {
                        return;
                    };
                    a.label_buf.push(c);
                    let buf = a.label_buf.clone();
                    let labels = a.labels.clone().unwrap_or_default();
                    if let Some((_, line, col)) = labels.iter().find(|(l, _, _)| l == &buf) {
                        let (line, col) = (*line, *col);
                        self.close_avy();
                        self.body_caret_shown = true;
                        self.set_caret(line, col);
                        self.status = format!("zz → {}:{}", line + 1, col + 1);
                    } else if labels.iter().any(|(l, _, _)| l.starts_with(&buf)) {
                        self.status = format!("zz — {buf}…");
                    } else {
                        self.close_avy();
                        self.status = "zz cancelled".into();
                    }
                }
                _ => {
                    self.close_avy();
                    self.status = "zz cancelled".into();
                }
            }
            return;
        }

        // query phase
        match key.code {
            KeyCode::Esc => {
                self.close_avy();
                self.status = "zz cancelled".into();
            }
            KeyCode::Enter => {
                self.avy_commit_query();
            }
            KeyCode::Backspace => {
                if let Some(a) = self.avy.as_mut() {
                    a.query.pop();
                    self.status = format!("zz — {}", a.query.replace(' ', "·"));
                }
            }
            KeyCode::Char(c)
                if !c.is_control()
                    && (key.modifiers == KeyModifiers::NONE
                        || key.modifiers == KeyModifiers::SHIFT
                        || c == ' ') =>
            {
                if let Some(a) = self.avy.as_mut() {
                    a.query.push(c);
                    self.status = format!("zz — {}", a.query.replace(' ', "·"));
                }
            }
            _ => {}
        }
    }

    fn avy_commit_query(&mut self) {
        let Some(avy) = self.avy.as_ref() else {
            return;
        };
        let q = avy.query.clone();
        if q.is_empty() {
            self.close_avy();
            self.status = "zz cancelled".into();
            return;
        }
        let hits = find_avy_hits(&self.body, &q);
        if hits.is_empty() {
            self.close_avy();
            self.status = format!("zz no match: {q}");
            return;
        }
        if hits.len() == 1 {
            let h = hits[0];
            self.close_avy();
            self.body_caret_shown = true;
            self.set_caret(h.line, h.start);
            self.status = format!("zz → {}:{}  /{q}", h.line + 1, h.start + 1);
            return;
        }
        let strings = Self::line_jump_label_strings(hits.len());
        let labels: Vec<(String, usize, usize)> = strings
            .into_iter()
            .zip(hits.iter())
            .map(|(s, h)| (s, h.line, h.start))
            .collect();
        if let Some(a) = self.avy.as_mut() {
            a.label_buf.clear();
            a.labels = Some(labels);
            self.status = format!("zz {} hits /{q} — type label", a.labels.as_ref().map(|v| v.len()).unwrap_or(0));
        }
    }

    #[cfg(test)]
    pub(crate) fn avy_hit_count_for_test(body: &[Line<'static>], q: &str) -> usize {
        find_avy_hits(body, q).len()
    }
}
