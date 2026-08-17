//! Ratatui 界面：音乐库、播放状态、搜索和模态弹层。

use std::path::Path;
use std::time::Duration;

use ratatui::prelude::*;
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap,
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::{App, Overlay};
use crate::player::PlayState;
use crate::theme::{DEFAULT_THEME, Theme};
use crate::track::Track;

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

/// 播放状态指示器列宽（与上面的指示器常量保持一致）。
const LIST_ICON_WIDTH: usize = 3;
/// 歌曲列表相邻列之间的空格数。
const LIST_GAP_WIDTH: usize = 2;
/// 格式列固定宽度。
const FORMAT_COLUMN_WIDTH: usize = 6;
const MIN_TITLE_WIDTH: usize = 12;
const MIN_ARTIST_WIDTH: usize = 8;
const MIN_ALBUM_WIDTH: usize = 8;
/// List 的 highlight 符号（"▸ "）宽度；ratatui 会为所有行预留这部分宽度。
const HIGHLIGHT_SYMBOL_WIDTH: usize = 2;

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

    // 可用宽度：area 减去块边框（2 列）与 List 为所有行预留的 highlight 符号宽。
    let usable = usize::from(area.width).saturating_sub(2 + HIGHLIGHT_SYMBOL_WIDTH);
    let layout = track_row_layout(usable, app.duration_column_width as usize);
    let muted = Style::new().fg(theme.muted);
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
        let mut columns = track_row_columns(track, &layout).into_iter();
        let title = columns.next()?;
        let mut spans = vec![
            Span::styled(icon, icon_style),
            Span::styled(title, title_style),
        ];
        for column in columns {
            spans.push(Span::styled("  ", muted));
            spans.push(Span::styled(column, muted));
        }
        Some(ListItem::new(Line::from(spans)))
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
            let budget = usize::from(top[0].width).saturating_sub(LIST_ICON_WIDTH);
            let (title, artist) =
                now_playing_text(track.display_title(), track.artist.as_deref(), budget);
            Line::from(vec![
                Span::styled(icon, style),
                Span::styled(title, Style::new().fg(theme.primary).bold()),
                Span::styled(artist, Style::new().fg(theme.muted)),
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
    let position_label = fmt_duration(position);
    let duration_label = duration.map(fmt_duration).unwrap_or_else(|| "--:--".into());
    // 布局：' ' pos ' ' bar ' ' dur，bar 填满剩余宽度。
    let bar_width =
        (rows[1].width as usize).saturating_sub(position_label.len() + duration_label.len() + 3);
    let bar = ascii_progress_bar(ratio, bar_width);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!(" {position_label} "), Style::new().fg(theme.muted)),
            Span::styled(bar, Style::new().fg(theme.primary)),
            Span::styled(format!(" {duration_label}"), Style::new().fg(theme.muted)),
        ])),
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
        .border_style(Style::new().fg(theme.border));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.is_empty() {
        return;
    }

    let spectrum = app.spectrum_bars();
    let bar_count = spectrum.len().min(usize::from(inner.width).div_ceil(2));
    let bars = resample_spectrum(spectrum, bar_count);
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

/// 生成 width 列的 ASCII 进度条：'=' 已播放、'>' 播放头、'-' 未播放。
/// filled 用 floor 保证比例未满时不虚报；ratio >= 1.0 时全部填满为 '='，
/// 末尾不再保留播放头。
fn ascii_progress_bar(ratio: f64, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let ratio = ratio.clamp(0.0, 1.0);
    if ratio >= 1.0 {
        return "=".repeat(width);
    }
    let filled = (ratio * width as f64).floor() as usize;
    let mut bar = "=".repeat(filled);
    bar.push('>');
    bar.push_str(&"-".repeat(width - filled - 1));
    bar
}

/// 按终端显示宽度截断：超出时保留 max_width-1 列内容后接 "…"（宽度 1）。
/// CJK 字符按 2 列、控制字符按 0 列计（unicode-width）。
fn truncate_display(text: &str, max_width: usize) -> String {
    if UnicodeWidthStr::width(text) <= max_width {
        return text.to_owned();
    }
    if max_width == 0 {
        return String::new();
    }
    // 为省略号预留 1 列。
    let mut used = 0;
    let mut truncated = String::new();
    for character in text.chars() {
        let character_width = UnicodeWidthChar::width(character).unwrap_or(0);
        if used + character_width + 1 > max_width {
            break;
        }
        truncated.push(character);
        used += character_width;
    }
    truncated.push('…');
    truncated
}

/// 把一列文本截断并补齐到 width 列；right 时右对齐（用于时长列）。
fn column_text(text: &str, width: usize, right: bool) -> String {
    let truncated = truncate_display(text, width);
    let padding = " ".repeat(width.saturating_sub(UnicodeWidthStr::width(truncated.as_str())));
    if right {
        format!("{padding}{truncated}")
    } else {
        format!("{truncated}{padding}")
    }
}

/// 歌曲列表一行的列宽布局：按可用宽度从完整到精简逐级收缩，分档阈值由各列
/// 最小宽、固定列宽与分隔宽推导，不写死魔法数字。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TrackRowLayout {
    title: usize,
    artist: Option<usize>,
    album: Option<usize>,
    format: bool,
    duration: usize,
}

fn track_row_layout(usable: usize, duration_width: usize) -> TrackRowLayout {
    // (artist, album, format)：从最完整到最精简依次尝试。
    let tiers = [
        (true, true, true),
        (true, true, false),
        (true, false, false),
        (false, false, false),
    ];
    for &(show_artist, show_album, show_format) in &tiers {
        let content_columns =
            2 + usize::from(show_artist) + usize::from(show_album) + usize::from(show_format);
        let minimum = LIST_ICON_WIDTH
            + duration_width
            + if show_format { FORMAT_COLUMN_WIDTH } else { 0 }
            + MIN_TITLE_WIDTH
            + if show_artist { MIN_ARTIST_WIDTH } else { 0 }
            + if show_album { MIN_ALBUM_WIDTH } else { 0 }
            + LIST_GAP_WIDTH * (content_columns - 1);
        if usable >= minimum {
            return distribute_row_width(
                usable,
                duration_width,
                show_artist,
                show_album,
                show_format,
            );
        }
    }
    // 再窄也退化为仅标题+时长；全局最小终端尺寸（42 列）保证走不到这里。
    distribute_row_width(usable, duration_width, false, false, false)
}

/// 先满足各列最小宽，再把剩余空间按比例分配给可变列；取整余数归标题（最高优先级）。
fn distribute_row_width(
    usable: usize,
    duration_width: usize,
    show_artist: bool,
    show_album: bool,
    show_format: bool,
) -> TrackRowLayout {
    let content_columns =
        2 + usize::from(show_artist) + usize::from(show_album) + usize::from(show_format);
    let fixed = LIST_ICON_WIDTH
        + duration_width
        + LIST_GAP_WIDTH * (content_columns - 1)
        + if show_format { FORMAT_COLUMN_WIDTH } else { 0 };
    let variable_area = usable.saturating_sub(fixed);
    if show_artist && show_album {
        let extra =
            variable_area.saturating_sub(MIN_TITLE_WIDTH + MIN_ARTIST_WIDTH + MIN_ALBUM_WIDTH);
        let mut title = MIN_TITLE_WIDTH + extra * 45 / 100;
        let artist = MIN_ARTIST_WIDTH + extra * 30 / 100;
        let album = MIN_ALBUM_WIDTH + extra * 25 / 100;
        title += variable_area.saturating_sub(title + artist + album);
        TrackRowLayout {
            title,
            artist: Some(artist),
            album: Some(album),
            format: show_format,
            duration: duration_width,
        }
    } else if show_artist {
        let extra = variable_area.saturating_sub(MIN_TITLE_WIDTH + MIN_ARTIST_WIDTH);
        let mut title = MIN_TITLE_WIDTH + extra * 60 / 100;
        let artist = MIN_ARTIST_WIDTH + extra * 40 / 100;
        title += variable_area.saturating_sub(title + artist);
        TrackRowLayout {
            title,
            artist: Some(artist),
            album: None,
            format: show_format,
            duration: duration_width,
        }
    } else {
        TrackRowLayout {
            title: variable_area,
            artist: None,
            album: None,
            format: show_format,
            duration: duration_width,
        }
    }
}

/// 按布局生成一行各列的文本（已截断并补齐到列宽），时长列右对齐。
fn track_row_columns(track: &Track, layout: &TrackRowLayout) -> Vec<String> {
    let mut columns = vec![column_text(track.display_title(), layout.title, false)];
    if let Some(width) = layout.artist {
        columns.push(column_text(
            track.artist.as_deref().unwrap_or("未知歌手"),
            width,
            false,
        ));
    }
    if let Some(width) = layout.album {
        columns.push(column_text(
            track.album.as_deref().unwrap_or("未知专辑"),
            width,
            false,
        ));
    }
    if layout.format {
        columns.push(column_text(
            track.format.as_deref().unwrap_or("?"),
            FORMAT_COLUMN_WIDTH,
            false,
        ));
    }
    let duration = track
        .duration
        .map(fmt_duration)
        .unwrap_or_else(|| "--:--".into());
    columns.push(column_text(&duration, layout.duration, true));
    columns
}

/// 把 "标题 — 歌手" 作为一个整体截到 budget 列：标题完整则剩余预算交给
/// " — 歌手" 段截断；标题本身超宽则截标题、舍弃歌手段。两段拼接后保证
/// 不超过 budget 列。
fn now_playing_text(title: &str, artist: Option<&str>, budget: usize) -> (String, String) {
    if UnicodeWidthStr::width(title) <= budget {
        let remaining = budget - UnicodeWidthStr::width(title);
        let artist = artist
            .map(|artist| format!(" — {artist}"))
            .unwrap_or_default();
        (title.to_owned(), truncate_display(&artist, remaining))
    } else {
        (truncate_display(title, budget), String::new())
    }
}

#[allow(dead_code)]
fn _path_reference(path: &Path) -> &Path {
    path
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

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
    fn ascii_progress_bar_has_fixed_width_and_expected_charset() {
        for width in 0..=40 {
            for ratio in [0.0, 0.25, 0.5, 0.99, 1.0] {
                let bar = ascii_progress_bar(ratio, width);
                assert_eq!(bar.chars().count(), width);
                assert!(bar.chars().all(|cell| matches!(cell, '=' | '>' | '-')));
            }
        }
    }

    #[test]
    fn ascii_progress_bar_renders_head_and_completion() {
        assert_eq!(ascii_progress_bar(0.0, 10), ">---------");
        assert_eq!(ascii_progress_bar(0.5, 10), "=====>----");
        // floor 不虚报：99.9% 仍未走完，播放头保留在最右列。
        assert_eq!(ascii_progress_bar(0.999, 10), "=========>");
        // 100% 全部填满，末尾不留 '>'。
        assert_eq!(ascii_progress_bar(1.0, 10), "==========");
        assert_eq!(ascii_progress_bar(0.5, 1), ">");
        assert_eq!(ascii_progress_bar(1.0, 1), "=");
        assert_eq!(ascii_progress_bar(0.5, 0), "");
    }

    #[test]
    fn truncate_display_limits_by_terminal_width() {
        assert_eq!(truncate_display("hello", 10), "hello");
        assert_eq!(truncate_display("hello world", 8), "hello w…");
        // CJK 按 2 列计，省略号占 1 列。
        assert_eq!(truncate_display("你好世界", 5), "你好…");
        assert_eq!(truncate_display("ab你好cd", 6), "ab你…");
        assert_eq!(truncate_display("abc", 1), "…");
        assert_eq!(truncate_display("abc", 0), "");
    }

    #[test]
    fn track_row_layout_tiers_follow_derived_min_widths() {
        let duration = 5;
        let full_min = LIST_ICON_WIDTH
            + MIN_TITLE_WIDTH
            + MIN_ARTIST_WIDTH
            + MIN_ALBUM_WIDTH
            + FORMAT_COLUMN_WIDTH
            + duration
            + LIST_GAP_WIDTH * 4;
        let layout = track_row_layout(full_min, duration);
        assert!(layout.artist.is_some() && layout.album.is_some() && layout.format);
        let layout = track_row_layout(full_min - 1, duration);
        assert!(layout.artist.is_some() && layout.album.is_some() && !layout.format);

        let no_album_min =
            LIST_ICON_WIDTH + MIN_TITLE_WIDTH + MIN_ARTIST_WIDTH + duration + LIST_GAP_WIDTH * 2;
        let layout = track_row_layout(no_album_min, duration);
        assert!(layout.artist.is_some() && layout.album.is_none() && !layout.format);
        let layout = track_row_layout(no_album_min - 1, duration);
        assert!(layout.artist.is_none() && layout.album.is_none() && !layout.format);
    }

    #[test]
    fn track_row_columns_respect_the_layout_width() {
        let track = Track {
            path: PathBuf::from("/music/长标题.flac"),
            relative_path: PathBuf::from("长标题.flac"),
            title: "这是一首名字特别特别长的歌曲标题".to_owned(),
            artist: Some("一个名字同样很长的歌手".to_owned()),
            album: Some("一张名字也非常非常长的专辑".to_owned()),
            duration: Some(Duration::from_secs(3_661)),
            format: Some("MPEG-4 AAC".to_owned()),
            file_size: 1,
            modified_ns: 1,
        };
        let duration_width = 7;
        for usable in [22usize, 30, 42, 50, 66, 90] {
            let layout = track_row_layout(usable, duration_width);
            let columns = track_row_columns(&track, &layout);
            let text_width = columns
                .iter()
                .map(|column| UnicodeWidthStr::width(column.as_str()))
                .sum::<usize>()
                + LIST_GAP_WIDTH * (columns.len() - 1);
            // 行内容（不含指示器）必须恰好填满 usable - LIST_ICON_WIDTH。
            assert_eq!(text_width + LIST_ICON_WIDTH, usable, "usable={usable}");
            // 超长 format 被截断，不能打破固定列宽。
            assert!(!columns.iter().any(|column| column.contains("MPEG-4")));
        }
    }

    #[test]
    fn now_playing_text_truncates_title_and_artist_as_a_whole() {
        // 整体不超宽：完整保留。
        let (title, artist) = now_playing_text("春日影", Some("MyGO!!!!!"), 30);
        assert_eq!(title, "春日影");
        assert_eq!(artist, " — MyGO!!!!!");

        // 标题放下、整体超宽：歌手段被截，合计恰好 12 列。
        let (title, artist) = now_playing_text("春日影", Some("MyGO!!!!!"), 12);
        assert_eq!(title, "春日影");
        assert_eq!(
            UnicodeWidthStr::width(title.as_str()) + UnicodeWidthStr::width(artist.as_str()),
            12
        );

        // 标题本身就超宽：截标题，歌手段舍弃。
        let (title, artist) = now_playing_text("这是一首名字特别特别长的歌", Some("X"), 9);
        assert_eq!(artist, "");
        assert!(UnicodeWidthStr::width(title.as_str()) <= 9);

        // 没有歌手时不产生多余内容。
        let (title, artist) = now_playing_text("歌", None, 30);
        assert_eq!(title, "歌");
        assert_eq!(artist, "");
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
