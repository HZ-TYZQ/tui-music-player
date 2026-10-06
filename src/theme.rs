//! 界面的颜色职责和内置预设。
//!
//! 主题刻意不包含背景色：播放器继承终端背景，只允许选中行使用局部背景。
//! 配置文件中 `theme {}` 块的读取见 `config` 模块。

use ratatui::style::Color;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Theme {
    pub(crate) primary: Color,
    pub(crate) muted: Color,
    pub(crate) border: Color,
    pub(crate) selection_bg: Color,
    pub(crate) danger: Color,
    pub(crate) spectrum_low: Color,
    pub(crate) spectrum_high: Color,
}

/// 深色终端用的灰白层级。
pub(crate) const DEFAULT_THEME: Theme = Theme {
    primary: Color::Rgb(242, 242, 242),
    muted: Color::Rgb(184, 184, 184),
    border: Color::Rgb(138, 138, 138),
    selection_bg: Color::Rgb(52, 56, 70),
    danger: Color::Red,
    spectrum_low: Color::Rgb(168, 168, 168),
    spectrum_high: Color::Rgb(242, 242, 242),
};

/// 浅色终端用的深灰层级。
const LIGHT_THEME: Theme = Theme {
    primary: Color::Rgb(38, 38, 38),
    muted: Color::Rgb(92, 92, 92),
    border: Color::Rgb(138, 138, 138),
    selection_bg: Color::Rgb(220, 224, 234),
    danger: Color::Red,
    spectrum_low: Color::Rgb(138, 138, 138),
    spectrum_high: Color::Rgb(38, 38, 38),
};

/// 只用终端调色板里的颜色，跟着终端配色走，16 色终端也能正常显示。
const TERMINAL_THEME: Theme = Theme {
    primary: Color::Reset,
    muted: Color::Gray,
    border: Color::DarkGray,
    selection_bg: Color::DarkGray,
    danger: Color::Red,
    spectrum_low: Color::Gray,
    spectrum_high: Color::Reset,
};

const PRESETS: [(&str, Theme); 3] = [
    ("default", DEFAULT_THEME),
    ("light", LIGHT_THEME),
    ("terminal", TERMINAL_THEME),
];

/// 提示里列出的可选预设名。
pub(crate) const PRESET_NAMES: &str = "default、light、terminal";

impl Default for Theme {
    fn default() -> Self {
        DEFAULT_THEME
    }
}

impl Theme {
    /// 按名字查找内置预设，不区分大小写。
    pub(crate) fn preset(name: &str) -> Option<Self> {
        PRESETS
            .iter()
            .find(|(preset, _)| preset.eq_ignore_ascii_case(name))
            .map(|&(_, theme)| theme)
    }

    /// 配置里的颜色键对应的字段；不认识的键返回 None。
    pub(crate) fn color_mut(&mut self, key: &str) -> Option<&mut Color> {
        Some(match key {
            "primary" => &mut self.primary,
            "muted" => &mut self.muted,
            "border" => &mut self.border,
            "selection-bg" => &mut self.selection_bg,
            "danger" => &mut self.danger,
            "spectrum-low" => &mut self.spectrum_low,
            "spectrum-high" => &mut self.spectrum_high,
            _ => return None,
        })
    }
}

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

    #[test]
    fn presets_are_found_by_name_ignoring_case() {
        assert_eq!(Theme::preset("default"), Some(DEFAULT_THEME));
        assert_eq!(Theme::preset("Light"), Some(LIGHT_THEME));
        assert_eq!(Theme::preset("TERMINAL"), Some(TERMINAL_THEME));
        assert_eq!(Theme::preset("dark"), None);
        for (name, _) in PRESETS {
            assert!(PRESET_NAMES.contains(name));
        }
    }

    #[test]
    fn every_color_has_a_config_key() {
        let mut theme = DEFAULT_THEME;
        let keys = [
            "primary",
            "muted",
            "border",
            "selection-bg",
            "danger",
            "spectrum-low",
            "spectrum-high",
        ];
        for key in keys {
            *theme.color_mut(key).unwrap() = Color::Indexed(1);
        }
        assert_eq!(
            theme,
            Theme {
                primary: Color::Indexed(1),
                muted: Color::Indexed(1),
                border: Color::Indexed(1),
                selection_bg: Color::Indexed(1),
                danger: Color::Indexed(1),
                spectrum_low: Color::Indexed(1),
                spectrum_high: Color::Indexed(1),
            }
        );
        assert!(theme.color_mut("selection_bg").is_none());
        assert!(theme.color_mut("background").is_none());
    }
}
