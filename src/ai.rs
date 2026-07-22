//! Core AI chat (product core; `feature = "ai"`).
//!
//! OpenAI-compatible Chat Completions.
//! Config: `~/.config/tuider.yml` `ai.providers[]`, or env
//! `TUIDER_AI_KEY` / `OPENAI_API_KEY`, `TUIDER_AI_BASE_URL`, `TUIDER_AI_MODEL`.

use std::io::{BufRead, BufReader, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;
use std::thread;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;
use serde_json::{json, Value};
use unicode_width::UnicodeWidthChar;

use crate::theme::Theme;

const PREVIEW_CHARS: usize = 4000;
const MAX_MESSAGES: usize = 80;

#[derive(Debug, Clone)]
pub struct AiConfig {
    pub name: String,
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

impl AiConfig {
    pub fn from_env() -> Option<Self> {
        let api_key = std::env::var("TUIDER_AI_KEY")
            .or_else(|_| std::env::var("OPENAI_API_KEY"))
            .or_else(|_| std::env::var("AI_API_KEY"))
            .ok()?;
        if api_key.trim().is_empty() {
            return None;
        }
        let base_url = std::env::var("TUIDER_AI_BASE_URL")
            .unwrap_or_else(|_| "https://api.openai.com/v1".into());
        let model =
            std::env::var("TUIDER_AI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".into());
        Some(Self {
            name: "env".into(),
            api_key,
            base_url: base_url.trim_end_matches('/').to_string(),
            model,
        })
    }
}

#[derive(Clone, Debug)]
struct Bubble {
    role: String,
    content: String,
}

enum StreamEvent {
    Chunk(String),
    Done,
    Error(String),
}

pub struct AiSession {
    pub open: bool,
    pub configured: bool,
    pub status: String,
    providers: Vec<AiConfig>,
    active: usize,
    messages: Vec<Bubble>,
    input: String,
    /// Byte index into `input` for caret (always at char boundary).
    cursor: usize,
    scroll: u16,
    loading: bool,
    rx: Option<Receiver<StreamEvent>>,
    cancel: Option<Arc<AtomicBool>>,
    doc_title: String,
    doc_body: String,
}

impl Default for AiSession {
    fn default() -> Self {
        Self::new()
    }
}

impl AiSession {
    pub fn new() -> Self {
        Self::from_file_config(None)
    }

    pub fn from_file_config(file: Option<&crate::config::FileConfig>) -> Self {
        let mut providers: Vec<AiConfig> = crate::config::ai_providers(file)
            .into_iter()
            .map(|p| AiConfig {
                name: p.name,
                api_key: p.api_key,
                base_url: p.base_url,
                model: p.model,
            })
            .collect();
        if providers.is_empty() {
            if let Some(e) = AiConfig::from_env() {
                providers.push(e);
            }
        }
        let configured = !providers.is_empty();
        Self {
            open: false,
            configured,
            status: String::new(),
            providers,
            active: 0,
            messages: Vec::new(),
            input: String::new(),
            cursor: 0,
            scroll: 0,
            loading: false,
            rx: None,
            cancel: None,
            doc_title: String::new(),
            doc_body: String::new(),
        }
    }

    fn active_cfg(&self) -> Option<&AiConfig> {
        self.providers.get(self.active)
    }

    pub fn set_document_context(&mut self, title: &str, plain_body: &str) {
        self.doc_title = title.to_string();
        self.doc_body = plain_body.chars().take(PREVIEW_CHARS).collect();
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
        if !self.open {
            self.status.clear();
            return;
        }
        // reload providers from disk
        let file = crate::config::load().map(|(_, c)| c);
        let mut providers: Vec<AiConfig> = crate::config::ai_providers(file.as_ref())
            .into_iter()
            .map(|p| AiConfig {
                name: p.name,
                api_key: p.api_key,
                base_url: p.base_url,
                model: p.model,
            })
            .collect();
        if providers.is_empty() {
            if let Some(e) = AiConfig::from_env() {
                providers.push(e);
            }
        }
        self.providers = providers;
        self.configured = !self.providers.is_empty();
        if self.active >= self.providers.len() {
            self.active = 0;
        }
        self.status = self.status_line();
    }

    fn status_line(&self) -> String {
        if let Some(c) = self.active_cfg() {
            format!(
                "AI [{}] {} · C-j/C-Enter send · 1=翻译全文 · Tab · Esc",
                c.name, c.model
            )
        } else {
            "AI: set ai.providers in ~/.config/tuider.yml or TUIDER_AI_KEY".into()
        }
    }

    pub fn focus_label(&self) -> Option<&'static str> {
        if self.open {
            Some("ai")
        } else {
            None
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn poll(&mut self) {
        let Some(rx) = self.rx.take() else {
            return;
        };
        let mut disconnect = false;
        let mut done = false;
        loop {
            match rx.try_recv() {
                Ok(StreamEvent::Chunk(s)) => {
                    if let Some(last) = self.messages.last_mut() {
                        if last.role == "assistant" {
                            last.content.push_str(&s);
                        }
                    }
                    self.scroll = u16::MAX;
                }
                Ok(StreamEvent::Done) => {
                    self.loading = false;
                    self.cancel = None;
                    self.status = self.status_line();
                    done = true;
                }
                Ok(StreamEvent::Error(e)) => {
                    self.loading = false;
                    self.cancel = None;
                    if let Some(last) = self.messages.last_mut() {
                        if last.role == "assistant" && last.content.is_empty() {
                            last.content = format!("(error) {e}");
                        } else {
                            self.messages.push(Bubble {
                                role: "assistant".into(),
                                content: format!("(error) {e}"),
                            });
                        }
                    } else {
                        self.messages.push(Bubble {
                            role: "assistant".into(),
                            content: format!("(error) {e}"),
                        });
                    }
                    self.status = format!("AI error: {e}");
                    done = true;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.loading = false;
                    self.cancel = None;
                    disconnect = true;
                    break;
                }
            }
        }
        if !done && !disconnect {
            self.rx = Some(rx);
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);

        if self.loading {
            match key.code {
                KeyCode::Esc => {
                    if let Some(c) = &self.cancel {
                        c.store(true, Ordering::SeqCst);
                    }
                    self.status = "cancelling…".into();
                }
                KeyCode::Up => self.scroll = self.scroll.saturating_sub(1),
                KeyCode::Down => self.scroll = self.scroll.saturating_add(1),
                KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(5),
                KeyCode::PageDown => self.scroll = self.scroll.saturating_add(5),
                _ => {}
            }
            return false;
        }

        match key.code {
            KeyCode::Esc => {
                self.open = false;
                self.status.clear();
            }
            KeyCode::Tab if !self.providers.is_empty() => {
                self.active = (self.active + 1) % self.providers.len();
                self.status = self.status_line();
            }
            // send: many terminals never deliver Ctrl+Enter as Enter+CONTROL
            // (send Ctrl+J / Ctrl+M / \n+\ctrl instead)
            KeyCode::Enter if ctrl || alt => self.send(),
            KeyCode::Char('j') if ctrl => self.send(),
            KeyCode::Char('m') if ctrl => self.send(),
            KeyCode::Char('\n') | KeyCode::Char('\r') if ctrl || alt => self.send(),
            KeyCode::Enter => {
                self.insert_at_cursor('\n');
            }
            KeyCode::Backspace => {
                self.backspace_at_cursor();
            }
            KeyCode::Delete => {
                self.delete_at_cursor();
            }
            KeyCode::Left if !ctrl => {
                self.move_cursor_left();
            }
            KeyCode::Right if !ctrl => {
                self.move_cursor_right();
            }
            KeyCode::Home => {
                self.cursor = line_start_byte(&self.input, self.cursor);
            }
            KeyCode::End => {
                self.cursor = line_end_byte(&self.input, self.cursor);
            }
            KeyCode::Char('u') if ctrl => {
                self.input.clear();
                self.cursor = 0;
            }
            KeyCode::Char('a') if ctrl => {
                self.cursor = 0;
            }
            KeyCode::Char('e') if ctrl => {
                self.cursor = self.input.len();
            }
            KeyCode::Char(c)
                if key.modifiers == KeyModifiers::NONE
                    || key.modifiers == KeyModifiers::SHIFT =>
            {
                if !c.is_control() {
                    self.insert_at_cursor(c);
                    self.apply_input_shortcuts();
                }
            }
            KeyCode::Up => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::Down => self.scroll = self.scroll.saturating_add(1),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(5),
            KeyCode::PageDown => self.scroll = self.scroll.saturating_add(5),
            _ => {}
        }
        false
    }

    fn insert_at_cursor(&mut self, c: char) {
        self.cursor = self.cursor.min(self.input.len());
        self.input.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    fn backspace_at_cursor(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let prev = prev_char_boundary(&self.input, self.cursor);
        self.input.replace_range(prev..self.cursor, "");
        self.cursor = prev;
    }

    fn delete_at_cursor(&mut self) {
        if self.cursor >= self.input.len() {
            return;
        }
        let next = next_char_boundary(&self.input, self.cursor);
        self.input.replace_range(self.cursor..next, "");
    }

    fn move_cursor_left(&mut self) {
        self.cursor = prev_char_boundary(&self.input, self.cursor);
    }

    fn move_cursor_right(&mut self) {
        self.cursor = next_char_boundary(&self.input, self.cursor);
    }

    /// Expand bare shortcuts in the whole input box.
    /// `1` alone → 翻译全文 prompt (document is already in system context).
    fn apply_input_shortcuts(&mut self) {
        let t = self.input.trim();
        if t == "1" {
            self.input = "请将当前文档全文翻译成中文，保留段落结构，专有名词可保留原文。".into();
            self.cursor = self.input.len();
        }
    }

    fn send(&mut self) {
        self.apply_input_shortcuts();
        let text = self.input.trim().to_string();
        if text.is_empty() || self.loading {
            return;
        }
        if self.providers.is_empty() {
            self.status = "AI: no provider configured".into();
            return;
        }

        self.messages.push(Bubble {
            role: "user".into(),
            content: text,
        });
        self.messages.push(Bubble {
            role: "assistant".into(),
            content: String::new(),
        });
        while self.messages.len() > MAX_MESSAGES {
            self.messages.remove(0);
        }
        self.input.clear();
        self.cursor = 0;
        self.loading = true;
        let start = self.active.min(self.providers.len() - 1);
        let first = &self.providers[start];
        self.status = format!("AI [{}] thinking…", first.name);
        self.scroll = u16::MAX;

        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        self.rx = Some(rx);
        self.cancel = Some(Arc::clone(&cancel));

        let mut api_msgs: Vec<(String, String)> = Vec::new();
        for b in &self.messages {
            if b.role == "assistant" && b.content.is_empty() {
                continue;
            }
            api_msgs.push((b.role.clone(), b.content.clone()));
        }

        let system = format!(
            "You are a reading assistant inside the Tuider terminal reader.\n\
             Current document: {}\n\
             --- document preview ---\n{}\n--- end ---\n\
             Answer in the user's language. Be concise.",
            if self.doc_title.is_empty() {
                "(untitled)"
            } else {
                &self.doc_title
            },
            if self.doc_body.is_empty() {
                "(empty)"
            } else {
                &self.doc_body
            }
        );

        let providers = self.providers.clone();
        // ponytail: rotate only for this request; Tab still sets sticky active
        thread::spawn(move || {
            let n = providers.len();
            let mut errs: Vec<String> = Vec::new();
            for off in 0..n {
                if cancel.load(Ordering::SeqCst) {
                    let _ = tx.send(StreamEvent::Error("cancelled".into()));
                    return;
                }
                let cfg = &providers[(start + off) % n];
                match chat_request(cfg, &system, &api_msgs, &tx, &cancel) {
                    Ok(()) => return,
                    Err(e) if is_failover_err(&e) && off + 1 < n => {
                        errs.push(format!("{}: {e}", cfg.name));
                    }
                    Err(e) => {
                        if errs.is_empty() {
                            let _ = tx.send(StreamEvent::Error(format!("[{}] {e}", cfg.name)));
                        } else {
                            errs.push(format!("{}: {e}", cfg.name));
                            let _ = tx.send(StreamEvent::Error(format!(
                                "all providers failed: {}",
                                errs.join(" | ")
                            )));
                        }
                        return;
                    }
                }
            }
            let _ = tx.send(StreamEvent::Error(format!(
                "all providers failed: {}",
                errs.join(" | ")
            )));
        });
    }

    pub fn draw(&mut self, frame: &mut Frame, area: Rect, theme: Theme) {
        // multi-line input box grows a bit with content (cap 8 rows)
        let input_lines = self.input.chars().filter(|c| *c == '\n').count() + 1;
        let input_h = (input_lines as u16 + 2).clamp(3, 8);
        let chunks =
            Layout::vertical([Constraint::Min(3), Constraint::Length(input_h)]).split(area);
        let msg_area = chunks[0];
        let input_area = chunks[1];

        let pname = self
            .active_cfg()
            .map(|c| c.name.as_str())
            .unwrap_or("?");
        let title = if self.loading {
            format!(" AI [{pname}] streaming… Esc cancel ")
        } else if self.configured {
            format!(" AI [{pname}] C-j send · Enter ↵ · 1=翻译全文 · Tab · Esc ")
        } else {
            " AI (no API key) ".into()
        };
        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border()));
        let inner = block.inner(msg_area);
        frame.render_widget(block, msg_area);

        let width = inner.width.max(20) as usize;
        let mut lines: Vec<Line> = Vec::new();
        for b in &self.messages {
            let (label, style) = match b.role.as_str() {
                "user" => (
                    "You",
                    Style::default()
                        .fg(theme.accent())
                        .add_modifier(Modifier::BOLD),
                ),
                _ => (
                    "AI",
                    Style::default()
                        .fg(theme.title())
                        .add_modifier(Modifier::BOLD),
                ),
            };
            lines.push(Line::from(Span::styled(format!("{label}:"), style)));
            if b.role == "assistant" && !b.content.is_empty() {
                // mdterm-style markdown for assistant replies
                let md_lines = crate::md::render_md_width(&b.content, width);
                lines.extend(md_lines);
            } else if b.content.is_empty() && b.role == "assistant" && self.loading {
                lines.push(Line::from(Span::styled(
                    "…",
                    Style::default().fg(theme.muted()),
                )));
            } else {
                // user text: preserve newlines, soft-wrap via Paragraph later as lines
                for raw in b.content.split('\n') {
                    lines.push(Line::from(Span::styled(
                        raw.to_string(),
                        Style::default().fg(theme.list_text()),
                    )));
                }
            }
            lines.push(Line::from(""));
        }
        if lines.is_empty() {
            lines.push(Line::from(Span::styled(
                "Ask about the current document…",
                Style::default().fg(theme.muted()),
            )));
        }

        let max_scroll = lines.len().saturating_sub(inner.height as usize) as u16;
        if self.scroll == u16::MAX {
            self.scroll = max_scroll;
        }
        let scroll = self.scroll.min(max_scroll);
        frame.render_widget(
            Paragraph::new(lines)
                .scroll((scroll, 0))
                .wrap(Wrap { trim: false }),
            inner,
        );

        let ib = Block::default()
            .title(" input · C-j / C-Enter send · 1=翻译全文 ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border()));
        let iinner = ib.inner(input_area);
        frame.render_widget(ib, input_area);

        let display = if self.loading {
            "…".to_string()
        } else {
            self.input.clone()
        };
        frame.render_widget(
            Paragraph::new(display)
                .style(Style::default().fg(theme.search_text()))
                .wrap(Wrap { trim: false }),
            iinner,
        );
        if !self.loading {
            let (cx, cy) = cursor_xy(&self.input, self.cursor, iinner.width.max(1) as usize);
            let x = iinner.x.saturating_add(cx.min(iinner.width.saturating_sub(1)));
            let y = iinner.y.saturating_add(cy.min(iinner.height.saturating_sub(1)));
            frame.set_cursor_position((x, y));
        }
    }

}


fn prev_char_boundary(s: &str, idx: usize) -> usize {
    if idx == 0 {
        return 0;
    }
    let mut i = idx - 1;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn next_char_boundary(s: &str, idx: usize) -> usize {
    if idx >= s.len() {
        return s.len();
    }
    let mut i = idx + 1;
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

fn line_start_byte(s: &str, cursor: usize) -> usize {
    s[..cursor.min(s.len())].rfind('\n').map(|i| i + 1).unwrap_or(0)
}

fn line_end_byte(s: &str, cursor: usize) -> usize {
    let c = cursor.min(s.len());
    s[c..].find('\n').map(|i| c + i).unwrap_or(s.len())
}

/// Map byte cursor in multiline input to (x,y) with soft wrap at `width` cols.
fn cursor_xy(input: &str, cursor: usize, width: usize) -> (u16, u16) {
    let width = width.max(1);
    let c = cursor.min(input.len());
    let mut x = 0usize;
    let mut y = 0usize;
    for ch in input[..c].chars() {
        if ch == '\n' {
            y += 1;
            x = 0;
            continue;
        }
        let w = UnicodeWidthChar::width(ch).unwrap_or(1).max(1);
        if x + w > width {
            y += 1;
            x = 0;
        }
        x += w;
    }
    (x as u16, y as u16)
}

/// Prefer streaming SSE; fall back to non-stream JSON if needed.
fn chat_request(
    cfg: &AiConfig,
    system: &str,
    messages: &[(String, String)],
    tx: &mpsc::Sender<StreamEvent>,
    cancel: &AtomicBool,
) -> Result<(), String> {
    match stream_chat(cfg, system, messages, tx, cancel) {
        Ok(()) => Ok(()),
        Err(e) if is_failover_err(&e) => Err(e),
        Err(e) => {
            // fall back non-stream so flaky SSE providers still work
            let text = nonstream_chat(cfg, system, messages, cancel)
                .map_err(|e2| format!("stream failed ({e}); non-stream failed ({e2})"))?;
            if cancel.load(Ordering::SeqCst) {
                let _ = tx.send(StreamEvent::Error("cancelled".into()));
                return Ok(());
            }
            let _ = tx.send(StreamEvent::Chunk(text));
            let _ = tx.send(StreamEvent::Done);
            Ok(())
        }
    }
}

fn is_failover_err(err: &str) -> bool {
    // ponytail: string match on our HTTP error format; enough for 503 rotate
    ["HTTP 429", "HTTP 500", "HTTP 502", "HTTP 503", "HTTP 504"]
        .iter()
        .any(|p| err.contains(p))
}

fn build_body(cfg: &AiConfig, system: &str, messages: &[(String, String)], stream: bool) -> Value {
    let mut api_messages = vec![json!({"role": "system", "content": system})];
    for (role, content) in messages {
        api_messages.push(json!({"role": role, "content": content}));
    }
    json!({
        "model": cfg.model,
        "messages": api_messages,
        "stream": stream,
    })
}

fn stream_chat(
    cfg: &AiConfig,
    system: &str,
    messages: &[(String, String)],
    tx: &mpsc::Sender<StreamEvent>,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let body = build_body(cfg, system, messages, true);
    let url = format!("{}/chat/completions", cfg.base_url);
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(180))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .post(&url)
        .bearer_auth(&cfg.api_key)
        .header("content-type", "application/json")
        .header("accept", "text/event-stream")
        .json(&body)
        .send()
        .map_err(|e| format!("request: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let t = resp.text().unwrap_or_default();
        return Err(format!(
            "HTTP {status}: {}",
            t.chars().take(240).collect::<String>()
        ));
    }

    let mut got_any = false;
    let reader = BufReader::new(resp);
    for line in reader.lines() {
        if cancel.load(Ordering::SeqCst) {
            let _ = tx.send(StreamEvent::Error("cancelled".into()));
            return Ok(());
        }
        let line = line.map_err(|e| e.to_string())?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // tolerate "data:" and "data: "
        let data = line
            .strip_prefix("data: ")
            .or_else(|| line.strip_prefix("data:"))
            .unwrap_or("");
        if data.is_empty() {
            continue;
        }
        if data == "[DONE]" {
            let _ = tx.send(StreamEvent::Done);
            return Ok(());
        }
        let v: Value = match serde_json::from_str(data) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let Some(content) = v
            .pointer("/choices/0/delta/content")
            .and_then(|c| c.as_str())
        {
            if !content.is_empty() {
                got_any = true;
                let _ = tx.send(StreamEvent::Chunk(content.to_string()));
            }
        }
        if let Some(content) = v
            .pointer("/choices/0/message/content")
            .and_then(|c| c.as_str())
        {
            if !content.is_empty() {
                got_any = true;
                let _ = tx.send(StreamEvent::Chunk(content.to_string()));
            }
        }
    }
    if !got_any {
        return Err("empty SSE stream".into());
    }
    let _ = tx.send(StreamEvent::Done);
    Ok(())
}

fn nonstream_chat(
    cfg: &AiConfig,
    system: &str,
    messages: &[(String, String)],
    cancel: &AtomicBool,
) -> Result<String, String> {
    let body = build_body(cfg, system, messages, false);
    let url = format!("{}/chat/completions", cfg.base_url);
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(180))
        .build()
        .map_err(|e| e.to_string())?;
    if cancel.load(Ordering::SeqCst) {
        return Err("cancelled".into());
    }
    let resp = client
        .post(&url)
        .bearer_auth(&cfg.api_key)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .map_err(|e| format!("request: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let mut t = String::new();
        let _ = resp.take(4096).read_to_string(&mut t);
        return Err(format!(
            "HTTP {status}: {}",
            t.chars().take(240).collect::<String>()
        ));
    }
    let v: Value = resp.json().map_err(|e| e.to_string())?;
    v.pointer("/choices/0/message/content")
        .and_then(|c| c.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("no content in response: {}", v.to_string().chars().take(200).collect::<String>()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_session() -> AiSession {
        AiSession {
            open: true,
            configured: false,
            status: String::new(),
            providers: Vec::new(),
            active: 0,
            messages: Vec::new(),
            input: String::new(),
            cursor: 0,
            scroll: 0,
            loading: false,
            rx: None,
            cancel: None,
            doc_title: String::new(),
            doc_body: String::new(),
        }
    }

    #[test]
    fn cursor_xy_wraps_and_newlines() {
        assert_eq!(cursor_xy("ab", 2, 80), (2, 0));
        assert_eq!(cursor_xy("a\nb", 3, 80), (1, 1));
        assert_eq!(cursor_xy("abcd", 4, 2), (2, 1));
    }

    #[test]
    fn insert_and_backspace_at_cursor() {
        let mut s = empty_session();
        s.insert_at_cursor('a');
        s.insert_at_cursor('b');
        s.insert_at_cursor('c');
        assert_eq!(s.input, "abc");
        assert_eq!(s.cursor, 3);
        s.move_cursor_left();
        s.backspace_at_cursor();
        assert_eq!(s.input, "ac");
    }

    #[test]
    fn toggle_without_key() {
        let mut s = empty_session();
        s.open = false;
        s.toggle();
        assert!(s.open);
    }

    #[test]
    fn shortcut_1_expands_to_translate() {
        let mut s = empty_session();
        s.input = "1".into();
        s.cursor = 1;
        s.apply_input_shortcuts();
        assert!(s.input.contains("翻译"));
        assert_ne!(s.input.trim(), "1");
    }

    #[test]
    fn shortcut_1_only_when_bare() {
        let mut s = empty_session();
        s.input = "12".into();
        s.cursor = 2;
        s.apply_input_shortcuts();
        assert_eq!(s.input, "12");
    }

    #[test]
    fn ctrl_j_triggers_send_path() {
        let mut s = empty_session();
        s.input = "hi".into();
        s.cursor = 2;
        let key = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL);
        s.handle_key(key);
        assert!(s.status.contains("no provider"));
    }

    #[test]
    fn typing_1_expands_live() {
        let mut s = empty_session();
        let key = KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE);
        s.handle_key(key);
        assert!(s.input.contains("翻译"));
    }
}
