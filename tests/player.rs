//! GStreamer 播放后端的无声集成测试。

use std::fs;
use std::io::Write;
use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, Instant};

use music_player::player::{PlayState, Player, PlayerEvent};

fn write_test_wav(path: &Path, duration_secs: f32) {
    let sample_rate = 8_000u32;
    let sample_count = (sample_rate as f32 * duration_secs) as usize;
    let mut samples = Vec::with_capacity(sample_count * 2);
    for index in 0..sample_count {
        let time = index as f32 / sample_rate as f32;
        let value = (time * 440.0 * std::f32::consts::TAU).sin() * 0.3;
        samples.extend_from_slice(&((value * i16::MAX as f32) as i16).to_le_bytes());
    }
    let data_len = samples.len() as u32;
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    wav.extend_from_slice(&samples);
    let mut file = fs::File::create(path).unwrap();
    file.write_all(&wav).unwrap();
}

#[test]
fn play_pause_seek_volume_mute_and_stop() {
    let temp = tempfile::tempdir().unwrap();
    let wav = temp.path().join("basic.wav");
    write_test_wav(&wav, 3.0);
    let mut player = Player::new_for_tests().unwrap();
    assert_eq!(player.state(), PlayState::Stopped);

    player.set_volume(55);
    assert_eq!(player.volume(), 55);
    player.set_muted(true);
    assert!(player.is_muted());

    player.play(&wav).expect("GStreamer 应能加载测试 WAV");
    assert_eq!(player.state(), PlayState::Playing);
    sleep(Duration::from_millis(150));
    player.seek_relative(1);
    player.toggle_pause();
    assert_eq!(player.state(), PlayState::Paused);
    player.toggle_pause();
    assert_eq!(player.state(), PlayState::Playing);
    player.stop();
    assert_eq!(player.state(), PlayState::Stopped);
}

#[test]
fn reports_natural_end_of_stream() {
    let temp = tempfile::tempdir().unwrap();
    let wav = temp.path().join("finish.wav");
    write_test_wav(&wav, 0.2);
    let mut player = Player::new_for_tests().unwrap();
    player.play(&wav).unwrap();

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut finished = false;
    while Instant::now() < deadline {
        if player.drain_events().contains(&PlayerEvent::EndOfStream) {
            finished = true;
            break;
        }
        sleep(Duration::from_millis(20));
    }
    assert!(finished, "短音频应产生播放结束事件");
    assert_eq!(player.state(), PlayState::Stopped);
}

#[test]
fn reports_spectrum_frames_from_playing_audio() {
    let temp = tempfile::tempdir().unwrap();
    let wav = temp.path().join("spectrum.wav");
    write_test_wav(&wav, 1.0);
    let mut player = Player::new_for_tests().unwrap();
    player.play(&wav).unwrap();

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut frame = None;
    while Instant::now() < deadline {
        frame = player.drain_events().into_iter().find_map(|event| {
            if let PlayerEvent::SpectrumFrame {
                magnitudes,
                sample_rate,
            } = event
            {
                Some((magnitudes, sample_rate))
            } else {
                None
            }
        });
        if frame.is_some() {
            break;
        }
        sleep(Duration::from_millis(20));
    }

    let (frame, sample_rate) = frame.expect("播放音频时应产生频谱事件");
    assert_eq!(frame.len(), 512);
    assert_eq!(sample_rate, 8_000);
    assert!(frame.iter().any(|magnitude| *magnitude > -60.0));
}

#[test]
fn missing_file_is_rejected_before_playback() {
    let mut player = Player::new_for_tests().unwrap();
    let error = player
        .play(Path::new("/nonexistent/definitely_missing.mp3"))
        .unwrap_err();
    assert!(error.contains("不存在"));
    assert_eq!(player.state(), PlayState::Stopped);
}

#[test]
fn plays_audio_from_a_unicode_path_with_spaces() {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("测试 音乐");
    fs::create_dir(&directory).unwrap();
    let wav = directory.join("示例 音频.wav");
    write_test_wav(&wav, 0.2);

    let mut player = Player::new_for_tests().unwrap();
    player.play(&wav).expect("GStreamer 应能加载 Unicode 路径");
    assert_eq!(player.state(), PlayState::Playing);
    player.stop();
}
