//! 帮助、播放列表和确认弹层。

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::app::{App, Overlay};
use crate::theme::Theme;

pub(super) fn draw_overlay(frame: &mut Frame, app: &App, theme: &Theme) {
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
                "z              循环方式：顺序 / 列表 / 单曲",
                "s              开 / 关随机播放",
                "v              显示 / 隐藏音频频谱",
                "/              实时模糊搜索",
                "r              后台重新扫描",
                "a / A          加到队尾 / 设为下一首",
                "P              播放列表",
                "? / Esc        关闭帮助",
                "q              退出",
            ],
            62,
            20,
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
