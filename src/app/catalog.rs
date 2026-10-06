//! 曲库视图：全部曲目（按当前排序）、模糊搜索结果，以及按路径查找。
//!
//! 曲目和搜索索引只能一起换，界面看到的下标因此总是同一份曲库的下标。

use std::path::Path;
use std::time::Duration;

use crate::search::SearchIndex;
use crate::track::{SortOrder, Track};

pub struct Catalog {
    tracks: Vec<Track>,
    search: SearchIndex,
    longest_duration: Option<Duration>,
}

impl Catalog {
    pub(super) fn new() -> Self {
        Self {
            tracks: Vec::new(),
            search: SearchIndex::new(),
            longest_duration: None,
        }
    }

    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    pub fn get(&self, index: usize) -> Option<&Track> {
        self.tracks.get(index)
    }

    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    pub fn index_of(&self, path: &Path) -> Option<usize> {
        self.tracks.iter().position(|track| track.path == path)
    }

    /// 当前查询的结果，按显示顺序排列的曲库下标；没有查询词时是全部曲目。
    pub fn visible(&self) -> &[usize] {
        self.search.results()
    }

    pub fn query(&self) -> &str {
        self.search.query()
    }

    /// 最长的曲目时长，界面据此定时长列的宽度。
    pub fn longest_duration(&self) -> Option<Duration> {
        self.longest_duration
    }

    /// 换上新的曲目集合，按 `sort` 排好并重建搜索索引。
    pub(super) fn replace(&mut self, mut tracks: Vec<Track>, sort: SortOrder) {
        tracks.sort_by(|left, right| sort.compare(left, right));
        self.set_unsorted(tracks);
    }

    /// 按 `sort` 重排现有曲目并重建搜索索引。
    pub(super) fn resort(&mut self, sort: SortOrder) {
        let tracks = std::mem::take(&mut self.tracks);
        self.replace(tracks, sort);
    }

    pub(super) fn set_query(&mut self, query: String) {
        self.search.set_query(query);
    }

    /// 推进后台搜索；结果有变化时返回 true。
    pub(super) fn tick_search(&mut self) -> bool {
        self.search.tick()
    }

    pub(super) fn search_running(&self) -> bool {
        self.search.is_running()
    }

    /// 测试用：按给定顺序换上曲目，不排序。
    #[cfg(test)]
    pub(super) fn set_tracks(&mut self, tracks: Vec<Track>) {
        self.set_unsorted(tracks);
    }

    fn set_unsorted(&mut self, tracks: Vec<Track>) {
        self.tracks = tracks;
        self.longest_duration = self.tracks.iter().filter_map(|track| track.duration).max();
        self.search.replace_tracks(&self.tracks);
    }
}
