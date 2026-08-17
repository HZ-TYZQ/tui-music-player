//! 非阻塞 PCM tap 与自有频谱分析。
//!
//! 音频线程只做轻量 downmix 和 latest-wins 写入；FFT 在独立 worker 中运行。

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use rodio::Source;
use rustfft::FftPlanner;
use rustfft::num_complex::Complex;

use super::{PlayerEvent, SPECTRUM_BANDS, SPECTRUM_THRESHOLD_DB};

pub(super) const FFT_SIZE: usize = 1024;
pub(super) const SPECTRUM_INTERVAL: Duration = Duration::from_millis(20);

pub(super) struct PcmBatch {
    pub generation: u64,
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

pub(super) struct PcmTap<S> {
    inner: S,
    generation: u64,
    pending: Vec<f32>,
    mix_acc: f32,
    mix_index: u16,
    slot: Arc<Mutex<Option<PcmBatch>>>,
    live: Arc<AtomicBool>,
}

impl<S: Source<Item = f32>> PcmTap<S> {
    pub(super) fn new(
        inner: S,
        generation: u64,
        slot: Arc<Mutex<Option<PcmBatch>>>,
        live: Arc<AtomicBool>,
    ) -> Self {
        Self {
            inner,
            generation,
            pending: Vec::with_capacity(FFT_SIZE),
            mix_acc: 0.0,
            mix_index: 0,
            slot,
            live,
        }
    }

    fn publish(&mut self) {
        let samples = std::mem::take(&mut self.pending);
        let batch = PcmBatch {
            generation: self.generation,
            samples,
            sample_rate: self.inner.sample_rate().get(),
        };
        if let Ok(mut slot) = self.slot.try_lock() {
            *slot = Some(batch);
        }
    }
}

impl<S: Source<Item = f32>> Iterator for PcmTap<S> {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        let sample = self.inner.next()?;
        if !self.live.load(Ordering::Relaxed) {
            self.pending.clear();
            self.mix_acc = 0.0;
            self.mix_index = 0;
            return Some(sample);
        }

        let channels = self.inner.channels().get();
        self.mix_acc += sample;
        self.mix_index += 1;
        if self.mix_index >= channels {
            self.pending.push(self.mix_acc / f32::from(channels.max(1)));
            self.mix_acc = 0.0;
            self.mix_index = 0;
            if self.pending.len() >= FFT_SIZE {
                self.publish();
            }
        }
        Some(sample)
    }
}

impl<S: Source<Item = f32>> Source for PcmTap<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }

    fn channels(&self) -> rodio::ChannelCount {
        self.inner.channels()
    }

    fn sample_rate(&self) -> rodio::SampleRate {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), rodio::source::SeekError> {
        self.pending.clear();
        self.mix_acc = 0.0;
        self.mix_index = 0;
        self.inner.try_seek(pos)
    }
}

pub(super) struct SpectrumWorker {
    shutdown: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl SpectrumWorker {
    pub(super) fn spawn(
        events: Sender<InternalSpectrumEvent>,
        generation: Arc<AtomicU64>,
        slot: Arc<Mutex<Option<PcmBatch>>>,
    ) -> Self {
        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_shutdown = Arc::clone(&shutdown);
        let handle = thread::Builder::new()
            .name("spectrum".to_owned())
            .spawn(move || {
                let mut planner = FftPlanner::<f32>::new();
                let fft = planner.plan_fft_forward(FFT_SIZE);
                let window = hann_window(FFT_SIZE);
                while !thread_shutdown.load(Ordering::Relaxed) {
                    let batch = slot.lock().ok().and_then(|mut guard| guard.take());
                    if let Some(batch) = batch {
                        if batch.generation == generation.load(Ordering::Relaxed) {
                            let magnitudes = analyze_frame(&fft, &window, &batch.samples);
                            let _ = events.send(InternalSpectrumEvent {
                                generation: batch.generation,
                                magnitudes,
                                sample_rate: batch.sample_rate,
                            });
                        }
                        thread::sleep(SPECTRUM_INTERVAL);
                    } else {
                        thread::sleep(Duration::from_millis(2));
                    }
                }
            })
            .expect("无法启动频谱分析线程");
        Self {
            shutdown,
            handle: Some(handle),
        }
    }
}

impl Drop for SpectrumWorker {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

pub(super) struct InternalSpectrumEvent {
    pub generation: u64,
    pub magnitudes: Vec<f32>,
    pub sample_rate: u32,
}

impl InternalSpectrumEvent {
    pub(super) fn into_player_event(self) -> PlayerEvent {
        PlayerEvent::SpectrumFrame {
            magnitudes: self.magnitudes,
            sample_rate: self.sample_rate,
        }
    }
}

fn hann_window(size: usize) -> Vec<f32> {
    (0..size)
        .map(|index| {
            0.5 - 0.5
                * (std::f32::consts::TAU * index as f32 / (size.saturating_sub(1) as f32).max(1.0))
                    .cos()
        })
        .collect()
}

fn analyze_frame(
    fft: &std::sync::Arc<dyn rustfft::Fft<f32>>,
    window: &[f32],
    samples: &[f32],
) -> Vec<f32> {
    let mut buffer = vec![Complex::new(0.0, 0.0); FFT_SIZE];
    for (index, slot) in buffer.iter_mut().enumerate() {
        let sample = samples.get(index).copied().unwrap_or(0.0);
        let gain = window.get(index).copied().unwrap_or(1.0);
        *slot = Complex::new(sample * gain, 0.0);
    }
    fft.process(&mut buffer);

    let norm = FFT_SIZE as f32;
    (0..SPECTRUM_BANDS)
        .map(|bin| {
            let magnitude = if bin == 0 {
                buffer[0].norm() / norm
            } else {
                2.0 * buffer[bin].norm() / norm
            };
            let db = 20.0 * magnitude.max(1e-12).log10();
            db.max(SPECTRUM_THRESHOLD_DB)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyzer_reports_energy_for_440hz_tone() {
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(FFT_SIZE);
        let window = hann_window(FFT_SIZE);
        let sample_rate = 8_000.0;
        let samples: Vec<f32> = (0..FFT_SIZE)
            .map(|index| (index as f32 / sample_rate * 440.0 * std::f32::consts::TAU).sin() * 0.3)
            .collect();
        let magnitudes = analyze_frame(&fft, &window, &samples);
        assert_eq!(magnitudes.len(), SPECTRUM_BANDS);
        assert!(magnitudes.iter().any(|value| *value > -60.0));
    }
}
