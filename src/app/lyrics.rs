//! 当前曲目的同步歌词：按需加载、开关。

use crate::lyrics::Lyrics;

use super::App;

impl App {
    /// 当前曲目的歌词；关闭歌词、没有歌词或尚未加载时为 None。
    pub fn lyrics(&self) -> Option<&Lyrics> {
        if !self.config.lyrics_enabled {
            return None;
        }
        let current = self.player.current_path()?;
        match &self.lyrics {
            Some((path, lyrics)) if path == current => lyrics.as_ref(),
            _ => None,
        }
    }

    /// 曲目变了就重新读歌词。放在 tick 里而不是各个切歌入口，
    /// 点播、自动下一首、会话恢复和系统媒体控制都不用各自记得加载。
    pub(super) fn sync_lyrics(&mut self) {
        if !self.config.lyrics_enabled {
            return;
        }
        let Some(current) = self.player.current_path() else {
            return;
        };
        if self
            .lyrics
            .as_ref()
            .is_some_and(|(path, _)| path == current)
        {
            return;
        }
        let current = current.to_path_buf();
        let lyrics = match Lyrics::load_for(&current) {
            Ok(lyrics) => lyrics,
            Err(error) => {
                // 不覆盖更要紧的提示（例如跳过失败曲目的汇总）。
                if self.message.is_none() {
                    self.message = Some(error);
                }
                None
            }
        };
        self.lyrics = Some((current, lyrics));
    }

    pub(super) fn toggle_lyrics(&mut self) {
        self.config.lyrics_enabled = !self.config.lyrics_enabled;
        if self.config.lyrics_enabled {
            self.sync_lyrics();
            self.message = Some(
                if self.lyrics().is_some() || self.player.current_path().is_none() {
                    "已开启歌词"
                } else {
                    "已开启歌词；当前曲目没有同步歌词"
                }
                .to_owned(),
            );
        } else {
            self.message = Some("已关闭歌词".to_owned());
        }
    }
}
