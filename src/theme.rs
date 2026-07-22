//! Colour roles — ported from mdx-tui (dark truecolor).

use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    #[default]
    Dark,
}

impl Theme {
    pub fn search_text(self) -> Color {
        Color::Rgb(255, 200, 40)
    }

    pub fn headword_text(self) -> Color {
        Color::Rgb(120, 160, 130)
    }

    pub fn headword_selected(self) -> Style {
        Style::default()
            .fg(Color::Rgb(10, 24, 16))
            .bg(Color::Rgb(46, 204, 113))
            .add_modifier(Modifier::BOLD)
    }

    #[allow(dead_code)] // secondary list (dict panel) — kept for parity with mdx-tui
    pub fn list_text(self) -> Color {
        Color::Rgb(180, 190, 200)
    }

    #[allow(dead_code)]
    pub fn list_selected(self) -> Style {
        Style::default()
            .fg(Color::Rgb(12, 18, 28))
            .bg(Color::Rgb(80, 140, 220))
            .add_modifier(Modifier::BOLD)
    }

    pub fn border(self) -> Color {
        Color::Rgb(70, 80, 95)
    }

    pub fn muted(self) -> Color {
        Color::Rgb(130, 140, 155)
    }

    pub fn accent(self) -> Color {
        Color::Rgb(255, 200, 40)
    }

    pub fn key_label(self) -> Color {
        Color::Rgb(100, 200, 140)
    }

    pub fn status_focus_bg(self) -> Color {
        Color::Rgb(46, 204, 113)
    }

    pub fn status_focus_fg(self) -> Color {
        Color::Rgb(10, 24, 16)
    }

    pub fn vim_prompt(self) -> Color {
        self.search_text()
    }

    pub fn title(self) -> Color {
        Color::Rgb(100, 200, 140)
    }

    pub fn help_bg(self) -> Color {
        Color::Rgb(24, 28, 36)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_uses_solid_bg() {
        let s = Theme::Dark.headword_selected();
        assert!(s.bg.is_some());
    }
}
