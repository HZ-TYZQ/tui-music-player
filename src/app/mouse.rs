//! 鼠标输入：命中测试、单击选中与双击激活。

use std::time::{Duration, Instant};

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

use crate::ui::ViewLayout;

use super::{App, Overlay};

/// 同一位置两次按下在此间隔内算双击。crossterm 不上报双击，只能自己判定。
const DOUBLE_CLICK_WINDOW: Duration = Duration::from_millis(400);
/// 滚轮一格移动的行数。
const SCROLL_STEP: usize = 3;

impl App {
    /// 开启鼠标捕获会接管终端自己的拖拽选区，因此保留一个开关；
    /// 真正的终端模式切换由主循环在下一次迭代时应用。
    pub(super) fn toggle_mouse(&mut self) {
        self.config.mouse_enabled = !self.config.mouse_enabled;
        self.message = Some(
            if self.config.mouse_enabled {
                "鼠标已开启"
            } else {
                "鼠标已关闭，可用终端自身的选区和复制"
            }
            .to_owned(),
        );
    }

    pub fn handle_mouse(&mut self, event: MouseEvent, view: &ViewLayout) {
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let double = self.register_click(event.column, event.row);
                self.click(event.column, event.row, double, view);
            }
            MouseEventKind::ScrollUp => self.scroll(-(SCROLL_STEP as isize), event, view),
            MouseEventKind::ScrollDown => self.scroll(SCROLL_STEP as isize, event, view),
            _ => {}
        }
    }

    /// 记录这次按下，并回答它是不是双击的第二下。
    fn register_click(&mut self, column: u16, row: u16) -> bool {
        let now = Instant::now();
        let double = self.last_click.is_some_and(|(at, last_column, last_row)| {
            (last_column, last_row) == (column, row)
                && now.duration_since(at) <= DOUBLE_CLICK_WINDOW
        });
        // 双击之后重新计时，三击不会被当成又一次双击。
        self.last_click = (!double).then_some((now, column, row));
        double
    }

    fn click(&mut self, column: u16, row: u16, double: bool, view: &ViewLayout) {
        if self.overlay != Overlay::None {
            if let Some(index) = view.overlay.index_at(row)
                && view.overlay.contains(column, row)
            {
                self.click_overlay_row(index, double);
            }
            return;
        }
        if let Some(progress) = view.progress
            && progress.contains(ratatui::layout::Position::new(column, row))
        {
            let ratio = f64::from(column - progress.x) / f64::from(progress.width);
            self.seek_to_ratio(ratio);
            return;
        }
        if let Some(index) = view.library.index_at(row)
            && view.library.contains(column, row)
        {
            self.selected = index;
            if double {
                self.play_selected();
            }
        }
    }

    fn click_overlay_row(&mut self, index: usize, double: bool) {
        match self.overlay {
            Overlay::Queue => {
                self.queue_selected = index;
                if double {
                    self.play_queue_selected();
                }
            }
            Overlay::Playlists => {
                self.playlist_selected = index;
                if double {
                    self.playlist_track_selected = 0;
                    self.overlay = Overlay::PlaylistTracks;
                }
            }
            Overlay::PlaylistTracks => {
                self.playlist_track_selected = index;
                if double {
                    self.play_playlist_from_selected();
                }
            }
            _ => {}
        }
    }

    /// 滚轮移动选中项而不是单独移动视口：ratatui 渲染时总会把选中项拉回可视
    /// 区域，单独改偏移会在下一帧被撤销。
    fn scroll(&mut self, delta: isize, event: MouseEvent, view: &ViewLayout) {
        let (column, row) = (event.column, event.row);
        if self.overlay != Overlay::None {
            if view.overlay.contains(column, row) {
                self.scroll_overlay(delta);
            }
            return;
        }
        if !view.library.contains(column, row) {
            return;
        }
        let steps = delta.unsigned_abs();
        for _ in 0..steps {
            if delta.is_negative() {
                self.select_previous();
            } else {
                self.select_next();
            }
        }
    }

    fn scroll_overlay(&mut self, delta: isize) {
        let (selected, len) = match self.overlay {
            Overlay::Queue => (self.queue_selected, self.queue.len()),
            Overlay::Playlists => (self.playlist_selected, self.playlists.all().len()),
            Overlay::PlaylistTracks => (
                self.playlist_track_selected,
                self.playlists
                    .all()
                    .get(self.playlist_selected)
                    .map(|playlist| playlist.tracks.len())
                    .unwrap_or(0),
            ),
            _ => return,
        };
        if len == 0 {
            return;
        }
        let target = selected
            .saturating_add_signed(delta)
            .min(len.saturating_sub(1));
        match self.overlay {
            Overlay::Queue => self.queue_selected = target,
            Overlay::Playlists => self.playlist_selected = target,
            Overlay::PlaylistTracks => self.playlist_track_selected = target,
            _ => {}
        }
    }
}
