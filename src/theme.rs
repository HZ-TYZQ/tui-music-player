//! 界面的颜色职责、内置预设，以及按配置得出最终主题。
//!
//! 主题刻意不包含背景色：播放器继承终端背景，只允许选中行使用局部背景。

use ratatui::style::Color;

use crate::config::ThemeConfig;

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

impl Default for Theme {
    fn default() -> Self {
        DEFAULT_THEME
    }
}

impl Theme {
    /// 先取预设，再逐项覆盖。预设名或某项颜色写错时只回退那一处，
    /// 其余照常生效，并返回一行说明哪里写错了。
    pub(crate) fn from_config(config: &ThemeConfig) -> (Self, Option<String>) {
        let mut problems = Vec::new();
        let name = config.preset.trim();
        let mut theme = match PRESETS
            .iter()
            .find(|(preset, _)| preset.eq_ignore_ascii_case(name))
        {
            Some(&(_, theme)) => theme,
            None => {
                problems.push(format!("未知预设 \"{name}\"，已改用 default"));
                DEFAULT_THEME
            }
        };
        let overrides = [
            ("primary", &config.primary, &mut theme.primary),
            ("muted", &config.muted, &mut theme.muted),
            ("border", &config.border, &mut theme.border),
            (
                "selection_bg",
                &config.selection_bg,
                &mut theme.selection_bg,
            ),
            ("danger", &config.danger, &mut theme.danger),
            (
                "spectrum_low",
                &config.spectrum_low,
                &mut theme.spectrum_low,
            ),
            (
                "spectrum_high",
                &config.spectrum_high,
                &mut theme.spectrum_high,
            ),
        ];
        for (key, value, color) in overrides {
            let Some(value) = value else {
                continue;
            };
            match value.trim().parse() {
                Ok(parsed) => *color = parsed,
                Err(_) => problems.push(format!("{key} 的值 \"{value}\" 无法识别，已沿用预设")),
            }
        }
        for key in config.unknown.keys() {
            problems.push(format!("未知项 {key} 已忽略"));
        }
        let warning =
            (!problems.is_empty()).then(|| format!("主题配置有误: {}", problems.join("；")));
        (theme, warning)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolve(toml: &str) -> (Theme, Option<String>) {
        Theme::from_config(&toml::from_str(toml).unwrap())
    }

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
    fn an_empty_theme_section_is_the_default_preset() {
        assert_eq!(resolve(""), (DEFAULT_THEME, None));
        assert_eq!(
            Theme::from_config(&ThemeConfig::default()),
            (DEFAULT_THEME, None)
        );
    }

    #[test]
    fn presets_are_found_by_name_ignoring_case_and_spaces() {
        assert_eq!(resolve("preset = \"light\""), (LIGHT_THEME, None));
        assert_eq!(resolve("preset = \" Terminal \""), (TERMINAL_THEME, None));
    }

    #[test]
    fn overrides_replace_single_colors_of_the_preset() {
        let (theme, warning) = resolve(
            r##"
            preset = "light"
            primary = "#102030"
            muted = "dark-gray"
            selection_bg = "236"
            danger = " reset "
            "##,
        );
        assert_eq!(warning, None);
        assert_eq!(
            theme,
            Theme {
                primary: Color::Rgb(16, 32, 48),
                muted: Color::DarkGray,
                selection_bg: Color::Indexed(236),
                danger: Color::Reset,
                ..LIGHT_THEME
            }
        );
    }

    #[test]
    fn a_bad_color_falls_back_alone_and_is_reported() {
        let (theme, warning) = resolve(
            r##"
            preset = "light"
            primary = "#12345"
            border = "#ABCDEF"
            "##,
        );
        assert_eq!(
            theme,
            Theme {
                border: Color::Rgb(171, 205, 239),
                ..LIGHT_THEME
            }
        );
        let warning = warning.unwrap();
        assert!(
            warning.contains("primary 的值 \"#12345\" 无法识别"),
            "{warning}"
        );
        assert!(!warning.contains("border"), "{warning}");
    }

    #[test]
    fn an_unknown_preset_falls_back_to_default_but_keeps_overrides() {
        let (theme, warning) = resolve(
            r##"
            preset = "dark"
            primary = "white"
            "##,
        );
        assert_eq!(
            theme,
            Theme {
                primary: Color::White,
                ..DEFAULT_THEME
            }
        );
        assert!(
            warning
                .unwrap()
                .contains("未知预设 \"dark\"，已改用 default")
        );
    }

    #[test]
    fn unknown_keys_are_reported_together() {
        let (theme, warning) = resolve(
            r##"
            preset = "nope"
            primay = "#FFFFFF"
            background = "black"
            "##,
        );
        assert_eq!(theme, DEFAULT_THEME);
        assert_eq!(
            warning.unwrap(),
            "主题配置有误: 未知预设 \"nope\"，已改用 default；\
             未知项 background 已忽略；未知项 primay 已忽略"
        );
    }
}
