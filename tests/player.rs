//! Player（ffplay 子进程后端）的集成测试。
//!
//! 用 Rust 手写一个 1 秒的正弦波 WAV 文件，
//! 验证播放 / 暂停 / 恢复 / 停止 / 自然结束检测。

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::thread::sleep;
use std::time::Duration;

use music_player::player::{PlayState, Player};

/// 生成一个 duration_secs 秒的 440Hz 正弦波 WAV
fn write_test_wav(path: &PathBuf, duration_secs: f32) {
    let sample_rate = 8000u32;
    let n = (sample_rate as f32 * duration_secs) as usize;
    let mut samples = Vec::with_capacity(n * 2);
    for i in 0..n {
        let t = i as f32 / sample_rate as f32;
        let v = (t * 440.0 * std::f32::consts::TAU).sin() * 0.3;
        samples.extend_from_slice(&((v * i16::MAX as f32) as i16).to_le_bytes());
    }
    let data_len = samples.len() as u32;
    let mut wav: Vec<u8> = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes()); // PCM 块大小
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM 格式
    wav.extend_from_slice(&1u16.to_le_bytes()); // 单声道
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // 字节率
    wav.extend_from_slice(&2u16.to_le_bytes()); // 块对齐
    wav.extend_from_slice(&16u16.to_le_bytes()); // 位深
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    wav.extend_from_slice(&samples);
    let mut f = fs::File::create(path).unwrap();
    f.write_all(&wav).unwrap();
}

fn temp_wav(name: &str, secs: f32) -> PathBuf {
    let path = std::env::temp_dir().join(format!("music_player_test_{name}.wav"));
    write_test_wav(&path, secs);
    path
}

#[test]
fn play_pause_resume_stop() {
    let wav = temp_wav("basic", 3.0);
    let mut p = Player::new();
    assert_eq!(p.state(), PlayState::Stopped);

    p.play(&wav).expect("ffplay 启动失败");
    assert_eq!(p.state(), PlayState::Playing);
    sleep(Duration::from_millis(500));
    let e1 = p.elapsed();
    assert!(e1 >= Duration::from_millis(300), "elapsed 应增长: {e1:?}");

    // 暂停后 elapsed 不再明显增长
    p.toggle_pause();
    assert_eq!(p.state(), PlayState::Paused);
    let e2 = p.elapsed();
    sleep(Duration::from_millis(400));
    let e3 = p.elapsed();
    assert!(
        e3 - e2 < Duration::from_millis(150),
        "暂停时 elapsed 不应增长: {e2:?} -> {e3:?}"
    );

    // 恢复后继续增长
    p.toggle_pause();
    assert_eq!(p.state(), PlayState::Playing);

    p.stop();
    assert_eq!(p.state(), PlayState::Stopped);

    let _ = fs::remove_file(&wav);
}

#[test]
fn detects_natural_finish() {
    let wav = temp_wav("finish", 1.0);
    let mut p = Player::new();
    p.play(&wav).expect("ffplay 启动失败");

    // 1 秒的音频应在 5 秒内自然播完
    let mut finished = false;
    for _ in 0..50 {
        if p.poll_finished() {
            finished = true;
            break;
        }
        sleep(Duration::from_millis(100));
    }
    assert!(finished, "短音频应自然播放结束");
    assert_eq!(p.state(), PlayState::Stopped);

    let _ = fs::remove_file(&wav);
}

#[test]
fn play_missing_file_reports_error_or_finishes() {
    // 不存在的文件：ffplay 可能 spawn 成功但立刻退出，不应导致 panic
    let mut p = Player::new();
    let path = PathBuf::from("/nonexistent/definitely_missing.mp3");
    let _ = p.play(&path); // 允许 Ok 或 Err
    sleep(Duration::from_millis(500));
    let _ = p.poll_finished();
    p.stop();
}
