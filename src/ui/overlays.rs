//! 帮助、播放列表、播放队列和确认弹层。

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Clear, List, ListItem, Paragraph, Wrap};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Overlay};
use crate::theme::Theme;
use crate::track::Track;

use super::ListView;
use super::text::{fmt_duration, now_playing_text, truncate_display};

pub(super) fn draw_overlay(frame: &mut Frame, app: &App, theme: &Theme, view: &mut ListView) {
    // 非列表型弹层没有可点击的行；先失效，命中测试才不会用上一帧的坐标。
    if !matches!(
        app.overlay,
        Overlay::Playlists | Overlay::PlaylistTracks | Overlay::Queue
    ) {
        view.clear();
    }
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
                "o / O          排序字段 / 升降序",
                "M              开 / 关鼠标",
                "/              实时模糊搜索",
                "r              后台重新扫描",
                "a / A          加到队尾 / 设为下一首",
                "Q              播放队列",
                "P              播放列表",
                "? / Esc        关闭帮助",
                "q              退出",
            ],
            62,
            22,
            theme,
        ),
        Overlay::Playlists => draw_playlists(frame, app, theme, view),
        Overlay::Queue => draw_queue(frame, app, theme, view),
        Overlay::PlaylistTracks => draw_playlist_tracks(frame, app, theme, view),
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

fn draw_playlists(frame: &mut Frame, app: &App, theme: &Theme, view: &mut ListView) {
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
    // 与队列面板一致：列表只用提示行以上的空间，点击行与显示行才能一一对应。
    let (inner, viewport) = list_viewport(area, &block);
    frame.render_widget(block, area);
    let list = List::new(items)
        .highlight_symbol(Span::styled("▸ ", Style::new().fg(theme.primary)))
        .highlight_style(Style::new().bg(theme.selection_bg).bold());
    let selected = (!app.playlists.all().is_empty()).then_some(app.playlist_selected);
    frame.render_stateful_widget(list, viewport, view.state_for(selected));
    view.record(viewport, app.playlists.all().len());
    let help = help_row(inner);
    frame.render_widget(
        Paragraph::new("c 新建 · a 加入选中歌曲 · Enter 查看 · x 删除 · Esc 关闭")
            .style(Style::new().fg(theme.muted)),
        help,
    );
}

fn draw_playlist_tracks(frame: &mut Frame, app: &App, theme: &Theme, view: &mut ListView) {
    let area = centered(frame.area(), 78, 76);
    frame.render_widget(Clear, area);
    let Some(playlist) = app.playlists.all().get(app.playlist_selected) else {
        view.clear();
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
    let (inner, viewport) = list_viewport(area, &block);
    frame.render_widget(block, area);
    let list = List::new(items)
        .highlight_symbol(Span::styled("▸ ", Style::new().fg(theme.primary)))
        .highlight_style(Style::new().bg(theme.selection_bg).bold());
    let selected = (!playlist.tracks.is_empty()).then_some(app.playlist_track_selected);
    frame.render_stateful_widget(list, viewport, view.state_for(selected));
    view.record(viewport, playlist.tracks.len());
    let help = help_row(inner);
    frame.render_widget(
        Paragraph::new("Enter 从此处播放 · d 从列表移除 · Esc 返回")
            .style(Style::new().fg(theme.muted)),
        help,
    );
}

const MISSING_LABEL: &str = " 不在曲库中";

/// 把队列路径一次性解析成曲库下标。整体是 O(曲库 + 队列)，
/// 避免逐条线性查找在大曲库加长队列时退化成平方级扫描。
pub(super) fn resolve_queue_rows(
    tracks: &[Track],
    queue: &VecDeque<PathBuf>,
) -> Vec<Option<usize>> {
    let wanted: HashSet<&Path> = queue.iter().map(PathBuf::as_path).collect();
    let mut found: HashMap<&Path, usize> = HashMap::with_capacity(wanted.len());
    for (index, track) in tracks.iter().enumerate() {
        if wanted.contains(track.path.as_path()) {
            found.entry(track.path.as_path()).or_insert(index);
        }
    }
    queue
        .iter()
        .map(|path| found.get(path.as_path()).copied())
        .collect()
}

fn draw_queue(frame: &mut Frame, app: &App, theme: &Theme, view: &mut ListView) {
    let area = centered(frame.area(), 74, 70);
    frame.render_widget(Clear, area);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .title(Span::styled(
            format!(" 播放队列 · {} 首 ", app.queue.len()),
            Style::new().fg(theme.primary).bold(),
        ))
        .border_style(Style::new().fg(theme.border));

    if app.queue.is_empty() {
        view.clear();
        frame.render_widget(
            Paragraph::new("  队列是空的\n  在曲库中按 a 加到队尾，按 A 设为下一首")
                .style(Style::new().fg(theme.muted))
                .block(block),
            area,
        );
        return;
    }

    let rows = resolve_queue_rows(&app.tracks, &app.queue);
    let order_width = format!("{}. ", app.queue.len()).len();
    let duration_width = rows
        .iter()
        .filter_map(|row| app.tracks.get((*row)?)?.duration)
        .map(|duration| fmt_duration(duration).len())
        .max()
        .unwrap_or(5)
        .max(5);
    // 去掉左右边框和 highlight_symbol 后，中间留一列间隔给时长。
    let text_width = usize::from(area.width)
        .saturating_sub(2 + 2 + order_width + duration_width + 1)
        .max(1);

    let items = app
        .queue
        .iter()
        .zip(&rows)
        .enumerate()
        .map(|(order, (path, row))| {
            let order = format!("{:>width$}. ", order + 1, width = order_width - 2);
            let Some(track) = row.and_then(|index| app.tracks.get(index)) else {
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("?");
                // 播放时这些条目会被跳过，因此名字要给提示语让出显示宽度。
                let name_width = text_width
                    .saturating_sub(UnicodeWidthStr::width(MISSING_LABEL))
                    .max(1);
                return ListItem::new(Line::from(vec![
                    Span::styled(order, Style::new().fg(theme.muted)),
                    Span::styled(
                        truncate_display(name, name_width),
                        Style::new().fg(theme.danger),
                    ),
                    Span::styled(MISSING_LABEL, Style::new().fg(theme.muted)),
                ]));
            };
            let (title, artist) =
                now_playing_text(track.display_title(), track.artist.as_deref(), text_width);
            let duration = track
                .duration
                .map(fmt_duration)
                .unwrap_or_else(|| "--:--".to_owned());
            let text_used =
                UnicodeWidthStr::width(title.as_str()) + UnicodeWidthStr::width(artist.as_str());
            let padding = " ".repeat(
                text_width.saturating_sub(text_used)
                    + duration_width.saturating_sub(duration.len())
                    + 1,
            );
            ListItem::new(Line::from(vec![
                Span::styled(order, Style::new().fg(theme.muted)),
                Span::styled(title, Style::new().fg(theme.primary)),
                Span::styled(artist, Style::new().fg(theme.muted)),
                Span::styled(padding, Style::new().fg(theme.muted)),
                Span::styled(duration, Style::new().fg(theme.muted)),
            ]))
        });

    // 列表只用底部提示行以上的空间，长队列不会有条目被提示行盖住。
    let (inner, viewport) = list_viewport(area, &block);
    frame.render_widget(block, area);
    let list = List::new(items)
        .highlight_symbol(Span::styled("▸ ", Style::new().fg(theme.primary)))
        .highlight_style(Style::new().bg(theme.selection_bg).bold());
    frame.render_stateful_widget(list, viewport, view.state_for(Some(app.queue_selected)));
    view.record(viewport, app.queue.len());
    let help = help_row(inner);
    frame.render_widget(
        Paragraph::new("Enter 跳到此处播放 · d 移除 · J/K 上下移动 · c 清空 · Esc 关闭")
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

/// 弹层内容区，以及扣掉底部提示行之后真正留给列表的视口。
fn list_viewport(area: Rect, block: &Block<'_>) -> (Rect, Rect) {
    let inner = block.inner(area);
    let viewport = Rect {
        height: inner.height.saturating_sub(1),
        ..inner
    };
    (inner, viewport)
}

fn help_row(inner: Rect) -> Rect {
    Rect::new(
        inner.x + 1,
        inner.bottom().saturating_sub(1),
        inner.width.saturating_sub(1),
        1,
    )
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
