//! 基于 GStreamer Play 的音频播放后端。
//!
//! GStreamer 自己负责解码和音频输出；这里把它的异步消息转换成应用事件，
//! 让终端主循环可以安全地处理播放结束、错误和状态变化。

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_play as gst_play;

pub const SPECTRUM_BANDS: usize = 512;
const SPECTRUM_INTERVAL_NS: u64 = 50_000_000;
pub const SPECTRUM_THRESHOLD_DB: f32 = -72.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayState {
    Playing,
    Paused,
    Stopped,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlayerEvent {
    EndOfStream,
    Error(String),
    StateChanged(PlayState),
    SpectrumFrame {
        magnitudes: Vec<f32>,
        sample_rate: u32,
    },
}

pub struct Player {
    play: gst_play::Play,
    /// 必须与 `play` 一起存活，否则信号连接会被释放。
    _adapter: gst_play::PlaySignalAdapter,
    spectrum: gst::Element,
    spectrum_bus: gst::Bus,
    spectrum_handler: Option<gst::glib::SignalHandlerId>,
    events: Receiver<PlayerEvent>,
    state: PlayState,
    current_path: Option<PathBuf>,
}

impl Player {
    pub fn new() -> Result<Self, String> {
        Self::new_with_audio_sink(None)
    }

    /// 创建使用 `fakesink` 的播放器，供无音频设备的自动化测试使用。
    #[doc(hidden)]
    pub fn new_for_tests() -> Result<Self, String> {
        gst::init().map_err(|error| format!("无法初始化 GStreamer: {error}"))?;
        let sink = gst::ElementFactory::make("fakesink")
            .property("sync", true)
            .build()
            .map_err(|error| format!("无法创建测试音频输出: {error}"))?;
        Self::new_with_audio_sink(Some(sink))
    }

    fn new_with_audio_sink(audio_sink: Option<gst::Element>) -> Result<Self, String> {
        gst::init().map_err(|error| format!("无法初始化 GStreamer: {error}"))?;

        let play = gst_play::Play::new(None::<gst_play::PlayVideoRenderer>);
        let pipeline = play.pipeline();
        if let Some(audio_sink) = audio_sink {
            pipeline.set_property("audio-sink", &audio_sink);
        }
        play.set_video_track_enabled(false);
        play.set_subtitle_track_enabled(false);

        let spectrum = gst::ElementFactory::make("spectrum")
            .property("bands", SPECTRUM_BANDS as u32)
            .property("interval", SPECTRUM_INTERVAL_NS)
            .property("threshold", SPECTRUM_THRESHOLD_DB as i32)
            .property("message-magnitude", true)
            .property("message-phase", false)
            .property("multi-channel", false)
            .property("post-messages", true)
            .build()
            .map_err(|error| {
                format!(
                    "无法创建 GStreamer spectrum 频谱分析器: {error}；请安装 gstreamer1-plugins-good"
                )
            })?;
        pipeline.set_property("audio-filter", &spectrum);

        let adapter = gst_play::PlaySignalAdapter::new_sync_emit(&play);
        let (sender, events) = mpsc::channel();

        let spectrum_bus = pipeline
            .bus()
            .ok_or_else(|| "GStreamer 播放管线没有消息总线".to_owned())?;
        spectrum_bus.enable_sync_message_emission();
        let spectrum_sender = sender.clone();
        let spectrum_for_messages = spectrum.clone();
        let spectrum_handler =
            spectrum_bus.connect_sync_message(Some("element"), move |_, message| {
                let Some(structure) = message.structure() else {
                    return;
                };
                if structure.name() != "spectrum" {
                    return;
                }
                let Ok(magnitudes) = structure.get::<gst::List>("magnitude") else {
                    return;
                };
                let frame = magnitudes
                    .iter()
                    .filter_map(|value| value.get::<f32>().ok())
                    .collect::<Vec<_>>();
                let sample_rate = spectrum_for_messages
                    .static_pad("sink")
                    .and_then(|pad| pad.current_caps())
                    .and_then(|caps| {
                        caps.structure(0)
                            .and_then(|structure| structure.get::<i32>("rate").ok())
                    })
                    .and_then(|rate| u32::try_from(rate).ok());
                if !frame.is_empty()
                    && let Some(sample_rate) = sample_rate
                {
                    let _ = spectrum_sender.send(PlayerEvent::SpectrumFrame {
                        magnitudes: frame,
                        sample_rate,
                    });
                }
            });

        let event_sender = sender.clone();
        adapter.connect_end_of_stream(move |_| {
            let _ = event_sender.send(PlayerEvent::EndOfStream);
        });

        let event_sender = sender.clone();
        adapter.connect_error(move |_, error, _details| {
            let _ = event_sender.send(PlayerEvent::Error(error.to_string()));
        });

        adapter.connect_state_changed(move |_, state| {
            let state = match state {
                gst_play::PlayState::Playing => PlayState::Playing,
                gst_play::PlayState::Paused => PlayState::Paused,
                gst_play::PlayState::Stopped | gst_play::PlayState::Buffering => PlayState::Stopped,
                _ => PlayState::Stopped,
            };
            let _ = sender.send(PlayerEvent::StateChanged(state));
        });

        Ok(Self {
            play,
            _adapter: adapter,
            spectrum,
            spectrum_bus,
            spectrum_handler: Some(spectrum_handler),
            events,
            state: PlayState::Stopped,
            current_path: None,
        })
    }

    pub fn state(&self) -> PlayState {
        self.state
    }

    pub fn current_path(&self) -> Option<&Path> {
        self.current_path.as_deref()
    }

    /// 加载并立即播放本地音频文件。
    pub fn play(&mut self, path: &Path) -> Result<(), String> {
        if !path.is_file() {
            return Err(format!("音频文件不存在: {}", path.display()));
        }
        let absolute = path
            .canonicalize()
            .map_err(|error| format!("无法解析音频路径 {}: {error}", path.display()))?;
        let uri = gst::glib::filename_to_uri(&absolute, None)
            .map_err(|error| format!("无法把音频路径转换为 URI: {error}"))?;

        self.play.stop();
        self.play.set_uri(Some(uri.as_str()));
        self.play.play();
        self.state = PlayState::Playing;
        self.current_path = Some(absolute);
        Ok(())
    }

    pub fn toggle_pause(&mut self) {
        match self.state {
            PlayState::Playing => {
                self.play.pause();
                self.state = PlayState::Paused;
            }
            PlayState::Paused => {
                self.play.play();
                self.state = PlayState::Playing;
            }
            PlayState::Stopped => {}
        }
    }

    pub fn stop(&mut self) {
        self.play.stop();
        self.state = PlayState::Stopped;
        self.current_path = None;
    }

    pub fn position(&self) -> Duration {
        self.play
            .position()
            .map(|time| Duration::from_nanos(time.nseconds()))
            .unwrap_or_default()
    }

    pub fn duration(&self) -> Option<Duration> {
        self.play
            .duration()
            .map(|time| Duration::from_nanos(time.nseconds()))
    }

    pub fn seek_relative(&self, offset_seconds: i64) {
        let current = self.position().as_nanos().min(u64::MAX as u128) as u64;
        let offset = offset_seconds.unsigned_abs().saturating_mul(1_000_000_000);
        let target = if offset_seconds.is_negative() {
            current.saturating_sub(offset)
        } else {
            current.saturating_add(offset)
        };
        let target = self
            .duration()
            .map(|duration| target.min(duration.as_nanos().min(u64::MAX as u128) as u64))
            .unwrap_or(target);
        self.play.seek(gst::ClockTime::from_nseconds(target));
    }

    pub fn set_volume(&self, percent: u8) {
        self.play.set_volume(f64::from(percent.min(100)) / 100.0);
    }

    pub fn volume(&self) -> u8 {
        (self.play.volume() * 100.0).round().clamp(0.0, 100.0) as u8
    }

    pub fn set_muted(&self, muted: bool) {
        self.play.set_mute(muted);
    }

    pub fn is_muted(&self) -> bool {
        self.play.is_muted()
    }

    pub fn set_spectrum_enabled(&self, enabled: bool) {
        self.spectrum.set_property("post-messages", enabled);
    }

    /// 取出目前已经到达的所有异步事件，并同步本地状态快照。
    pub fn drain_events(&mut self) -> Vec<PlayerEvent> {
        let mut drained = Vec::new();
        while let Ok(event) = self.events.try_recv() {
            match &event {
                PlayerEvent::EndOfStream | PlayerEvent::Error(_) => {
                    self.state = PlayState::Stopped;
                }
                PlayerEvent::StateChanged(state) => self.state = *state,
                PlayerEvent::SpectrumFrame { .. } => {}
            }
            drained.push(event);
        }
        drained
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.play.stop();
        if let Some(handler) = self.spectrum_handler.take() {
            self.spectrum_bus.disconnect(handler);
        }
        self.spectrum_bus.disable_sync_message_emission();
    }
}
