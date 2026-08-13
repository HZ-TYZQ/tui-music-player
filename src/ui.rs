//! Ratatui 界面：音乐库、播放状态、搜索和模态弹层。

use std::path::Path;
use std::time::Duration;

use ratatui::prelude::*;
use ratatui::widgets::{
    Block, BorderType, Clear, Gauge, List, ListItem, ListState, Paragraph, Wrap,
};

use crate::app::{App, Overlay};
use crate::player::PlayState;

const HIGHLIGHT_BG: Color = Color::Rgb(52, 56, 70);

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    if area.width < 42 || area.height < 12 {
        frame.render_widget(
            Paragraph::new("终端窗口太小\n请调整到至少 42×12")
                .alignment(Alignment::Center)
                .block(Block::bordered().title(" Music Player ")),
            area,
        );
        return;
    }

    let chunks = Layout::vertical([
        Constraint::Min(6),
        Constraint::Length(4),
        Constraint::Length(1),
    ])
    .split(area);
    draw_library(frame, app, chunks[0]);
    draw_now_playing(frame, app, chunks[1]);
    draw_footer(frame, app, chunks[2]);
    draw_overlay(frame, app);
}

fn draw_library(frame: &mut Frame, app: &App, area: Rect) {
    let scan = if app.scanning {
        format!(" · 扫描中 {}/{} ", app.scan_progress.0, app.scan_progress.1)
    } else {
        format!(" · {} 首 ", app.visible_indices().len())
    };
    let title = Line::from(vec![
        " ♪ Music Player ".cyan().bold(),
        format!("· {}", app.library_dir.display()).dark_gray(),
        scan.dark_gray(),
    ]);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().dark_gray())
        .title(title);

    if app.tracks.is_empty() {
        let text = if app.scanning {
            "  正在后台扫描音乐库……\n  界面仍可响应，扫描完成后歌曲会自动出现"
        } else {
            "  音乐库中没有支持的音频文件\n  按 r 重新扫描，或用 --set-library PATH 更换主库"
        };
        frame.render_widget(Paragraph::new(text).dark_gray().block(block), area);
        return;
    }

    let items = app.visible_indices().iter().filter_map(|index| {
        let track = app.tracks.get(*index)?;
        let current = app.playing_index == Some(*index);
        let (icon, style) = if current {
            match app.player.state() {
                PlayState::Playing => ("♪ ", Style::new().green().bold()),
                PlayState::Paused => ("⏸ ", Style::new().yellow()),
                PlayState::Stopped => ("■ ", Style::new().dark_gray()),
            }
        } else {
            ("  ", Style::new())
        };
        let artist = track.artist.as_deref().unwrap_or("未知歌手");
        let album = track.album.as_deref().unwrap_or("未知专辑");
        let format = track.format.as_deref().unwrap_or("?");
        let duration = track
            .duration
            .map(fmt_duration)
            .unwrap_or_else(|| "--:--".into());
        Some(ListItem::new(Line::from(vec![
            Span::styled(icon, style),
            Span::styled(track.display_title(), style),
            "  ".into(),
            Span::styled(artist, Style::new().dark_gray()),
            " · ".dark_gray(),
            Span::styled(album, Style::new().dark_gray()),
            " · ".dark_gray(),
            Span::styled(format, Style::new().cyan()),
            " · ".dark_gray(),
            Span::styled(duration, Style::new().dark_gray()),
        ])))
    });

    let list = List::new(items)
        .block(block)
        .highlight_style(Style::new().bg(HIGHLIGHT_BG).bold())
        .highlight_symbol("▸ ".cyan());
    let selected = (!app.visible_indices().is_empty()).then_some(app.selected);
    let mut state = ListState::default().with_selected(selected);
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_now_playing(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().magenta())
        .title(" ♫ 正在播放 ".magenta().bold());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(inner);
    let top = Layout::horizontal([Constraint::Min(10), Constraint::Length(30)]).split(rows[0]);
    let now = match app.current_track() {
        Some(track) => {
            let (icon, style) = match app.player.state() {
                PlayState::Playing => ("▶ ", Style::new().green().bold()),
                PlayState::Paused => ("⏸ ", Style::new().yellow().bold()),
                PlayState::Stopped => ("■ ", Style::new().dark_gray()),
            };
            Line::from(vec![
                Span::styled(icon, style),
                Span::styled(track.display_title(), Style::new().white().bold()),
                Span::styled(
                    track
                        .artist
                        .as_ref()
                        .map(|artist| format!(" — {artist}"))
                        .unwrap_or_default(),
                    Style::new().dark_gray(),
                ),
            ])
        }
        None => Line::from("■ 未在播放".dark_gray()),
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
        .dark_gray(),
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
            .gauge_style(Style::new().cyan())
            .ratio(ratio)
            .label(label),
        rows[1],
    );
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let line = if app.search_active {
        Line::from(vec![
            " / ".cyan().bold(),
            app.search.query().white(),
            "█".cyan(),
            format!(
                "  {} 个结果 · Enter 播放 · Esc 清除",
                app.visible_indices().len()
            )
            .dark_gray(),
        ])
    } else if let Some(message) = &app.message {
        Line::from(vec![" • ".cyan(), message.clone().into()])
    } else {
        Line::from(vec![
            " ↑↓/jk 选择 · Enter 播放 · Space 暂停 · / 搜索 · a/A 队列 · P 列表 · ? 帮助 · q 退出"
                .dark_gray(),
        ])
    };
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_overlay(frame: &mut Frame, app: &App) {
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
                "/              实时模糊搜索",
                "r              后台重新扫描",
                "a / A          加到队尾 / 设为下一首",
                "P              播放列表",
                "? / Esc        关闭帮助",
                "q              退出",
            ],
            62,
            18,
        ),
        Overlay::Playlists => draw_playlists(frame, app),
        Overlay::PlaylistTracks => draw_playlist_tracks(frame, app),
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
            );
        }
    }
}

fn draw_playlists(frame: &mut Frame, app: &App) {
    let area = centered(frame.area(), 70, 70);
    frame.render_widget(Clear, area);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .title(" 播放列表 ")
        .border_style(Style::new().cyan());
    let items = if app.playlists.all().is_empty() {
        vec![ListItem::new("暂无播放列表，按 c 创建".dark_gray())]
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
        .highlight_symbol("▸ ")
        .highlight_style(Style::new().bg(HIGHLIGHT_BG).bold());
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
        Paragraph::new("c 新建 · a 加入选中歌曲 · Enter 查看 · x 删除 · Esc 关闭").dark_gray(),
        help,
    );
}

fn draw_playlist_tracks(frame: &mut Frame, app: &App) {
    let area = centered(frame.area(), 78, 76);
    frame.render_widget(Clear, area);
    let Some(playlist) = app.playlists.all().get(app.playlist_selected) else {
        return;
    };
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .title(format!(" {} ", playlist.name))
        .border_style(Style::new().magenta());
    let items = playlist.tracks.iter().map(|path| {
        let missing = !path.is_file();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("?");
        if missing {
            ListItem::new(Line::from(vec![
                "⚠ ".red(),
                name.red(),
                "  文件不可用".dark_gray(),
            ]))
        } else {
            ListItem::new(name)
        }
    });
    let list = List::new(items)
        .block(block)
        .highlight_symbol("▸ ")
        .highlight_style(Style::new().bg(HIGHLIGHT_BG).bold());
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
        Paragraph::new("Enter 从此处播放 · d 从列表移除 · Esc 返回").dark_gray(),
        help,
    );
}

fn draw_text_popup(frame: &mut Frame, title: &str, lines: Vec<&str>, width: u16, height: u16) {
    let area = centered(frame.area(), width, height);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines.join("\n"))
            .wrap(Wrap { trim: false })
            .block(
                Block::bordered()
                    .border_type(BorderType::Rounded)
                    .border_style(Style::new().cyan())
                    .title(title),
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
}
