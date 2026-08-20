//! 系统媒体命令、Seek 与对外快照。

use std::time::Duration;

use crate::media::{MediaCommand, MediaEvent, MediaSnapshot};
use crate::player::PlayState;
use crate::track::RepeatMode;

use super::App;

impl App {
    pub fn apply_media_command(&mut self, command: MediaCommand) {
        match command {
            MediaCommand::Play => self.play_or_resume(),
            MediaCommand::Toggle => self.toggle_or_start(),
            MediaCommand::Pause => self.player.pause(),
            MediaCommand::Next => self.play_next(false),
            MediaCommand::Previous => self.play_previous(),
            MediaCommand::SeekRelMicros(offset) => self.seek_rel_micros(offset),
            MediaCommand::SeekTo { position, track_id } => {
                self.seek_to_requested(position, track_id.as_deref());
            }
            MediaCommand::SetVolume(volume) => {
                if volume > 0 {
                    self.player.set_muted(false);
                    self.config.muted = false;
                }
                self.player.set_volume(volume);
                self.config.volume = volume;
            }
            MediaCommand::SetRepeat(repeat) => self.config.repeat = repeat,
            MediaCommand::SetShuffle(shuffle) => {
                if shuffle != self.config.shuffle {
                    self.toggle_shuffle();
                }
            }
            MediaCommand::Quit => self.should_quit = true,
        }
    }

    pub(super) fn seek_rel_micros(&mut self, offset: i64) {
        if self.player.state() == PlayState::Stopped {
            return;
        }
        let current = duration_as_micros(self.player.position());
        let requested = current.saturating_add(offset);
        if requested < 0 {
            if self.player.seek_to(Duration::ZERO) {
                self.push_seeked(Duration::ZERO);
            }
            return;
        }
        if let Some(duration) = self.effective_duration()
            && requested > duration_as_micros(duration)
        {
            self.play_next(false);
            return;
        }
        let target = Duration::from_micros(requested as u64);
        if self.player.seek_to(target) {
            self.push_seeked(self.player.position());
        }
    }

    fn seek_to_requested(&mut self, position: Duration, track_id: Option<&str>) {
        if self.player.state() == PlayState::Stopped {
            return;
        }
        if let Some(expected) = track_id
            && expected != self.current_track_id()
        {
            return;
        }
        if let Some(duration) = self.effective_duration()
            && position > duration
        {
            return;
        }
        if self.player.seek_to(position) {
            self.push_seeked(self.player.position());
        }
    }

    fn effective_duration(&self) -> Option<Duration> {
        self.player
            .duration()
            .or_else(|| self.current_track().and_then(|track| track.duration))
    }

    fn push_seeked(&mut self, position: Duration) {
        self.media_events.push(MediaEvent::Seeked { position });
    }

    pub fn drain_media_events(&mut self) -> Vec<MediaEvent> {
        std::mem::take(&mut self.media_events)
    }

    pub fn media_snapshot(&self) -> MediaSnapshot {
        let track = self.current_track();
        MediaSnapshot {
            status: self.player.state(),
            title: track
                .map(|track| track.display_title().to_owned())
                .unwrap_or_default(),
            artist: track.and_then(|track| track.artist.clone()),
            album: track.and_then(|track| track.album.clone()),
            path: track.map(|track| track.path.clone()),
            duration: self.effective_duration(),
            position: self.player.position(),
            volume: self.player.volume(),
            muted: self.player.is_muted(),
            repeat: self.config.repeat,
            shuffle: self.config.shuffle,
            can_go_previous: !self.history.is_empty(),
            can_go_next: self.can_go_next(),
            track_id: self.current_track_id(),
        }
    }

    fn current_track_id(&self) -> String {
        match self.playing_index {
            Some(index) => format!("/org/mpris/MediaPlayer2/Track/{index}"),
            None => "/org/mpris/MediaPlayer2/TrackList/NoTrack".to_owned(),
        }
    }

    fn can_go_next(&self) -> bool {
        if !self.queue.is_empty() {
            return true;
        }
        if self.tracks.is_empty() {
            return false;
        }
        if self.config.repeat != RepeatMode::None {
            return true;
        }
        if self.config.shuffle {
            if self.shuffle_order.is_empty() {
                return true;
            }
            return self.shuffle_cursor < self.shuffle_order.len();
        }
        match self.playing_index {
            Some(index) => index + 1 < self.tracks.len(),
            None => !self.tracks.is_empty(),
        }
    }
}

fn duration_as_micros(duration: Duration) -> i64 {
    i64::try_from(duration.as_micros()).unwrap_or(i64::MAX)
}
