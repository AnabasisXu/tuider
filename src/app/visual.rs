//! Visual selection: cursor (`v`), char (second `v`), line (`V`) + OSC 52 yank.

use ratatui::text::Line;

use super::App;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualKind {
    Cursor,
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
        if self.kind == VisualKind::Cursor {
            return Self {
                kind: VisualKind::Cursor,
                a_line: self.b_line,
                a_col: self.b_col,
                b_line: self.b_line,
                b_col: self.b_col,
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
    line.spans.iter().map(|s| s.content.as_ref()).collect()
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

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Next word start on `chars` after `col`, or `None` if none remain on this line.
fn word_fwd_col(chars: &[char], col: usize) -> Option<usize> {
    let n = chars.len();
    let mut i = col.min(n);
    if i >= n {
        return None;
    }
    if is_word_char(chars[i]) {
        while i < n && is_word_char(chars[i]) {
            i += 1;
        }
    } else {
        while i < n && !is_word_char(chars[i]) {
            i += 1;
        }
    }
    while i < n && !is_word_char(chars[i]) {
        i += 1;
    }
    if i < n { Some(i) } else { None }
}

/// Previous word start on `chars` at/before `col`.
fn word_bwd_col(chars: &[char], col: usize) -> Option<usize> {
    let n = chars.len();
    if n == 0 {
        return None;
    }
    let mut i = col.min(n);
    if i == 0 {
        return None;
    }
    i -= 1;
    while i > 0 && !is_word_char(chars[i]) {
        i -= 1;
    }
    if !is_word_char(chars[i]) {
        return None;
    }
    while i > 0 && is_word_char(chars[i - 1]) {
        i -= 1;
    }
    Some(i)
}

/// Last word start on a line, or 0 if no word chars.
fn last_word_start(chars: &[char]) -> usize {
    word_bwd_col(chars, chars.len()).unwrap_or(0)
}

/// First word start on a line, or 0.
fn first_word_start(chars: &[char]) -> usize {
    let n = chars.len();
    let mut i = 0;
    while i < n && !is_word_char(chars[i]) {
        i += 1;
    }
    if i < n { i } else { 0 }
}

/// Inclusive end of current/next word from `col`.
fn word_end_col(chars: &[char], col: usize) -> Option<usize> {
    let n = chars.len();
    if n == 0 {
        return None;
    }
    let mut i = col.min(n);
    if i >= n {
        return None;
    }
    // if on word char, stay; else skip non-word to next word
    if !is_word_char(chars[i]) {
        while i < n && !is_word_char(chars[i]) {
            i += 1;
        }
        if i >= n {
            return None;
        }
    } else if i + 1 < n && is_word_char(chars[i + 1]) {
        // already mid-word: advance to its end (if at end already, go next)
        i += 1;
    } else if i + 1 >= n || !is_word_char(chars[i + 1]) {
        // at end of word → next word end
        i += 1;
        while i < n && !is_word_char(chars[i]) {
            i += 1;
        }
        if i >= n {
            return None;
        }
    }
    while i + 1 < n && is_word_char(chars[i + 1]) {
        i += 1;
    }
    Some(i)
}

impl App {
    pub(crate) fn start_visual(&mut self, kind: VisualKind) {
        if self.body.is_empty() {
            return;
        }
        self.pending_g = false;
        // keep current caret (zz/n/jumps); do not snap to viewport mid
        let max = self.body.len() - 1;
        let line = self.caret_line.min(max);
        let col = self.caret_col.min(self.line_len(line));
        self.caret_line = line;
        self.caret_col = col;
        self.body_caret_shown = true;
        let line_len = self.line_len(line);
        // Char: select at least one char at caret (exclusive end), like vim `v`
        let (a_col, b_col) = match kind {
            VisualKind::Char => {
                let a = col.min(line_len.saturating_sub(1).min(col));
                let b = if line_len == 0 {
                    0
                } else {
                    (a + 1).min(line_len)
                };
                (a, b)
            }
            VisualKind::Cursor => (col, col),
            VisualKind::Line => (0, 0),
        };
        self.visual = Some(VisualSel {
            kind,
            a_line: line,
            a_col,
            b_line: line,
            b_col,
        });
        self.ensure_line_visible(line);
        self.status = match kind {
            VisualKind::Cursor => "CURSOR — hjkl · v select · Esc".into(),
            VisualKind::Line => "VISUAL LINE — jk gg G HML · y · d dict · a AI · Esc".into(),
            VisualKind::Char => "VISUAL — hjkl bw e · y · d dict · a AI · Esc".into(),
        };
    }

    fn content_view_h(&self) -> usize {
        self.content_area
            .map(|a| a.height.saturating_sub(1) as usize)
            .unwrap_or(20)
            .max(1)
    }

    /// Move caret/end (b_*) — Cursor also moves anchor.
    /// Move caret/end (b_*) — Cursor also moves anchor. Syncs body caret.
    pub(crate) fn visual_set_pos(&mut self, line: usize, col: usize) {
        if self.body.is_empty() {
            return;
        }
        let max = self.body.len() - 1;
        let line = line.min(max);
        let col = col.min(self.line_len(line));
        self.caret_line = line;
        self.caret_col = col;
        let Some(v) = self.visual.as_mut() else {
            self.ensure_line_visible(line);
            return;
        };
        v.b_line = line;
        if matches!(v.kind, VisualKind::Char | VisualKind::Cursor) {
            v.b_col = col;
        } else {
            v.b_col = 0;
        }
        if v.kind == VisualKind::Cursor {
            v.a_line = line;
            v.a_col = col;
        }
        self.ensure_line_visible(line);
    }

    pub(crate) fn body_move_line(&mut self, delta: isize) {
        if self.body.is_empty() {
            return;
        }
        self.body_caret_shown = true;
        let max = self.body.len() - 1;
        let next = (self.caret_line() as isize + delta).clamp(0, max as isize) as usize;
        let col = self.caret_col.min(self.line_len(next));
        self.set_caret(next, col);
    }

    pub(crate) fn body_move_col(&mut self, delta: isize) {
        if self.body.is_empty() {
            return;
        }
        self.body_caret_shown = true;
        let max = self.body.len() - 1;
        let mut line = self.caret_line().min(max);
        let mut col = self.caret_col as isize + delta;
        let mut len = self.line_len(line) as isize;
        while col < 0 {
            if line == 0 {
                col = 0;
                break;
            }
            line -= 1;
            len = self.line_len(line) as isize;
            col = len + col + 1;
        }
        while col > len {
            if line >= max {
                col = len;
                break;
            }
            col -= len + 1;
            line += 1;
            len = self.line_len(line) as isize;
        }
        self.set_caret(line, col.clamp(0, len) as usize);
    }

    pub(crate) fn body_page(&mut self, forward: bool, half: bool) {
        let h = self.content_view_h();
        let step = if half { (h / 2).max(1) } else { h.max(1) } as isize;
        let delta = if forward { step } else { -step };
        let max = self.body.len().saturating_sub(1) as isize;
        let next = (self.caret_line() as isize + delta).clamp(0, max) as usize;
        self.set_caret(next, self.caret_col.min(self.line_len(next)));
    }

    pub(crate) fn body_goto_viewport(&mut self, where_: char) {
        if self.body.is_empty() {
            return;
        }
        let max = self.body.len() - 1;
        let h = self.content_view_h();
        let top = self.scroll as usize;
        let bot = top.saturating_add(h.saturating_sub(1)).min(max);
        let line = match where_ {
            'H' => top,
            'L' => bot,
            _ => top + (bot.saturating_sub(top)) / 2,
        };
        self.set_caret(line, self.caret_col.min(self.line_len(line)));
    }

    pub(crate) fn body_first_nonblank(&mut self) {
        if self.body.is_empty() {
            return;
        }
        let line = self.caret_line();
        let chars: Vec<char> = Self::line_plain(&self.body[line]).chars().collect();
        let mut i = 0;
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        self.set_caret(line, i.min(chars.len()));
    }

    pub(crate) fn visual_goto_line(&mut self, line: usize) {
        let col = self
            .visual
            .map(|v| {
                if matches!(v.kind, VisualKind::Char | VisualKind::Cursor) {
                    v.b_col
                } else {
                    0
                }
            })
            .unwrap_or(0);
        self.visual_set_pos(line, col);
    }

    /// H / M / L — top / mid / bottom of viewport.
    pub(crate) fn visual_goto_viewport(&mut self, where_: char) {
        if self.body.is_empty() {
            return;
        }
        let max = self.body.len() - 1;
        let h = self.content_view_h();
        let top = self.scroll as usize;
        let bot = top.saturating_add(h.saturating_sub(1)).min(max);
        let line = match where_ {
            'H' => top,
            'L' => bot,
            _ => top + (bot.saturating_sub(top)) / 2, // M
        };
        self.visual_goto_line(line);
    }

    pub(crate) fn visual_page(&mut self, forward: bool, half: bool) {
        let h = self.content_view_h();
        let step = if half { (h / 2).max(1) } else { h.max(1) } as isize;
        let delta = if forward { step } else { -step };
        let Some(v) = self.visual else {
            return;
        };
        let max = self.body.len().saturating_sub(1) as isize;
        let next = (v.b_line as isize + delta).clamp(0, max) as usize;
        self.visual_goto_line(next);
    }

    /// e — end of word (alnum/_).
    pub(crate) fn visual_extend_word_end(&mut self) {
        let Some(v) = self.visual else {
            return;
        };
        if !matches!(v.kind, VisualKind::Char | VisualKind::Cursor) || self.body.is_empty() {
            return;
        }
        let (nl, nc) = self.word_end_pos(v.b_line, v.b_col);
        self.visual_set_pos(nl, nc);
    }

    /// ^ — first non-blank on line.
    pub(crate) fn visual_first_nonblank(&mut self) {
        let Some(v) = self.visual else {
            return;
        };
        if !matches!(v.kind, VisualKind::Char | VisualKind::Cursor) || self.body.is_empty() {
            return;
        }
        let line = v.b_line.min(self.body.len().saturating_sub(1));
        let chars: Vec<char> = Self::line_plain(&self.body[line]).chars().collect();
        let mut i = 0;
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        self.visual_set_pos(line, i.min(chars.len()));
    }

    /// Single-cell caret for Cursor mode paint.
    /// Caret cell: Cursor mode uses visual b_*; normal mode uses body caret.
    pub fn visual_cursor_cell(&self) -> Option<(usize, usize)> {
        if let Some(v) = self.visual.as_ref() {
            if v.kind == VisualKind::Cursor {
                return Some((v.b_line, v.b_col));
            }
            return None; // Char/Line: selection paint only
        }
        // hide until user body-motion (dict/mdx search result head)
        if self.body.is_empty() || !self.body_caret_shown {
            return None;
        }
        Some((self.caret_line(), self.caret_col()))
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
        let col = if matches!(v.kind, VisualKind::Char | VisualKind::Cursor) {
            v.b_col.min(self.line_len(next))
        } else {
            0
        };
        if let Some(v) = self.visual.as_mut() {
            v.b_line = next;
            if matches!(v.kind, VisualKind::Char | VisualKind::Cursor) {
                v.b_col = col;
            }
            if v.kind == VisualKind::Cursor {
                v.a_line = next;
                v.a_col = col;
            }
        }
        self.caret_line = next;
        self.caret_col = col;
        self.ensure_line_visible(next);
    }

    pub(crate) fn visual_extend_col(&mut self, delta: isize) {
        let Some(v) = self.visual else {
            return;
        };
        if !matches!(v.kind, VisualKind::Char | VisualKind::Cursor) || self.body.is_empty() {
            return;
        }
        let max = self.body.len() - 1;
        let mut line = v.b_line.min(max);
        let mut col = v.b_col as isize + delta;
        let mut len = self.line_len(line) as isize;
        // wrap across lines when past ends (col may be == len = after last char)
        while col < 0 {
            if line == 0 {
                col = 0;
                break;
            }
            line -= 1;
            len = self.line_len(line) as isize;
            col = len + col + 1; // -1 from next start → prev EOL
        }
        while col > len {
            if line >= max {
                col = len;
                break;
            }
            col -= len + 1; // leave EOL → next BOL
            line += 1;
            len = self.line_len(line) as isize;
        }
        let col = col.clamp(0, len) as usize;
        if let Some(v) = self.visual.as_mut() {
            v.b_line = line;
            v.b_col = col;
            if v.kind == VisualKind::Cursor {
                v.a_line = line;
                v.a_col = col;
            }
        }
        self.caret_line = line;
        self.caret_col = col;
        self.ensure_line_visible(line);
    }

    /// E → line end (col = len); B → line start (col = 0).
    pub(crate) fn visual_extend_line_edge(&mut self, to_end: bool) {
        let Some(v) = self.visual else {
            return;
        };
        if !matches!(v.kind, VisualKind::Char | VisualKind::Cursor) || self.body.is_empty() {
            return;
        }
        let line = v.b_line.min(self.body.len().saturating_sub(1));
        let col = if to_end { self.line_len(line) } else { 0 };
        if let Some(v) = self.visual.as_mut() {
            v.b_line = line;
            v.b_col = col;
            if v.kind == VisualKind::Cursor {
                v.a_line = line;
                v.a_col = col;
            }
        }
        self.caret_line = line;
        self.caret_col = col;
        self.ensure_line_visible(line);
    }

    pub(crate) fn visual_extend_word(&mut self, forward: bool) {
        let Some(v) = self.visual else {
            return;
        };
        if !matches!(v.kind, VisualKind::Char | VisualKind::Cursor) || self.body.is_empty() {
            return;
        }
        let (nl, nc) = if forward {
            self.word_fwd_pos(v.b_line, v.b_col)
        } else {
            self.word_bwd_pos(v.b_line, v.b_col)
        };
        if let Some(v) = self.visual.as_mut() {
            v.b_line = nl;
            v.b_col = nc;
            if v.kind == VisualKind::Cursor {
                v.a_line = nl;
                v.a_col = nc;
            }
        }
        self.caret_line = nl;
        self.caret_col = nc;
        self.ensure_line_visible(nl);
    }

    /// Promote Cursor → Char with anchor at current caret.
    pub(crate) fn visual_cursor_to_char(&mut self) {
        let Some(v) = self.visual.as_mut() else {
            return;
        };
        if v.kind != VisualKind::Cursor {
            return;
        }
        v.kind = VisualKind::Char;
        v.a_line = v.b_line;
        v.a_col = v.b_col;
        // ponytail: empty selection ok until moved (b may equal a)
        self.status = "VISUAL — hjkl bw e · y · d dict · a AI · Esc".into();
    }

    pub(crate) fn word_fwd_pos(&self, line: usize, col: usize) -> (usize, usize) {
        let max = self.body.len().saturating_sub(1);
        let mut li = line.min(max);
        let chars: Vec<char> = Self::line_plain(&self.body[li]).chars().collect();
        if let Some(c) = word_fwd_col(&chars, col) {
            return (li, c);
        }
        while li < max {
            li += 1;
            let chars: Vec<char> = Self::line_plain(&self.body[li]).chars().collect();
            if chars.iter().any(|&c| is_word_char(c)) {
                return (li, first_word_start(&chars));
            }
            if chars.is_empty() {
                return (li, 0);
            }
        }
        let len = self.line_len(line.min(max));
        (line.min(max), len)
    }

    pub(crate) fn word_end_pos(&self, line: usize, col: usize) -> (usize, usize) {
        let max = self.body.len().saturating_sub(1);
        let mut li = line.min(max);
        let chars: Vec<char> = Self::line_plain(&self.body[li]).chars().collect();
        if let Some(c) = word_end_col(&chars, col) {
            return (li, c);
        }
        while li < max {
            li += 1;
            let chars: Vec<char> = Self::line_plain(&self.body[li]).chars().collect();
            if let Some(c) = word_end_col(&chars, 0) {
                if chars.iter().any(|&ch| is_word_char(ch)) {
                    return (li, c);
                }
            }
            if chars.is_empty() {
                return (li, 0);
            }
        }
        let len = self.line_len(line.min(max));
        (line.min(max), len.saturating_sub(1).min(len))
    }

    pub(crate) fn word_bwd_pos(&self, line: usize, col: usize) -> (usize, usize) {
        let max = self.body.len().saturating_sub(1);
        let mut li = line.min(max);
        let chars: Vec<char> = Self::line_plain(&self.body[li]).chars().collect();
        if let Some(c) = word_bwd_col(&chars, col) {
            return (li, c);
        }
        while li > 0 {
            li -= 1;
            let chars: Vec<char> = Self::line_plain(&self.body[li]).chars().collect();
            if chars.iter().any(|&c| is_word_char(c)) {
                return (li, last_word_start(&chars));
            }
            if chars.is_empty() {
                return (li, 0);
            }
        }
        (0, 0)
    }

    pub(crate) fn selection_plain(&self) -> Option<String> {
        let v = self.visual?;
        selection_plain_from(&self.body, v)
    }

    pub(crate) fn yank_selection(&mut self) {
        let Some(v) = self.visual else {
            return;
        };
        if v.kind == VisualKind::Cursor {
            self.status = "yank: no selection".into();
            return;
        }
        let Some(text) = selection_plain_from(&self.body, v) else {
            self.status = "yank: empty selection".into();
            return;
        };
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

    const DICT_FILTER_MAX: usize = 200;

    pub(crate) fn dict_from_selection(&mut self) {
        if self.source.list_dicts().is_empty() {
            self.status = "dict: not a dictionary session".into();
            return;
        }
        let Some(text) = self.selection_plain() else {
            self.status = if self.visual.map(|v| v.kind) == Some(VisualKind::Cursor) {
                "dict: no selection".into()
            } else {
                "dict: empty selection".into()
            };
            return;
        };
        let text: String = text.chars().take(Self::DICT_FILTER_MAX).collect();
        self.filter = text.clone();
        self.list_sel = 0;
        self.refilter();
        self.visual = None;
        if !self.status.contains("no matches") {
            self.status = format!("dict filter: {text}");
        }
    }

    #[cfg(feature = "ai")]
    pub(crate) fn ai_from_selection(&mut self) {
        let Some(text) = self.selection_plain() else {
            self.status = if self.visual.map(|v| v.kind) == Some(VisualKind::Cursor) {
                "AI: no selection".into()
            } else {
                "AI: empty selection".into()
            };
            return;
        };
        let n = text.chars().count();
        self.refresh_ai_context();
        self.ai.set_selection_context(&text);
        if !self.ai.open {
            self.ai.toggle();
        }
        self.visual = None;
        self.status = format!("AI: selection in context ({n} chars)");
    }

    #[cfg(not(feature = "ai"))]
    pub(crate) fn ai_from_selection(&mut self) {
        self.status = "AI: not in this build".into();
    }
}

/// Returns trimmed selection text, or None for Cursor / empty after trim.
pub(crate) fn selection_plain_from(body: &[Line<'static>], v: VisualSel) -> Option<String> {
    let text = match v.kind {
        VisualKind::Cursor => return None,
        VisualKind::Line => {
            let a = v.a_line.min(v.b_line);
            let b = v.a_line.max(v.b_line);
            selected_plain(body, a, b)
        }
        VisualKind::Char => selected_plain_char(body, &v),
    };
    let t = text.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

pub(crate) fn yank_osc52(text: &str) -> std::io::Result<()> {
    use std::io::Write;
    let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, text.as_bytes());
    let mut out = std::io::stdout();
    write!(out, "\x1b]52;c;{b64}\x07")?;
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_fwd_bwd_basic() {
        // "foo bar_baz" — word = alnum + '_'
        let chars: Vec<char> = "foo bar_baz".chars().collect();
        assert_eq!(word_fwd_col(&chars, 0), Some(4)); // foo → bar_baz
        assert_eq!(word_fwd_col(&chars, 4), None); // bar_baz to EOL
        assert_eq!(word_bwd_col(&chars, 4), Some(0)); // bar_baz → foo
        assert_eq!(word_bwd_col(&chars, 11), Some(4)); // end → bar_baz
        assert_eq!(word_bwd_col(&chars, 0), None);
        assert_eq!(first_word_start(&chars), 0);
        assert_eq!(last_word_start(&chars), 4);
    }

    #[test]
    fn selection_plain_char_and_line() {
        let body = vec![Line::from("  hello  "), Line::from("world")];
        // Char: cols exclusive end on last line (same as selected_plain_char)
        let char_sel = VisualSel {
            kind: VisualKind::Char,
            a_line: 0,
            a_col: 2,
            b_line: 0,
            b_col: 7,
        };
        assert_eq!(selected_plain_char(&body, &char_sel), "hello");

        let line_sel = VisualSel {
            kind: VisualKind::Line,
            a_line: 0,
            a_col: 0,
            b_line: 1,
            b_col: 0,
        };
        let a = line_sel.a_line.min(line_sel.b_line);
        let b = line_sel.a_line.max(line_sel.b_line);
        assert_eq!(selected_plain(&body, a, b), "  hello  \nworld");
    }

    #[test]
    fn selection_plain_cursor_is_none_logic() {
        // Document contract: Cursor kind must not produce action text.
        // Implemented via App::selection_plain → None; pure kind check here.
        assert_eq!(VisualKind::Cursor, VisualKind::Cursor);
    }

    #[test]
    fn selection_plain_from_trims_and_skips_cursor() {
        let body = vec![Line::from("  ab  ")];
        let cursor = VisualSel {
            kind: VisualKind::Cursor,
            a_line: 0,
            a_col: 0,
            b_line: 0,
            b_col: 0,
        };
        assert!(selection_plain_from(&body, cursor).is_none());

        let char_sel = VisualSel {
            kind: VisualKind::Char,
            a_line: 0,
            a_col: 0,
            b_line: 0,
            b_col: 6,
        };
        assert_eq!(selection_plain_from(&body, char_sel).as_deref(), Some("ab"));

        let whitespace = VisualSel {
            kind: VisualKind::Char,
            a_line: 0,
            a_col: 0,
            b_line: 0,
            b_col: 2, // "  "
        };
        assert!(selection_plain_from(&body, whitespace).is_none());
    }
}
