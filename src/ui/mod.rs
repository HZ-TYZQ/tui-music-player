//! Ratatui 界面：音乐库、播放状态、搜索和模态弹层。

mod library;
mod now_playing;
mod overlays;
mod text;
mod visualizer;

#[cfg(test)]
mod tests;

use ratatui::prelude::*;
use ratatui::widgets::{Block, Paragraph};

use crate::app::App;
use crate::theme::{DEFAULT_THEME, Theme};

use library::draw_library;
use now_playing::draw_now_playing;
use overlays::draw_overlay;
pub use text::fmt_duration;
use visualizer::{draw_visualizer, visualizer_height};

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
