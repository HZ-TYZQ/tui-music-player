//! Ratatui 界面：音乐库、播放状态、搜索和模态弹层。

mod library;
mod now_playing;
mod overlays;
mod text;
mod visualizer;

#[cfg(test)]
mod tests;

use ratatui::prelude::*;
use ratatui::widgets::{Block, ListState, Paragraph};

use crate::app::App;
use crate::theme::{DEFAULT_THEME, Theme};

use library::draw_library;
use now_playing::draw_now_playing;
use overlays::draw_overlay;
pub use text::fmt_duration;
use visualizer::{draw_visualizer, visualizer_height};

/// 上一帧的列表布局与滚动位置。
///
/// 哪一屏幕行对应哪一列表项，只有渲染之后才确定：滚动偏移由 ratatui 在
/// 渲染时按选中项算出并写回 `ListState`。因此鼠标命中测试只能依赖上一帧
/// 留下的结果，不能重新推导。它是纯视图状态，不属于 `App`。
#[derive(Debug, Default)]
pub struct ViewLayout {
    pub(crate) library: ListView,
    /// 同一时刻只会有一个列表型弹层可见，所以共用一份。
    pub(crate) overlay: ListView,
    pub(crate) progress: Option<Rect>,
}

#[derive(Debug, Default)]
pub struct ListView {
    state: ListState,
    area: Option<Rect>,
    len: usize,
}

impl ListView {
    pub(crate) fn state_for(&mut self, selected: Option<usize>) -> &mut ListState {
        self.state.select(selected);
        &mut self.state
    }

    pub(crate) fn record(&mut self, area: Rect, len: usize) {
        self.area = Some(area);
        self.len = len;
    }

    pub(crate) fn clear(&mut self) {
        self.area = None;
        self.len = 0;
    }

    /// 屏幕行 → 列表项下标；落在列表之外或空白行返回 None。
    pub(crate) fn index_at(&self, row: u16) -> Option<usize> {
        let area = self.area?;
        if row < area.y || row >= area.bottom() {
            return None;
        }
        let index = self.state.offset() + usize::from(row - area.y);
        (index < self.len).then_some(index)
    }

    pub(crate) fn contains(&self, column: u16, row: u16) -> bool {
        self.area
            .is_some_and(|area| area.contains(Position::new(column, row)))
    }
}

pub fn draw(frame: &mut Frame, app: &App, view: &mut ViewLayout) {
    draw_with_theme(frame, app, view, &DEFAULT_THEME);
}

fn draw_with_theme(frame: &mut Frame, app: &App, view: &mut ViewLayout, theme: &Theme) {
    let area = frame.area();
    if area.width < 42 || area.height < 12 {
        // 这一帧什么列表都没画，命中测试必须失效，否则会用上一帧的坐标误判。
        view.library.clear();
        view.overlay.clear();
        view.progress = None;
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
        draw_library(frame, app, chunks[0], theme, &mut view.library);
        view.progress = draw_now_playing(frame, app, chunks[1], theme);
        draw_footer(frame, app, chunks[2], theme);
    } else {
        let chunks = Layout::vertical([
            Constraint::Min(6),
            Constraint::Length(visualizer_height),
            Constraint::Length(4),
            Constraint::Length(1),
        ])
        .split(area);
        draw_library(frame, app, chunks[0], theme, &mut view.library);
        draw_visualizer(frame, app, chunks[1], theme);
        view.progress = draw_now_playing(frame, app, chunks[2], theme);
        draw_footer(frame, app, chunks[3], theme);
    }
    draw_overlay(frame, app, theme, &mut view.overlay);
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
            " ↑↓/jk 选择 · Enter 播放 · Space 暂停 · / 搜索 · o 排序 · v 频谱 · Q 队列 · P 列表 · ? 帮助 · q 退出",
            Style::new().fg(theme.muted),
        ))
    };
    frame.render_widget(Paragraph::new(line), area);
}
