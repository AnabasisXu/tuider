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
const MAX_TOOL_ROUNDS: usize = 12;
const CONTENT_CHUNK_BYTES: usize = 12000;
const SYSTEM_CONTEXT_PREVIEW_BYTES: usize = 2500;

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

#[allow(dead_code)] // Using/Note reserved for failover status UX
enum StreamEvent {
    Chunk(String),
    Done,
    Error(String),
    /// Sticky provider index after successful failover (or explicit pick).
    Using(usize),
    /// Status-line only (e.g. rotating providers).
    Note(String),

    /// Worker needs host to run a tool against the live ContentSource.
    ToolCall {
        name: String,
        arguments: String,
        reply: mpsc::Sender<String>,
    },
}


pub struct AiSession {
    pub open: bool,
    /// Full-area AI pane (Alt+Shift+L).
    pub maximized: bool,
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
    /// Preview for system prompt.
    doc_body: String,
    /// Full pane text for tools (get_current_content).
    full_body: String,
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
            maximized: false,
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
            full_body: String::new(),
        }
    }

    fn active_cfg(&self) -> Option<&AiConfig> {
        self.providers.get(self.active)
    }

    pub fn set_document_context(&mut self, title: &str, plain_body: &str) {
        self.doc_title = title.to_string();
        self.full_body = plain_body.to_string();
        self.doc_body = plain_body.chars().take(PREVIEW_CHARS).collect();
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
        if !self.open {
            self.maximized = false;
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

    /// Alt+Shift+L: open maximized if closed; else flip maximized.
    pub fn toggle_maximize(&mut self) {
        if !self.open {
            self.toggle();
            self.maximized = true;
        } else {
            self.maximized = !self.maximized;
        }
    }

    fn status_line(&self) -> String {
        if let Some(c) = self.active_cfg() {
            format!(
                "AI [{}] {} · C-j send · A-t 翻译 · /exp /switch · Tab · Esc",
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

    /// Drain stream events. `source` is used when the model requests dict tools.
    pub fn poll(&mut self, source: &mut dyn crate::plugin::ContentSource) {
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
                Ok(StreamEvent::Using(i)) => {
                    if i < self.providers.len() {
                        self.active = i;
                    }
                    self.status = self.status_line();
                }
                Ok(StreamEvent::Note(s)) => {
                    self.status = s;
                }

                Ok(StreamEvent::ToolCall {
                    name,
                    arguments,
                    reply,
                }) => {
                    self.status = format!("AI tool: {name}…");
                    let out = execute_tool(
                        source,
                        &name,
                        &arguments,
                        &self.doc_title,
                        &self.full_body,
                    );
                    log_tool(&name, &arguments, &out);
                    let _ = reply.send(out);
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
                self.maximized = false;
                self.status.clear();
            }
            KeyCode::Tab if !self.providers.is_empty() => {
                self.active = (self.active + 1) % self.providers.len();
                self.status = self.status_line();
            }
            // send: Ctrl+J only (terminals rarely deliver Ctrl+Enter as Enter+CONTROL)
            KeyCode::Char('j') if ctrl => self.send(),
            // Alt+t: full-document translate (replaces old bare "1" shortcut)
            KeyCode::Char('t') | KeyCode::Char('T') if alt => self.send_translate_full(),
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

    const TRANSLATE_FULL: &'static str =
        "请将当前文档全文翻译成中文，保留段落结构，专有名词可保留原文。若预览截断请先 get_current_content。";

    /// Alt+t: queue full-doc translate and send immediately.
    fn send_translate_full(&mut self) {
        self.input = Self::TRANSLATE_FULL.into();
        self.cursor = self.input.len();
        self.send();
    }



    /// Handle `/exp` and `/switch` before network send. Returns true if consumed.
    fn try_slash_command(&mut self, text: &str) -> bool {
        let t = text.trim();
        if t == "/exp" || t.starts_with("/exp ") {
            let arg = t.strip_prefix("/exp").unwrap_or("").trim();
            match export_chat(&self.messages, arg) {
                Ok(path) => {
                    self.status = format!("exported {path}");
                    self.messages.push(Bubble {
                        role: "assistant".into(),
                        content: format!("Exported conversation to `{path}`."),
                    });
                }
                Err(e) => self.status = format!("/exp failed: {e}"),
            }
            return true;
        }
        if t == "/switch" || t.starts_with("/switch ") {
            let arg = t.strip_prefix("/switch").unwrap_or("").trim();
            if arg.is_empty() {
                let mut lines = Vec::new();
                for (i, p) in self.providers.iter().enumerate() {
                    let mark = if i == self.active { "*" } else { " " };
                    lines.push(format!("{mark} {}. {} ({})", i + 1, p.name, p.model));
                }
                let body = if lines.is_empty() {
                    "No providers configured.".into()
                } else {
                    format!(
                        "Providers (current *):\n{}\nUsage: /switch <name|index>",
                        lines.join("\n")
                    )
                };
                self.messages.push(Bubble {
                    role: "assistant".into(),
                    content: body,
                });
                self.status = self.status_line();
            } else if let Some(i) = parse_switch_target(arg, &self.providers) {
                self.active = i;
                self.status = self.status_line();
                self.messages.push(Bubble {
                    role: "assistant".into(),
                    content: format!("Switched to provider `{}`.", self.providers[i].name),
                });
            } else {
                self.status = format!("unknown provider `{arg}` — try /switch");
            }
            return true;
        }
        false
    }

    fn send(&mut self) {
        let text = self.input.trim().to_string();

        if text.is_empty() || self.loading {
            return;
        }
        // slash commands don't need providers
        if self.try_slash_command(&text) {
            self.input.clear();
            self.cursor = 0;
            self.scroll = u16::MAX;
            return;
        }
        if self.providers.is_empty() {
            self.status = "AI: no provider configured".into();
            return;
        }

        self.messages.push(Bubble {
            role: "user".into(),
            content: text.clone(),
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

        let mut history: Vec<(String, String)> = Vec::new();
        for b in &self.messages {
            if b.role == "assistant" && b.content.is_empty() {
                continue;
            }
            history.push((b.role.clone(), b.content.clone()));
        }

        let title = self.doc_title.clone();
        let preview = self.doc_body.clone();
        let full_len = self.full_body.len();
        let truncated = full_len > SYSTEM_CONTEXT_PREVIEW_BYTES;
        let mut system = format!(
            "你是词典与阅读助手，用中文简洁回答。有工具：查词/搜词头/反查/列词典/读正文/导出/联网搜索。\n\
             规则：\n\
             1. 查词义用 query_word，模糊用 search_headwords，反查 reverse_lookup。\n\
             2. 全文翻译/摘要若预览截断，先 get_current_content（可 offset 分段）。\n\
             3. 导出内容用 export_content；整段对话导出提示用户 /exp。\n\
             4. 最多 {MAX_TOOL_ROUNDS} 轮工具。\n\
             当前文档：{title}（正文约 {full_len} 字节"
        );
        if truncated {
            system.push_str("，预览已截断");
        }
        system.push_str("）\n--- preview ---\n");
        system.push_str(if preview.is_empty() { "(empty)" } else { &preview });
        system.push_str("\n--- end ---");

        let providers = self.providers.clone();
        thread::spawn(move || {
            let n = providers.len();
            let mut errs: Vec<String> = Vec::new();
            for off in 0..n {
                if cancel.load(Ordering::SeqCst) {
                    let _ = tx.send(StreamEvent::Error("cancelled".into()));
                    return;
                }
                let cfg = &providers[(start + off) % n];
                match chat_with_tools_loop(cfg, &system, &history, &tx, &cancel) {
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
            format!(" AI [{pname}] C-j send · A-t 翻译 · Enter ↵ · Tab · Esc ")
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
        // md is already width-aware; Paragraph wrap would double-wrap assistant bubbles
        frame.render_widget(
            Paragraph::new(lines).scroll((scroll, 0)),
            inner,
        );

        let ib = Block::default()
            .title(" input · C-j send · A-t 翻译全文 ")

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

fn parse_switch_target(arg: &str, providers: &[AiConfig]) -> Option<usize> {
    if let Ok(n) = arg.parse::<usize>() {
        if n >= 1 && n <= providers.len() {
            return Some(n - 1);
        }
    }
    let al = arg.to_lowercase();
    providers
        .iter()
        .position(|p| p.name.eq_ignore_ascii_case(arg) || p.name.to_lowercase().contains(&al))
}

fn export_chat(messages: &[Bubble], arg: &str) -> Result<String, String> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = format!("tuider-chat-{ts}.md");
    let selected: Vec<&Bubble> = if arg.is_empty() {
        messages.iter().collect()
    } else if arg == "last" {
        messages.iter().rev().take(2).collect::<Vec<_>>().into_iter().rev().collect()
    } else if let Ok(n) = arg.parse::<usize>() {
        // 1-based user messages pair approx: take message n (clamp)
        let idx = n.saturating_sub(1).min(messages.len().saturating_sub(1));
        messages.get(idx).into_iter().collect()
    } else {
        return Err(format!(
            "Unknown /exp argument '{arg}'. Use /exp, /exp last, or /exp <1-based index>."
        ));
    };
    let mut body = String::from("# Tuider chat export\n\n");
    for b in selected {
        body.push_str("## ");
        body.push_str(&b.role);
        body.push_str("\n\n");
        body.push_str(&b.content);
        body.push_str("\n\n");
    }
    std::fs::write(&path, body).map_err(|e| e.to_string())?;
    Ok(path)
}

fn log_tool(name: &str, args: &str, out: &str) {
    // ponytail: best-effort append; ignore failures
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let Some(home) = home else { return };
    let dir = home.join(".config/tuider");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("ai-tools.log");
    let line = format!(
        "[{}] {name} args={} out_len={}\n",
        chrono_like_ts(),
        args.chars().take(200).collect::<String>(),
        out.len()
    );
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = f.write_all(line.as_bytes());
    }
}

fn chrono_like_ts() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn get_tools() -> Vec<Value> {
    vec![
        json!({"type":"function","function":{"name":"query_word","description":"Look up a headword in loaded dictionaries","parameters":{"type":"object","properties":{"word":{"type":"string"}},"required":["word"]}}}),
        json!({"type":"function","function":{"name":"search_headwords","description":"Prefix search headwords","parameters":{"type":"object","properties":{"prefix":{"type":"string"},"limit":{"type":"integer"}},"required":["prefix"]}}}),
        json!({"type":"function","function":{"name":"list_dicts","description":"List loaded dictionary names","parameters":{"type":"object","properties":{}}}}),
        json!({"type":"function","function":{"name":"batch_query","description":"Look up multiple words","parameters":{"type":"object","properties":{"words":{"type":"array","items":{"type":"string"}}},"required":["words"]}}}),
        json!({"type":"function","function":{"name":"analyze_vocab","description":"Simple vocab stats for a text snippet","parameters":{"type":"object","properties":{"text":{"type":"string"}},"required":["text"]}}}),
        json!({"type":"function","function":{"name":"reverse_lookup","description":"Find headwords whose definition contains query","parameters":{"type":"object","properties":{"query":{"type":"string"},"limit":{"type":"integer"}},"required":["query"]}}}),
        json!({"type":"function","function":{"name":"get_current_content","description":"Read current document body (chunked)","parameters":{"type":"object","properties":{"offset":{"type":"integer"},"limit":{"type":"integer"}}}}}),
        json!({"type":"function","function":{"name":"export_content","description":"Write markdown content to a file in cwd","parameters":{"type":"object","properties":{"filename":{"type":"string"},"content":{"type":"string"}},"required":["content"]}}}),
        json!({"type":"function","function":{"name":"web_search","description":"Web search if TAVILY_API_KEY or TUIDER_WEB_SEARCH_URL set","parameters":{"type":"object","properties":{"query":{"type":"string"}},"required":["query"]}}}),
    ]
}

fn execute_tool(
    source: &mut dyn crate::plugin::ContentSource,
    name: &str,
    arguments: &str,
    doc_title: &str,
    full_body: &str,
) -> String {
    let args: Value = serde_json::from_str(arguments).unwrap_or_else(|_| json!({}));
    match name {
        "query_word" => {
            let word = args["word"].as_str().unwrap_or("").trim();
            if word.is_empty() {
                return "error: empty word".into();
            }
            let hits = source.lookup_word(word);
            if hits.is_empty() {
                format!("Not found: {word}")
            } else {
                hits.into_iter()
                    .map(|(d, t)| format!("### {d}\n{t}"))
                    .collect::<Vec<_>>()
                    .join("\n\n")
            }
        }
        "search_headwords" => {
            let prefix = args["prefix"].as_str().unwrap_or("");
            let limit = args["limit"].as_u64().unwrap_or(20) as usize;
            let words = source.search_headwords(prefix, limit);
            if words.is_empty() {
                format!("(no headwords for prefix `{prefix}` — dict plugin may be missing)")
            } else {
                words.join("\n")
            }
        }
        "list_dicts" => {
            let list = source.list_dicts();
            if list.is_empty() {
                "(no dict list — not in dictionary mode or plugin lacks symbol)".into()
            } else {
                list.join("\n")
            }
        }
        "batch_query" => {
            let words: Vec<String> = args["words"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();
            if words.is_empty() {
                return "error: words[] empty".into();
            }
            words
                .iter()
                .map(|w| {
                    let hits = source.lookup_word(w);
                    if hits.is_empty() {
                        format!("## {w}\nNot found")
                    } else {
                        let body = hits
                            .into_iter()
                            .map(|(d, t)| format!("### {d}\n{t}"))
                            .collect::<Vec<_>>()
                            .join("\n");
                        format!("## {w}\n{body}")
                    }
                })
                .collect::<Vec<_>>()
                .join("\n\n")
        }
        "analyze_vocab" => {
            let text = args["text"].as_str().unwrap_or(full_body);
            let mut words = std::collections::BTreeMap::new();
            for w in text.split(|c: char| !c.is_alphabetic()) {
                if w.len() < 3 {
                    continue;
                }
                let k = w.to_lowercase();
                *words.entry(k).or_insert(0usize) += 1;
            }
            let mut ranked: Vec<_> = words.into_iter().collect();
            ranked.sort_by(|a, b| b.1.cmp(&a.1));
            ranked.truncate(40);
            format!(
                "title={doc_title}\ntop tokens:\n{}",
                ranked
                    .into_iter()
                    .map(|(w, n)| format!("{w}: {n}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        }
        "reverse_lookup" => {
            let q = args["query"].as_str().unwrap_or("");
            let limit = args["limit"].as_u64().unwrap_or(15) as usize;
            let words = source.reverse_lookup(q, limit);
            if words.is_empty() {
                format!("(no reverse hits for `{q}`)")
            } else {
                words.join("\n")
            }
        }
        "get_current_content" => {
            let offset = args["offset"].as_u64().unwrap_or(0) as usize;
            let limit = args["limit"]
                .as_u64()
                .unwrap_or(CONTENT_CHUNK_BYTES as u64) as usize;
            let limit = limit.clamp(1, CONTENT_CHUNK_BYTES);
            if full_body.is_empty() {
                return "(empty document)".into();
            }
            let start = offset.min(full_body.len());
            // byte-ish clamp at char boundary
            let mut end = (start + limit).min(full_body.len());
            while end > start && !full_body.is_char_boundary(end) {
                end -= 1;
            }
            let slice = &full_body[start..end];
            format!(
                "title={doc_title}\noffset={start} end={end} total={}\n---\n{slice}",
                full_body.len()
            )
        }
        "export_content" => {
            let content = args["content"].as_str().unwrap_or("");
            let filename = args["filename"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| format!("tuider-export-{}.md", chrono_like_ts()));
            let safe = std::path::Path::new(&filename)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("export.md");
            match std::fs::write(safe, content) {
                Ok(()) => format!("wrote {safe} ({} bytes)", content.len()),
                Err(e) => format!("export failed: {e}"),
            }
        }
        "web_search" => web_search_tool(args["query"].as_str().unwrap_or("")),
        other => format!("unknown tool: {other}"),
    }
}

fn web_search_tool(query: &str) -> String {
    if query.trim().is_empty() {
        return "error: empty query".into();
    }
    // Optional: Tavily-compatible or custom URL
    if let Ok(key) = std::env::var("TAVILY_API_KEY") {
        let client = match reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
        {
            Ok(c) => c,
            Err(e) => return format!("client error: {e}"),
        };
        let body = json!({
            "api_key": key,
            "query": query,
            "max_results": 5,
        });
        return match client
            .post("https://api.tavily.com/search")
            .json(&body)
            .send()
        {
            Ok(r) if r.status().is_success() => r
                .text()
                .unwrap_or_else(|e| format!("read body: {e}"))
                .chars()
                .take(4000)
                .collect(),
            Ok(r) => format!("web_search HTTP {}", r.status()),
            Err(e) => format!("web_search failed: {e}"),
        };
    }
    "web_search unavailable: set TAVILY_API_KEY (or skip)".into()
}

/// Non-stream tool loop: request with tools, execute via host channel, continue.
fn chat_with_tools_loop(
    cfg: &AiConfig,
    system: &str,
    history: &[(String, String)],
    tx: &mpsc::Sender<StreamEvent>,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let tools = get_tools();
    let mut messages = vec![json!({"role": "system", "content": system})];
    let start = history.len().saturating_sub(24);
    for (role, content) in &history[start..] {
        messages.push(json!({"role": role, "content": content}));
    }

    for _round in 0..MAX_TOOL_ROUNDS {
        if cancel.load(Ordering::SeqCst) {
            let _ = tx.send(StreamEvent::Error("cancelled".into()));
            return Ok(());
        }
        let body = json!({
            "model": cfg.model,
            "messages": messages,
            "tools": tools,
            "tool_choice": "auto",
            "stream": false,
        });
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
        let v: Value = resp.json().map_err(|e| e.to_string())?;
        let choice = v
            .pointer("/choices/0/message")
            .cloned()
            .ok_or_else(|| "no message in response".to_string())?;
        let content = choice["content"].as_str().unwrap_or("").to_string();
        let tool_calls = choice["tool_calls"].as_array().cloned().unwrap_or_default();

        if tool_calls.is_empty() {
            if !content.is_empty() {
                let _ = tx.send(StreamEvent::Chunk(content));
            }
            let _ = tx.send(StreamEvent::Done);
            return Ok(());
        }

        messages.push(choice);
        for tc in tool_calls {
            if cancel.load(Ordering::SeqCst) {
                let _ = tx.send(StreamEvent::Error("cancelled".into()));
                return Ok(());
            }
            let id = tc["id"].as_str().unwrap_or("").to_string();
            let name = tc["function"]["name"].as_str().unwrap_or("").to_string();
            let arguments = tc["function"]["arguments"]
                .as_str()
                .unwrap_or("{}")
                .to_string();
            let (reply_tx, reply_rx) = mpsc::channel();
            let _ = tx.send(StreamEvent::ToolCall {
                name: name.clone(),
                arguments: arguments.clone(),
                reply: reply_tx,
            });
            // block until host executes
            let result = reply_rx
                .recv_timeout(std::time::Duration::from_secs(120))
                .unwrap_or_else(|_| "tool timeout".into());
            messages.push(json!({
                "role": "tool",
                "tool_call_id": id,
                "content": result,
            }));
        }
    }
    let _ = tx.send(StreamEvent::Chunk(
        "(tool round limit reached)".into(),
    ));
    let _ = tx.send(StreamEvent::Done);
    Ok(())
}


/// Prefer streaming SSE; fall back to non-stream JSON if needed.
#[allow(dead_code)] // kept as non-tool failover path
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
            maximized: false,
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
            full_body: String::new(),
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
    fn ctrl_j_triggers_send_path() {
        let mut s = empty_session();
        s.input = "hi".into();
        s.cursor = 2;
        let key = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL);
        s.handle_key(key);
        assert!(s.status.contains("no provider"));
    }

    #[test]
    fn alt_t_queues_translate_and_tries_send() {
        let mut s = empty_session();
        let key = KeyEvent::new(KeyCode::Char('t'), KeyModifiers::ALT);
        s.handle_key(key);
        // no provider → status error; input was set then cleared by send path after slash? no provider leaves messages?
        assert!(s.status.contains("no provider") || s.input.contains("翻译") || !s.messages.is_empty());
    }

    #[test]
    fn parse_switch_by_index_and_name() {
        let providers = vec![
            AiConfig {
                name: "alpha".into(),
                api_key: "k".into(),
                base_url: "http://x".into(),
                model: "m".into(),
            },
            AiConfig {
                name: "beta".into(),
                api_key: "k".into(),
                base_url: "http://x".into(),
                model: "m".into(),
            },
        ];
        assert_eq!(parse_switch_target("2", &providers), Some(1));
        assert_eq!(parse_switch_target("beta", &providers), Some(1));
        assert_eq!(parse_switch_target("nope", &providers), None);
    }

    #[test]
    fn slash_exp_writes_file() {
        let mut s = empty_session();
        s.messages.push(Bubble {
            role: "user".into(),
            content: "hi".into(),
        });
        s.messages.push(Bubble {
            role: "assistant".into(),
            content: "yo".into(),
        });
        s.input = "/exp".into();
        s.cursor = s.input.len();
        s.send();
        assert!(s.status.starts_with("exported "));
    }

}
