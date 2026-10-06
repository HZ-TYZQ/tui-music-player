//! 曲库光标、搜索编辑，以及重扫后按路径恢复选中项。

use super::App;

impl App {
    pub(super) fn select_next(&mut self) {
        self.cancel_pending_selection_restore();
        if !self.visible_indices().is_empty() {
            self.selected = (self.selected + 1).min(self.visible_indices().len() - 1);
        }
    }

    pub(super) fn select_previous(&mut self) {
        self.cancel_pending_selection_restore();
        self.selected = self.selected.saturating_sub(1);
    }

    pub(super) fn restore_pending_selection(&mut self) {
        if !self.search.query().is_empty() && self.search.is_running() {
            return;
        }
        let Some(path) = self.pending_selected_path.take() else {
            return;
        };
        self.selected = self
            .index_for_path(&path)
            .and_then(|index| {
                self.visible_indices()
                    .iter()
                    .position(|visible| *visible == index)
            })
            .unwrap_or(0);
    }

    /// 换了查询词，光标回到第一条结果。
    pub(super) fn edit_search(&mut self, query: String) {
        self.cancel_pending_selection_restore();
        self.search.set_query(query);
        self.selected = 0;
    }

    /// 关闭搜索框并清空查询词。
    pub(super) fn cancel_search(&mut self) {
        self.cancel_pending_selection_restore();
        self.search_active = false;
        self.search.set_query(String::new());
        self.selected = 0;
    }

    pub(super) fn cancel_pending_selection_restore(&mut self) {
        self.pending_selected_path = None;
    }

    pub(super) fn clamp_selections(&mut self) {
        self.selected = self
            .selected
            .min(self.visible_indices().len().saturating_sub(1));
        self.playlist_selected = self
            .playlist_selected
            .min(self.playlists.all().len().saturating_sub(1));
        self.queue_selected = self.queue_selected.min(self.queue.len().saturating_sub(1));
    }
}
