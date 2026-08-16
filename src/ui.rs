//! Ratatui 界面：音乐库、播放状态、搜索和模态弹层。

use std::path::Path;
use std::time::Duration;

use ratatui::prelude::*;
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph, Wrap,
};

use crate::app::{App, Overlay};
use crate::player::PlayState;
use crate::theme::{DEFAULT_THEME, Theme};

const BASE_LAYOUT_HEIGHT: u16 = 11;
const MAX_VISUALIZER_HEIGHT: u16 = 5;
const MIN_VISUALIZER_HEIGHT: u16 = 2;
/// 正在播放时 Space 会暂停。
const PAUSE_ACTION_ICON: &str = "|| ";
/// 暂停时 Space 会继续播放。
const PLAY_ACTION_ICON: &str = ">  ";
/// 已停止。
const STOPPED_ICON: &str = "■  ";
/// 非当前行的指示器占位。四种指示器均为 3 个显示列（所涉字符宽度皆为 1），
/// 保证标题起始列不随播放状态变化而水平跳动。
const INACTIVE_ICON: &str = "   ";

pub fn draw(frame: &mut Frame, app: &App) {
    draw_with_theme(frame, app, &DEFAULT_THEME);
}

fn draw_with_theme(frame: &mut Frame, app: &App, theme: &Theme) {
    let area = frame.area();
    if area.width < 42 || area.height < 12 {
        frame.render_widget(
            Paragraph::new("终端窗口太小\n请调整到至少 42×12")
                .alignment(Alignment::Center)
                .style(Style::new().fg(theme.muted))
                .block(
                    Block::bordered()
                        .border_style(Style::new().fg(theme.border))
                        .title(Span::styled(
                            " Music Player ",
                            Style::new().fg(theme.primary).bold(),
                        )),
                ),
            area,
        );
        return;
    }

    let visualizer_height = visualizer_height(area.height, app.config.visualizer_enabled);
    if visualizer_height == 0 {
        let chunks = Layout::vertical([
            Constraint::Min(6),
            Constraint::Length(4),
            Constraint::Length(1),
        ])
        .split(area);
        draw_library(frame, app, chunks[0], theme);
        draw_now_playing(frame, app, chunks[1], theme);
        draw_footer(frame, app, chunks[2], theme);
    } else {
        let chunks = Layout::vertical([
            Constraint::Min(6),
            Constraint::Length(visualizer_height),
            Constraint::Length(4),
            Constraint::Length(1),
        ])
        .split(area);
        draw_library(frame, app, chunks[0], theme);
        draw_visualizer(frame, app, chunks[1], theme);
        draw_now_playing(frame, app, chunks[2], theme);
        draw_footer(frame, app, chunks[3], theme);
    }
    draw_overlay(frame, app, theme);
}

fn visualizer_height(terminal_height: u16, enabled: bool) -> u16 {
    if !enabled {
        return 0;
    }
    let available = terminal_height.saturating_sub(BASE_LAYOUT_HEIGHT);
    if available < MIN_VISUALIZER_HEIGHT {
        0
    } else {
        available.min(MAX_VISUALIZER_HEIGHT)
    }
}

fn draw_library(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let scan = if app.scanning {
        format!(" · 扫描中 {}/{} ", app.scan_progress.0, app.scan_progress.1)
    } else {
        format!(" · {} 首 ", app.visible_indices().len())
    };
    let title = Line::from(vec![
        Span::styled(" ♪ Music Player ", Style::new().fg(theme.primary).bold()),
        Span::styled(
            format!("· {}", app.library_dir.display()),
            Style::new().fg(theme.muted),
        ),
        Span::styled(scan, Style::new().fg(theme.muted)),
    ]);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(theme.border))
        .title(title);

    if app.tracks.is_empty() {
        let text = if app.scanning {
            "  正在后台扫描音乐库……\n  界面仍可响应，扫描完成后歌曲会自动出现"
        } else {
            "  音乐库中没有支持的音频文件\n  按 r 重新扫描，或用 --set-library PATH 更换主库"
        };
        frame.render_widget(
            Paragraph::new(text)
                .style(Style::new().fg(theme.muted))
                .block(block),
            area,
        );
        return;
    }

    let items = app.visible_indices().iter().filter_map(|index| {
        let track = app.tracks.get(*index)?;
        let current = app.playing_index == Some(*index);
        let (icon, icon_style) = if current {
            playback_action_indicator(app.player.state(), theme)
        } else {
            (INACTIVE_ICON, Style::new().fg(theme.primary))
        };
        let title_style = Style::new().fg(theme.primary);
        let title_style = if current {
            title_style.bold()
        } else {
            title_style
        };
        let artist = track.artist.as_deref().unwrap_or("未知歌手");
        let album = track.album.as_deref().unwrap_or("未知专辑");
        let format = track.format.as_deref().unwrap_or("?");
        let duration = track
            .duration
            .map(fmt_duration)
            .unwrap_or_else(|| "--:--".into());
        Some(ListItem::new(Line::from(vec![
            Span::styled(icon, icon_style),
            Span::styled(track.display_title(), title_style),
            "  ".into(),
            Span::styled(artist, Style::new().fg(theme.muted)),
            Span::styled(" · ", Style::new().fg(theme.muted)),
            Span::styled(album, Style::new().fg(theme.muted)),
            Span::styled(" · ", Style::new().fg(theme.muted)),
            Span::styled(format, Style::new().fg(theme.muted)),
            Span::styled(" · ", Style::new().fg(theme.muted)),
            Span::styled(duration, Style::new().fg(theme.muted)),
        ])))
    });

    let list = List::new(items)
        .block(block)
        .highlight_style(Style::new().bg(theme.selection_bg).bold())
        .highlight_symbol(Span::styled("▸ ", Style::new().fg(theme.primary)));
    let selected = (!app.visible_indices().is_empty()).then_some(app.selected);
    let mut state = ListState::default().with_selected(selected);
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_now_playing(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(theme.border))
        .title(Span::styled(
            " ♫ 正在播放 ",
            Style::new().fg(theme.primary).bold(),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(inner);
    let top = Layout::horizontal([Constraint::Min(10), Constraint::Length(30)]).split(rows[0]);
    let now = match app.current_track() {
        Some(track) => {
            let (icon, style) = playback_action_indicator(app.player.state(), theme);
            Line::from(vec![
                Span::styled(icon, style),
                Span::styled(track.display_title(), Style::new().fg(theme.primary).bold()),
                Span::styled(
                    track
                        .artist
                        .as_ref()
                        .map(|artist| format!(" — {artist}"))
                        .unwrap_or_default(),
                    Style::new().fg(theme.muted),
                ),
            ])
        }
        None => Line::from(Span::styled("■ 未在播放", Style::new().fg(theme.muted))),
    };
    frame.render_widget(Paragraph::new(now), top[0]);

    let volume = if app.player.is_muted() {
        "静音".to_owned()
    } else {
        format!("{}%", app.player.volume())
    };
    frame.render_widget(
        Paragraph::new(format!(
            "{} · 音量 {} · 队列 {}",
            app.config.play_mode.label(),
            volume,
            app.queue.len()
        ))
        .alignment(Alignment::Right)
        .style(Style::new().fg(theme.muted)),
        top[1],
    );

    let position = app.player.position();
    let duration = app
        .player
        .duration()
        .or_else(|| app.current_track()?.duration);
    let ratio = duration
        .filter(|duration| !duration.is_zero())
        .map(|duration| position.as_secs_f64() / duration.as_secs_f64())
        .unwrap_or_default()
        .clamp(0.0, 1.0);
    let label = format!(
        "{} / {}",
        fmt_duration(position),
        duration.map(fmt_duration).unwrap_or_else(|| "--:--".into())
    );
    frame.render_widget(
        Gauge::default()
            .gauge_style(Style::new().fg(theme.primary))
            .ratio(ratio)
            .label(label),
        rows[1],
    );
}

fn playback_action_indicator(state: PlayState, theme: &Theme) -> (&'static str, Style) {
    match state {
        PlayState::Playing => (PAUSE_ACTION_ICON, Style::new().fg(theme.primary).bold()),
        PlayState::Paused => (PLAY_ACTION_ICON, Style::new().fg(theme.primary).bold()),
        PlayState::Stopped => (STOPPED_ICON, Style::new().fg(theme.muted)),
    }
}

fn draw_visualizer(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::new().fg(theme.border))
        .title(Span::styled(
            " 频谱 · 50 Hz → 8 kHz ",
            Style::new().fg(theme.primary).bold(),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.is_empty() {
        return;
    }

    let bar_count = app.spectrum.len().min(usize::from(inner.width).div_ceil(2));
    let bars = resample_spectrum(&app.spectrum, bar_count);
    let plot_width = bars.len().saturating_mul(2).saturating_sub(1);
    let left_padding = (usize::from(inner.width).saturating_sub(plot_width)) / 2;
    let right_padding = usize::from(inner.width)
        .saturating_sub(left_padding)
        .saturating_sub(plot_width);
    for row in 0..inner.height {
        let remaining_rows = f32::from(inner.height - row - 1);
        let mut spans = Vec::with_capacity(bars.len().saturating_mul(2) + 2);
        spans.push(Span::raw(" ".repeat(left_padding)));
        for (index, value) in bars.iter().enumerate() {
            let cell_fill = (value * f32::from(inner.height) - remaining_rows).clamp(0.0, 1.0);
            spans.push(Span::styled(
                spectrum_block(cell_fill).to_string(),
                Style::new().fg(frequency_color(index, bars.len(), theme)),
            ));
            if index + 1 < bars.len() {
                spans.push(Span::raw(" "));
            }
        }
        spans.push(Span::raw(" ".repeat(right_padding)));
        let row_area = Rect::new(inner.x, inner.y + row, inner.width, 1);
        frame.render_widget(Paragraph::new(Line::from(spans)), row_area);
    }
}

fn resample_spectrum(values: &[f32], width: usize) -> Vec<f32> {
    if values.is_empty() || width == 0 {
        return Vec::new();
    }

    (0..width)
        .map(|column| {
            let start = column * values.len() / width;
            let end = ((column + 1) * values.len()).div_ceil(width);
            values[start..end.max(start + 1).min(values.len())]
                .iter()
                .copied()
                .filter(|value| value.is_finite())
                .fold(0.0_f32, f32::max)
                .clamp(0.0, 1.0)
        })
        .collect()
}

fn spectrum_block(fill: f32) -> char {
    match (fill.clamp(0.0, 1.0) * 8.0).ceil() as u8 {
        0 => ' ',
        1 => '▁',
        2 => '▂',
        3 => '▃',
        4 => '▄',
        5 => '▅',
        6 => '▆',
        7 => '▇',
        _ => '█',
    }
}

fn frequency_color(index: usize, count: usize, theme: &Theme) -> Color {
    let ratio = if count <= 1 {
        0.0
    } else {
        index as f32 / (count - 1) as f32
    };
    let (Color::Rgb(low_r, low_g, low_b), Color::Rgb(high_r, high_g, high_b)) =
        (theme.spectrum_low, theme.spectrum_high)
    else {
        return theme.spectrum_low;
    };
    let mix =
        |low: u8, high: u8| (f32::from(low) + (f32::from(high) - f32::from(low)) * ratio) as u8;
    Color::Rgb(mix(low_r, high_r), mix(low_g, high_g), mix(low_b, high_b))
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let line = if app.search_active {
        Line::from(vec![
            Span::styled(" / ", Style::new().fg(theme.primary).bold()),
            Span::styled(app.search.query(), Style::new().fg(theme.primary)),
            Span::styled("█", Style::new().fg(theme.primary)),
            Span::styled(
                format!(
                    "  {} 个结果 · Enter 播放 · Esc 清除",
                    app.visible_indices().len()
                ),
                Style::new().fg(theme.muted),
            ),
        ])
    } else if let Some(message) = &app.message {
        Line::from(vec![
            Span::styled(" • ", Style::new().fg(theme.primary)),
            Span::styled(message.clone(), Style::new().fg(theme.primary)),
        ])
    } else {
        Line::from(Span::styled(
            " ↑↓/jk 选择 · Enter 播放 · Space 暂停 · / 搜索 · v 频谱 · P 列表 · ? 帮助 · q 退出",
            Style::new().fg(theme.muted),
        ))
    };
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_overlay(frame: &mut Frame, app: &App, theme: &Theme) {
    match app.overlay {
        Overlay::None => {}
        Overlay::Help => draw_text_popup(
            frame,
            " 快捷键帮助 ",
            vec![
                "↑/↓ 或 j/k    选择歌曲",
                "Enter          立即播放（保留队列）",
                "Space          暂停 / 继续",
                "←/→ 或 h/l    后退 / 前进 10 秒",
                "- / =          音量降低 / 提高 5%",
                "m              静音",
                "n / p          下一首 / 上一首历史",
                "z              切换播放模式",
                "v              显示 / 隐藏音频频谱",
                "/              实时模糊搜索",
                "r              后台重新扫描",
                "a / A          加到队尾 / 设为下一首",
                "P              播放列表",
                "? / Esc        关闭帮助",
                "q              退出",
            ],
            62,
            19,
            theme,
        ),
        Overlay::Playlists => draw_playlists(frame, app, theme),
        Overlay::PlaylistTracks => draw_playlist_tracks(frame, app, theme),
        Overlay::NameInput => draw_text_popup(
            frame,
            " 新建播放列表 ",
            vec![
                "请输入名称：",
                &format!("> {}█", app.name_input),
                "",
                "Enter 创建 · Esc 取消",
            ],
            58,
            8,
            theme,
        ),
        Overlay::DeleteConfirm => {
            let name = app
                .playlists
                .all()
                .get(app.playlist_selected)
                .map(|playlist| playlist.name.as_str())
                .unwrap_or("");
            draw_text_popup(
                frame,
                " 确认删除 ",
                vec![
                    &format!("删除播放列表“{name}”？"),
                    "音乐文件不会被删除。",
                    "",
                    "y 确认 · n/Esc 取消",
                ],
                56,
                8,
                theme,
            );
        }
    }
}

fn draw_playlists(frame: &mut Frame, app: &App, theme: &Theme) {
    let area = centered(frame.area(), 70, 70);
    frame.render_widget(Clear, area);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .title(Span::styled(
            " 播放列表 ",
            Style::new().fg(theme.primary).bold(),
        ))
        .border_style(Style::new().fg(theme.border));
    let items = if app.playlists.all().is_empty() {
        vec![ListItem::new(Span::styled(
            "暂无播放列表，按 c 创建",
            Style::new().fg(theme.muted),
        ))]
    } else {
        app.playlists
            .all()
            .iter()
            .map(|playlist| {
                ListItem::new(format!("{}  ({} 首)", playlist.name, playlist.tracks.len()))
            })
            .collect()
    };
    let list = List::new(items)
        .block(block)
        .highlight_symbol(Span::styled("▸ ", Style::new().fg(theme.primary)))
        .highlight_style(Style::new().bg(theme.selection_bg).bold());
    let selected = (!app.playlists.all().is_empty()).then_some(app.playlist_selected);
    let mut state = ListState::default().with_selected(selected);
    frame.render_stateful_widget(list, area, &mut state);
    let help = Rect::new(
        area.x + 2,
        area.bottom().saturating_sub(2),
        area.width.saturating_sub(4),
        1,
    );
    frame.render_widget(
        Paragraph::new("c 新建 · a 加入选中歌曲 · Enter 查看 · x 删除 · Esc 关闭")
            .style(Style::new().fg(theme.muted)),
        help,
    );
}

fn draw_playlist_tracks(frame: &mut Frame, app: &App, theme: &Theme) {
    let area = centered(frame.area(), 78, 76);
    frame.render_widget(Clear, area);
    let Some(playlist) = app.playlists.all().get(app.playlist_selected) else {
        return;
    };
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .title(Span::styled(
            format!(" {} ", playlist.name),
            Style::new().fg(theme.primary).bold(),
        ))
        .border_style(Style::new().fg(theme.border));
    let items = playlist.tracks.iter().map(|path| {
        let missing = !path.is_file();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("?");
        if missing {
            ListItem::new(Line::from(vec![
                Span::styled("⚠ ", Style::new().fg(theme.danger)),
                Span::styled(name, Style::new().fg(theme.danger)),
                Span::styled("  文件不可用", Style::new().fg(theme.muted)),
            ]))
        } else {
            ListItem::new(name)
        }
    });
    let list = List::new(items)
        .block(block)
        .highlight_symbol(Span::styled("▸ ", Style::new().fg(theme.primary)))
        .highlight_style(Style::new().bg(theme.selection_bg).bold());
    let selected = (!playlist.tracks.is_empty()).then_some(app.playlist_track_selected);
    let mut state = ListState::default().with_selected(selected);
    frame.render_stateful_widget(list, area, &mut state);
    let help = Rect::new(
        area.x + 2,
        area.bottom().saturating_sub(2),
        area.width.saturating_sub(4),
        1,
    );
    frame.render_widget(
        Paragraph::new("Enter 从此处播放 · d 从列表移除 · Esc 返回")
            .style(Style::new().fg(theme.muted)),
        help,
    );
}

fn draw_text_popup(
    frame: &mut Frame,
    title: &str,
    lines: Vec<&str>,
    width: u16,
    height: u16,
    theme: &Theme,
) {
    let area = centered(frame.area(), width, height);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines.join("\n"))
            .style(Style::new().fg(theme.primary))
            .wrap(Wrap { trim: false })
            .block(
                Block::bordered()
                    .border_type(BorderType::Rounded)
                    .border_style(Style::new().fg(theme.border))
                    .title(Span::styled(title, Style::new().fg(theme.primary).bold())),
            ),
        area,
    );
}

fn centered(area: Rect, max_width: u16, max_height: u16) -> Rect {
    let width = max_width.min(area.width.saturating_sub(4)).max(10);
    let height = max_height.min(area.height.saturating_sub(2)).max(5);
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

pub fn fmt_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    let (hours, minutes, seconds) = (seconds / 3600, (seconds % 3600) / 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

#[allow(dead_code)]
fn _path_reference(path: &Path) -> &Path {
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_short_and_long_durations() {
        assert_eq!(fmt_duration(Duration::from_secs(65)), "1:05");
        assert_eq!(fmt_duration(Duration::from_secs(3661)), "1:01:01");
    }

    #[test]
    fn visualizer_uses_available_height_without_breaking_base_layout() {
        assert_eq!(visualizer_height(12, true), 0);
        assert_eq!(visualizer_height(13, true), 2);
        assert_eq!(visualizer_height(16, true), 5);
        assert_eq!(visualizer_height(40, true), 5);
        assert_eq!(visualizer_height(40, false), 0);
    }

    #[test]
    fn spectrum_resampling_handles_narrow_wide_and_invalid_input() {
        assert_eq!(resample_spectrum(&[0.1, 0.8, 0.3, 0.6], 2), vec![0.8, 0.6]);
        assert_eq!(
            resample_spectrum(&[0.25, 0.75], 4),
            vec![0.25, 0.25, 0.75, 0.75]
        );
        assert_eq!(resample_spectrum(&[f32::NAN, 2.0], 2), vec![0.0, 1.0]);
        assert!(resample_spectrum(&[], 10).is_empty());
        assert!(resample_spectrum(&[0.5], 0).is_empty());
    }

    #[test]
    fn spectrum_blocks_cover_empty_fractional_and_full_cells() {
        assert_eq!(spectrum_block(0.0), ' ');
        assert_eq!(spectrum_block(0.01), '▁');
        assert_eq!(spectrum_block(0.5), '▄');
        assert_eq!(spectrum_block(1.0), '█');
    }

    #[test]
    fn playback_icon_describes_the_space_key_action() {
        assert_eq!(
            playback_action_indicator(PlayState::Playing, &DEFAULT_THEME).0,
            PAUSE_ACTION_ICON
        );
        assert_eq!(
            playback_action_indicator(PlayState::Paused, &DEFAULT_THEME).0,
            PLAY_ACTION_ICON
        );
        assert_eq!(
            playback_action_indicator(PlayState::Stopped, &DEFAULT_THEME).0,
            STOPPED_ICON
        );
    }

    #[test]
    fn playback_icons_have_fixed_display_width() {
        // 指示器均为 ASCII/单宽字符，chars().count() 即显示列数。
        for state in [PlayState::Playing, PlayState::Paused, PlayState::Stopped] {
            assert_eq!(
                playback_action_indicator(state, &DEFAULT_THEME)
                    .0
                    .chars()
                    .count(),
                3
            );
        }
        assert_eq!(INACTIVE_ICON.chars().count(), 3);
    }

    #[test]
    fn playback_actions_share_primary_color_and_stopped_is_muted() {
        let playing = playback_action_indicator(PlayState::Playing, &DEFAULT_THEME).1;
        let paused = playback_action_indicator(PlayState::Paused, &DEFAULT_THEME).1;
        let stopped = playback_action_indicator(PlayState::Stopped, &DEFAULT_THEME).1;

        assert_eq!(playing, paused);
        assert_eq!(playing.fg, Some(DEFAULT_THEME.primary));
        assert_eq!(stopped.fg, Some(DEFAULT_THEME.muted));
    }

    #[test]
    fn spectrum_gradient_uses_theme_endpoints() {
        assert_eq!(
            frequency_color(0, 32, &DEFAULT_THEME),
            Color::Rgb(168, 168, 168)
        );
        assert_eq!(
            frequency_color(31, 32, &DEFAULT_THEME),
            Color::Rgb(242, 242, 242)
        );
    }
}
