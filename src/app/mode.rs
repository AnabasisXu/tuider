//! Input mode derived from App flags (readability for key routing).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Help,
    #[cfg(feature = "ai")]
    Ai,
    Nav,
    VimSearch,
    Visual,
    Normal,
}

/// Pure derivation for tests and `App::input_mode`.
pub fn derive_input_mode(
    show_help: bool,
    #[cfg(feature = "ai")] ai_open: bool,
    nav_open: bool,
    vim_mode: bool,
    visual: bool,
) -> InputMode {
    if show_help {
        return InputMode::Help;
    }
    #[cfg(feature = "ai")]
    if ai_open {
        return InputMode::Ai;
    }
    if nav_open {
        return InputMode::Nav;
    }
    if vim_mode {
        return InputMode::VimSearch;
    }
    if visual {
        return InputMode::Visual;
    }
    InputMode::Normal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_help_over_all() {
        assert_eq!(
            derive_input_mode(
                true,
                #[cfg(feature = "ai")]
                true,
                true,
                true,
                true
            ),
            InputMode::Help
        );
    }

    #[cfg(feature = "ai")]
    #[test]
    fn priority_ai_over_nav() {
        assert_eq!(
            derive_input_mode(false, true, true, true, true),
            InputMode::Ai
        );
    }

    #[test]
    fn priority_nav_vim_visual_normal() {
        assert_eq!(
            derive_input_mode(
                false,
                #[cfg(feature = "ai")]
                false,
                true,
                true,
                true
            ),
            InputMode::Nav
        );
        assert_eq!(
            derive_input_mode(
                false,
                #[cfg(feature = "ai")]
                false,
                false,
                true,
                true
            ),
            InputMode::VimSearch
        );
        assert_eq!(
            derive_input_mode(
                false,
                #[cfg(feature = "ai")]
                false,
                false,
                false,
                true
            ),
            InputMode::Visual
        );
        assert_eq!(
            derive_input_mode(
                false,
                #[cfg(feature = "ai")]
                false,
                false,
                false,
                false
            ),
            InputMode::Normal
        );
    }
}
