//! 命名播放列表：新建、删除、编辑和从中播放。

use super::{App, BagUpdate, Overlay};

impl App {
    /// 选中的播放列表里有几首；没有选中的列表时为 0。
    pub(super) fn selected_playlist_len(&self) -> usize {
        self.playlists
            .all()
            .get(self.view.playlist_selected)
            .map(|playlist| playlist.tracks.len())
            .unwrap_or(0)
    }

    pub(super) fn create_playlist(&mut self) {
        match self.playlists.create(&self.view.name_input) {
            Ok(index) => {
                self.view.playlist_selected = index;
                self.view.overlay = Overlay::Playlists;
                self.view.message = Some("播放列表已创建".to_owned());
            }
            Err(error) => self.view.message = Some(error),
        }
    }

    pub(super) fn delete_selected_playlist(&mut self) {
        match self.playlists.delete(self.view.playlist_selected) {
            Ok(()) => {
                self.view.playlist_selected = self.view.playlist_selected.saturating_sub(1);
                self.view.message = Some("播放列表已删除，音乐文件未受影响".to_owned());
            }
            Err(error) => self.view.message = Some(error),
        }
        self.view.overlay = Overlay::Playlists;
    }

    pub(super) fn add_selected_to_playlist(&mut self) {
        let Some(path) = self.selected_track().map(|track| track.path.clone()) else {
            self.view.message = Some("没有选中的歌曲".to_owned());
            return;
        };
        match self.playlists.add_track(self.view.playlist_selected, &path) {
            Ok(()) => self.view.message = Some("已加入播放列表".to_owned()),
            Err(error) => self.view.message = Some(error),
        }
    }

    pub(super) fn play_playlist_from_selected(&mut self) {
        if self.output_blocks_playback() {
            return;
        }
        let Some(playlist) = self.playlists.all().get(self.view.playlist_selected) else {
            return;
        };
        let paths = playlist.tracks[self.view.playlist_track_selected.min(playlist.tracks.len())..]
            .to_vec();
        let mut paths = paths.into_iter();
        let Some(first) = paths.next() else {
            self.view.message = Some("播放列表是空的".to_owned());
            return;
        };
        self.playback.queue = paths.collect();
        match self.play_path(&first, true, BagUpdate::Reanchor) {
            Ok(()) => self.view.overlay = Overlay::None,
            Err(skip) => {
                self.advance(false, vec![skip]);
                if self.playback.playing_index.is_some() {
                    self.view.overlay = Overlay::None;
                }
            }
        }
    }

    pub(super) fn remove_playlist_track(&mut self) {
        match self.playlists.remove_track(
            self.view.playlist_selected,
            self.view.playlist_track_selected,
        ) {
            Ok(()) => {
                self.view.playlist_track_selected =
                    self.view.playlist_track_selected.saturating_sub(1);
                self.view.message = Some("已从播放列表移除，音乐文件未受影响".to_owned());
            }
            Err(error) => self.view.message = Some(error),
        }
    }
}
