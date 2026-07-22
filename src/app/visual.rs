//! Visual selection: line (`V`) and char (`v`) + OSC 52 yank.

use ratatui::text::Line;

use super::App;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualKind {
    Line,
    Char,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisualSel {
    pub kind: VisualKind,
    pub a_line: usize,
    pub a_col: usize,
    pub b_line: usize,
    pub b_col: usize,
}

impl VisualSel {
    pub fn normalized(self) -> Self {
        if self.kind == VisualKind::Line {
            return Self {
                kind: VisualKind::Line,
                a_line: self.a_line.min(self.b_line),
                a_col: 0,
                b_line: self.a_line.max(self.b_line),
                b_col: 0,
            };
        }
        let (al, ac, bl, bc) = if (self.a_line, self.a_col) <= (self.b_line, self.b_col) {
            (self.a_line, self.a_col, self.b_line, self.b_col)
        } else {
            (self.b_line, self.b_col, self.a_line, self.a_col)
        };
        Self {
            kind: VisualKind::Char,
            a_line: al,
            a_col: ac,
            b_line: bl,
            b_col: bc,
        }
    }
}

/// Plain text of body lines `start..=end`.
pub fn selected_plain(body: &[Line<'static>], start: usize, end: usize) -> String {
    let a = start.min(end);
    let b = start.max(end).min(body.len().saturating_sub(1));
    if body.is_empty() {
        return String::new();
    }
    body[a..=b]
        .iter()
        .map(line_text)
        .collect::<Vec<_>>()
        .join("\n")
}

fn line_text(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect()
}

/// Char-level selection plain text (exclusive end col on last line).
pub fn selected_plain_char(body: &[Line<'static>], sel: &VisualSel) -> String {
    let s = sel.normalized();
    if body.is_empty() {
        return String::new();
    }
    if s.a_line == s.b_line {
        let t = line_text(&body[s.a_line.min(body.len() - 1)]);
        let chars: Vec<char> = t.chars().collect();
        let a = s.a_col.min(chars.len());
        let b = s.b_col.min(chars.len()).max(a);
        return chars[a..b].iter().collect();
    }
    let mut out = String::new();
    for li in s.a_line..=s.b_line.min(body.len().saturating_sub(1)) {
        let t = line_text(&body[li]);
        let chars: Vec<char> = t.chars().collect();
        if li == s.a_line {
            let a = s.a_col.min(chars.len());
            out.push_str(&chars[a..].iter().collect::<String>());
        } else if li == s.b_line {
            let b = s.b_col.min(chars.len());
            out.push_str(&chars[..b].iter().collect::<String>());
        } else {
            out.push_str(&t);
        }
        if li != s.b_line {
            out.push('\n');
        }
    }
    out
}

impl App {
    pub(crate) fn start_visual(&mut self, kind: VisualKind) {
        if self.body.is_empty() {
            return;
        }
        let line = self.caret_line();
        let col = 0;
        self.visual = Some(VisualSel {
            kind,
            a_line: line,
            a_col: col,
            b_line: line,
            b_col: if kind == VisualKind::Char {
                1.min(self.line_len(line))
            } else {
                0
            },
        });
        self.status = match kind {
            VisualKind::Line => "VISUAL LINE — j/k · y yank · Esc".into(),
            VisualKind::Char => "VISUAL — hjkl · y yank · Esc".into(),
        };
    }

    pub(crate) fn visual_extend_line(&mut self, delta: isize) {
        let Some(v) = self.visual else {
            return;
        };
        if self.body.is_empty() {
            return;
        }
        let max = self.body.len() - 1;
        let next = (v.b_line as isize + delta).clamp(0, max as isize) as usize;
        let col = if v.kind == VisualKind::Char {
            v.b_col.min(self.line_len(next))
        } else {
            0
        };
        if let Some(v) = self.visual.as_mut() {
            v.b_line = next;
            if v.kind == VisualKind::Char {
                v.b_col = col;
            }
        }
        self.ensure_line_visible(next);
    }

    pub(crate) fn visual_extend_col(&mut self, delta: isize) {
        let Some(v) = self.visual else {
            return;
        };
        if v.kind != VisualKind::Char || self.body.is_empty() {
            return;
        }
        let len = self.line_len(v.b_line);
        let next = (v.b_col as isize + delta).clamp(0, len as isize) as usize;
        let line = v.b_line;
        if let Some(v) = self.visual.as_mut() {
            v.b_col = next;
        }
        self.ensure_line_visible(line);
    }

    pub(crate) fn yank_selection(&mut self) {
        let Some(v) = self.visual else {
            return;
        };
        let text = match v.kind {
            VisualKind::Line => {
                let a = v.a_line.min(v.b_line);
                let b = v.a_line.max(v.b_line);
                selected_plain(&self.body, a, b)
            }
            VisualKind::Char => selected_plain_char(&self.body, &v),
        };
        if text.is_empty() {
            self.status = "yank: empty selection".into();
            return;
        }
        match yank_osc52(&text) {
            Ok(()) => {
                let n = text.lines().count();
                self.status = format!("yanked {n} line(s) via OSC 52");
                self.visual = None;
            }
            Err(e) => {
                self.status = format!("yank failed: {e}");
            }
        }
    }
}

fn yank_osc52(text: &str) -> std::io::Result<()> {
    use std::io::Write;
    let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, text.as_bytes());
    let mut out = std::io::stdout();
    write!(out, "\x1b]52;c;{b64}\x07")?;
    out.flush()
}
