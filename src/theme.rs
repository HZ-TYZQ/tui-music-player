//! 默认界面的颜色职责。
//!
//! 主题刻意不包含背景色：播放器继承终端背景，只允许选中行使用局部背景。

use ratatui::style::Color;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Theme {
    pub(crate) primary: Color,
    pub(crate) muted: Color,
    pub(crate) border: Color,
    pub(crate) selection_bg: Color,
    pub(crate) danger: Color,
    pub(crate) spectrum_low: Color,
    pub(crate) spectrum_high: Color,
}

pub(crate) const DEFAULT_THEME: Theme = Theme {
    primary: Color::Rgb(242, 242, 242),
    muted: Color::Rgb(184, 184, 184),
    border: Color::Rgb(138, 138, 138),
    selection_bg: Color::Rgb(52, 56, 70),
    danger: Color::Red,
    spectrum_low: Color::Rgb(168, 168, 168),
    spectrum_high: Color::Rgb(242, 242, 242),
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_theme_uses_fixed_white_levels_and_monochrome_spectrum() {
        assert_eq!(DEFAULT_THEME.primary, Color::Rgb(242, 242, 242));
        assert_eq!(DEFAULT_THEME.muted, Color::Rgb(184, 184, 184));
        assert_eq!(DEFAULT_THEME.border, Color::Rgb(138, 138, 138));
        assert_eq!(DEFAULT_THEME.selection_bg, Color::Rgb(52, 56, 70));
        assert_eq!(DEFAULT_THEME.danger, Color::Red);
        assert_eq!(DEFAULT_THEME.spectrum_low, Color::Rgb(168, 168, 168));
        assert_eq!(DEFAULT_THEME.spectrum_high, Color::Rgb(242, 242, 242));
    }
}
