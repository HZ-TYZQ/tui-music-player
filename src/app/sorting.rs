//! 曲库排序：排序字段、升降序和重排后的下标重建。

use super::App;

impl App {
    pub(super) fn cycle_sort(&mut self) {
        self.config.sort.key = self.config.sort.key.next();
        self.resort();
    }

    pub(super) fn toggle_sort_direction(&mut self) {
        self.config.sort.descending = !self.config.sort.descending;
        self.resort();
    }

    /// 顺序播放跟随列表顺序，因此重排也会改变“下一首”的走向；
    /// 正在播放的曲目本身不变，只是它在列表中的位置变了。
    fn resort(&mut self) {
        let tracks = std::mem::take(&mut self.tracks);
        self.replace_tracks(tracks);
        self.message = Some(format!("排序：{}", self.config.sort.label()));
    }
}
