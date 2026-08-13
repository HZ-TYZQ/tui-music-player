//! 界面绘制：曲目列表 + 正在播放栏 + 帮助栏。

use std::time::Duration;

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, List, ListItem, ListState, Paragraph};

use crate::app::App;
use crate::player::PlayState;

/// 选中项背景色（柔和的深灰蓝）
const HIGHLIGHT_BG: Color = Color::Rgb(52, 56, 70);

pub fn draw(frame: &mut Frame, app: &App) {
    let chunks = Layout::vertical([
        Constraint::Min(5),    // 曲目列表
        Constraint::Length(3), // 正在播放
        Constraint::Length(1), // 帮助 / 消息
    ])
    .split(frame.area());

    draw_list(frame, app, chunks[0]);
    draw_now_playing(frame, app, chunks[1]);
    draw_help(frame, app, chunks[2]);
}

fn draw_list(frame: &mut Frame, app: &App, area: Rect) {
    let title = Line::from(vec![
        " ♪ Music Player ".cyan().bold(),
        format!("· {} ", app.dir.display()).dark_gray(),
    ]);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().dark_gray())
        .title(title);

    if app.tracks.is_empty() {
        let empty = Paragraph::new("  当前目录没有音频文件\n  用法: music-player <音乐目录>")
            .dark_gray()
            .block(block);
        frame.render_widget(empty, area);
        return;
    }

    let items: Vec<ListItem> = app
        .tracks
        .iter()
        .enumerate()
        .map(|(i, _)| {
            let name = app.track_name(i);
            let is_current = app.playing_index == Some(i);
            let (icon, style) = if is_current {
                match app.player.state() {
                    PlayState::Playing => ("♪ ", Style::new().green().bold()),
                    PlayState::Paused => ("⏸ ", Style::new().yellow()),
                    PlayState::Stopped => ("  ", Style::new()),
                }
            } else {
                ("  ", Style::new())
            };
            let mut spans = vec![Span::styled(icon, style), Span::styled(name, style)];
            if is_current {
                spans.push(Span::styled("  ●", Style::new().green()));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(Style::new().bg(HIGHLIGHT_BG).bold())
        .highlight_symbol("▸ ".cyan());

    let mut state = ListState::default().with_selected(Some(app.selected));
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_now_playing(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().magenta())
        .title(" ♫ ".magenta().bold());

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let cols = Layout::horizontal([Constraint::Min(10), Constraint::Length(9)]).split(inner);

    let line = match app.playing_index {
        Some(idx) => {
            let (icon, style) = match app.player.state() {
                PlayState::Playing => ("▶ ", Style::new().green().bold()),
                PlayState::Paused => ("⏸ ", Style::new().yellow().bold()),
                PlayState::Stopped => ("■ ", Style::new().dark_gray()),
            };
            Line::from(vec![
                Span::styled(icon, style),
                Span::styled(app.track_name(idx), Style::new().white().bold()),
            ])
        }
        None => Line::from("■ 未在播放".dark_gray()),
    };
    frame.render_widget(Paragraph::new(line), cols[0]);

    let elapsed = if app.playing_index.is_some() {
        fmt_duration(app.player.elapsed())
    } else {
        String::new()
    };
    frame.render_widget(
        Paragraph::new(elapsed).cyan().alignment(Alignment::Right),
        cols[1],
    );
}

fn draw_help(frame: &mut Frame, app: &App, area: Rect) {
    let line = if let Some(msg) = &app.message {
        Line::from(format!(" ✗ {msg}")).red()
    } else {
        Line::from(vec![
            " ↑↓/jk ".dark_gray(),
            "选择".gray(),
            " · enter ".dark_gray(),
            "播放".gray(),
            " · space ".dark_gray(),
            "暂停".gray(),
            " · s ".dark_gray(),
            "停止".gray(),
            " · n/p ".dark_gray(),
            "下/上曲".gray(),
            " · q ".dark_gray(),
            "退出".gray(),
        ])
    };
    frame.render_widget(Paragraph::new(line), area);
}

pub fn fmt_duration(d: Duration) -> String {
    let s = d.as_secs();
    let (h, m, sec) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{sec:02}")
    } else {
        format!("{m}:{sec:02}")
    }
}
