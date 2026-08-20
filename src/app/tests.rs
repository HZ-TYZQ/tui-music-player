use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::config::{AppConfig, AppPaths};
use crate::track::{RepeatMode, Track};

use super::{App, Overlay};

fn test_app(config: AppConfig) -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    let music = temp.path().join("music");
    std::fs::create_dir(&music).unwrap();
    let paths = AppPaths::from_roots(
        temp.path().join("config"),
        temp.path().join("data"),
        temp.path().join("cache"),
        Some(music.clone()),
    );
    let app = App::new_for_tests(music, paths, config, None, false).unwrap();
    (temp, app)
}

fn track(path: PathBuf) -> Track {
    Track {
        relative_path: path.file_name().unwrap().into(),
        path,
        title: "Song".to_owned(),
        artist: None,
        album: None,
        duration: Some(Duration::from_secs(1)),
        format: Some("WAV".to_owned()),
        file_size: 1,
        modified_ns: 1,
    }
}

fn settle_search_and_selection(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        app.search.tick();
        app.restore_pending_selection();
        if !app.search.is_running() && app.pending_selected_path.is_none() {
            return;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("搜索和待恢复选择未在截止时间内完成");
}

#[test]
fn playback_modes_choose_expected_library_index() {
    let (_temp, mut app) = test_app(AppConfig::default());
    app.tracks = (0..3)
        .map(|index| track(PathBuf::from(format!("/music/{index}.wav"))))
        .collect();
    app.playing_index = Some(2);

    app.config.repeat = RepeatMode::None;
    app.config.shuffle = false;
    assert_eq!(app.next_library_index(true), None);
    app.config.repeat = RepeatMode::All;
    assert_eq!(app.next_library_index(true), Some(0));
    app.config.repeat = RepeatMode::One;
    assert_eq!(app.next_library_index(true), Some(2));
    assert_eq!(app.next_library_index(false), Some(0));
}

#[test]
fn immediate_play_does_not_clear_manual_queue() {
    let (_temp, mut app) = test_app(AppConfig::default());
    let selected = PathBuf::from("/missing/selected.wav");
    let queued = PathBuf::from("/music/queued.wav");
    app.tracks = vec![track(selected)];
    app.search.replace_tracks(&app.tracks);
    app.queue.push_back(queued.clone());
    app.play_selected();
    assert_eq!(app.queue.front(), Some(&queued));
}

#[test]
fn sequential_next_skips_a_broken_track_without_polluting_history() {
    let (temp, mut app) = test_app(AppConfig::default());
    let music = temp.path().join("music");
    let first = music.join("a.wav");
    let broken = music.join("b.wav");
    let third = music.join("c.wav");
    write_test_wav(&first);
    std::fs::write(&broken, b"not audio").unwrap();
    write_test_wav(&third);
    app.tracks = vec![track(first.clone()), track(broken), track(third.clone())];
    app.search.replace_tracks(&app.tracks);
    app.playing_index = Some(0);

    app.play_next(false);

    assert_eq!(app.playing_index, Some(2));
    let current_path = app.player.current_path().unwrap().canonicalize().unwrap();
    assert_eq!(current_path, third.canonicalize().unwrap());
    assert_eq!(app.history, vec![first]);
}

#[test]
fn rescan_preserves_selected_track_by_path() {
    let (_temp, mut app) = test_app(AppConfig::default());
    let paths: Vec<PathBuf> = (0..4)
        .map(|index| PathBuf::from(format!("/music/{index}.wav")))
        .collect();
    app.tracks = paths.iter().map(|path| track(path.clone())).collect();
    app.search.replace_tracks(&app.tracks);
    app.selected = 2;

    let mut rescanned: Vec<Track> = paths.iter().map(|path| track(path.clone())).collect();
    rescanned.rotate_left(2);
    app.apply_scan_finished(rescanned, Vec::new());

    assert_eq!(
        app.selected_track().map(|track| track.path.clone()),
        Some(paths[2].clone())
    );
}

#[test]
fn rescan_preserves_selected_track_after_active_search_settles() {
    let (_temp, mut app) = test_app(AppConfig::default());
    let paths: Vec<PathBuf> = (0..4)
        .map(|index| PathBuf::from(format!("/music/{index}.wav")))
        .collect();
    app.tracks = paths.iter().map(|path| track(path.clone())).collect();
    app.search.replace_tracks(&app.tracks);
    app.search.set_query("Song".to_owned());
    settle_search_and_selection(&mut app);
    assert_eq!(app.visible_indices(), &[0, 1, 2, 3]);
    app.selected = 2;

    let mut rescanned: Vec<Track> = paths.iter().map(|path| track(path.clone())).collect();
    // 2.wav 重扫后位于可见结果第 1 项，确保错误回退到第 0 项无法通过测试。
    rescanned.rotate_left(1);
    app.apply_scan_finished(rescanned, Vec::new());
    settle_search_and_selection(&mut app);

    assert_eq!(app.selected, 1);
    assert_eq!(
        app.selected_track().map(|track| track.path.clone()),
        Some(paths[2].clone())
    );
}

#[test]
fn rescan_falls_back_to_first_match_when_selected_track_stops_matching() {
    let (_temp, mut app) = test_app(AppConfig::default());
    let first_path = PathBuf::from("/music/first.wav");
    let selected_path = PathBuf::from("/music/selected.wav");
    let mut first = track(first_path.clone());
    first.title = "Other".to_owned();
    let mut selected = track(selected_path.clone());
    selected.title = "Favorite".to_owned();
    app.tracks = vec![first, selected];
    app.search.replace_tracks(&app.tracks);
    app.search.set_query("Favorite".to_owned());
    settle_search_and_selection(&mut app);
    assert_eq!(
        app.selected_track().map(|track| &track.path),
        Some(&selected_path)
    );

    let mut replacement_match = track(first_path.clone());
    replacement_match.title = "Favorite".to_owned();
    let mut replacement_selected = track(selected_path);
    replacement_selected.title = "Other".to_owned();
    app.apply_scan_finished(vec![replacement_match, replacement_selected], Vec::new());
    settle_search_and_selection(&mut app);

    assert_eq!(app.selected, 0);
    assert_eq!(
        app.selected_track().map(|track| &track.path),
        Some(&first_path)
    );
}

#[test]
fn user_actions_cancel_pending_selection_restore() {
    let (_temp, mut app) = test_app(AppConfig::default());
    app.tracks = (0..3)
        .map(|index| track(PathBuf::from(format!("/music/{index}.wav"))))
        .collect();
    app.search.replace_tracks(&app.tracks);

    app.pending_selected_path = Some(app.tracks[2].path.clone());
    app.select_next();
    assert_eq!(app.selected, 1);
    assert!(app.pending_selected_path.is_none());

    app.pending_selected_path = Some(app.tracks[2].path.clone());
    app.selected = 2;
    app.select_previous();
    assert_eq!(app.selected, 1);
    assert!(app.pending_selected_path.is_none());

    for code in [KeyCode::Char('x'), KeyCode::Backspace, KeyCode::Esc] {
        app.pending_selected_path = Some(app.tracks[2].path.clone());
        app.handle_search_key(code);
        assert!(app.pending_selected_path.is_none());
    }

    app.pending_selected_path = Some(app.tracks[2].path.clone());
    app.handle_search_key(KeyCode::Enter);
    assert!(app.pending_selected_path.is_none());
}

#[test]
fn rescan_resets_selection_when_selected_track_disappears() {
    let (_temp, mut app) = test_app(AppConfig::default());
    app.tracks = (0..3)
        .map(|index| track(PathBuf::from(format!("/music/{index}.wav"))))
        .collect();
    app.search.replace_tracks(&app.tracks);
    app.selected = 2;

    app.apply_scan_finished(vec![track(PathBuf::from("/music/new.wav"))], Vec::new());

    assert_eq!(app.selected, 0);
    assert_eq!(
        app.selected_track().map(|track| track.path.clone()),
        Some(PathBuf::from("/music/new.wav"))
    );
    assert_eq!(app.playing_index, None);
}

#[test]
fn shuffle_does_not_immediately_repeat_current_track() {
    let config = AppConfig {
        shuffle: true,
        ..AppConfig::default()
    };
    let (_temp, mut app) = test_app(config);
    app.tracks = (0..4)
        .map(|index| track(PathBuf::from(format!("/music/{index}.wav"))))
        .collect();
    app.playing_index = Some(2);
    app.reanchor_shuffle_bag(2);
    for _ in 0..3 {
        assert_ne!(app.next_library_index(true), Some(2));
    }
}

#[test]
fn z_cycles_repeat_only_and_s_toggles_shuffle() {
    let (_temp, mut app) = test_app(AppConfig::default());
    assert_eq!(app.config.repeat, RepeatMode::None);
    assert!(!app.config.shuffle);

    app.handle_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE));
    assert_eq!(app.config.repeat, RepeatMode::All);
    assert!(!app.config.shuffle);

    app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
    assert_eq!(app.config.repeat, RepeatMode::All);
    assert!(app.config.shuffle);

    app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
    assert!(!app.config.shuffle);
    assert_eq!(app.config.repeat, RepeatMode::All);
}

#[test]
fn shuffle_bag_exhausts_then_stops_without_repeat() {
    let config = AppConfig {
        shuffle: true,
        repeat: RepeatMode::None,
        ..AppConfig::default()
    };
    let (_temp, mut app) = test_app(config);
    app.tracks = (0..3)
        .map(|index| track(PathBuf::from(format!("/music/{index}.wav"))))
        .collect();
    app.playing_index = Some(0);
    app.reanchor_shuffle_bag(0);
    let mut seen = vec![0];
    while let Some(next) = app.next_library_index(true) {
        seen.push(next);
    }
    assert_eq!(seen.len(), 3);
    let mut unique = seen.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique, vec![0, 1, 2]);
    assert_eq!(app.next_library_index(true), None);
}

#[test]
fn shuffle_repeat_all_reshuffles_after_bag() {
    let config = AppConfig {
        shuffle: true,
        repeat: RepeatMode::All,
        ..AppConfig::default()
    };
    let (_temp, mut app) = test_app(config);
    app.tracks = (0..3)
        .map(|index| track(PathBuf::from(format!("/music/{index}.wav"))))
        .collect();
    app.playing_index = Some(0);
    app.reanchor_shuffle_bag(0);
    for _ in 0..2 {
        assert!(app.next_library_index(true).is_some());
    }
    let first_of_next_round = app.next_library_index(true);
    assert!(first_of_next_round.is_some());
    assert_eq!(app.shuffle_cursor, 1);
    assert_eq!(app.shuffle_order.len(), 3);
}

#[test]
fn visualizer_toggle_updates_persisted_setting() {
    let (_temp, mut app) = test_app(AppConfig::default());
    assert!(app.config.visualizer_enabled);

    app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
    assert!(!app.config.visualizer_enabled);
    assert!(app.spectrum_bars().iter().all(|value| *value == 0.0));

    app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
    assert!(app.config.visualizer_enabled);
}

#[test]
fn visualizer_key_does_not_escape_search_or_overlay_modes() {
    let (_temp, mut app) = test_app(AppConfig::default());
    app.search_active = true;
    app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
    assert!(app.config.visualizer_enabled);
    assert_eq!(app.search.query(), "v");

    app.search_active = false;
    app.overlay = Overlay::Help;
    app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
    assert!(app.config.visualizer_enabled);
    assert_eq!(app.overlay, Overlay::Help);
}

#[test]
fn duration_column_width_follows_the_longest_track_duration() {
    let (_temp, mut app) = test_app(AppConfig::default());
    assert_eq!(app.duration_column_width, 5);

    app.apply_scan_finished(vec![track(PathBuf::from("/music/a.wav"))], Vec::new());
    assert_eq!(app.duration_column_width, 5);

    let mut long = track(PathBuf::from("/music/long.wav"));
    long.duration = Some(Duration::from_secs(3_661));
    app.apply_scan_finished(vec![long], Vec::new());
    assert_eq!(app.duration_column_width, 7);

    app.apply_scan_finished(Vec::new(), Vec::new());
    assert_eq!(app.duration_column_width, 5);
}

fn write_test_wav(path: &Path) {
    let sample_rate = 8_000_u32;
    let sample_count = 800_usize;
    let data_len = (sample_count * 2) as u32;
    let mut wav = Vec::with_capacity(44 + data_len as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    wav.extend_from_slice(&2_u16.to_le_bytes());
    wav.extend_from_slice(&16_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    wav.resize(44 + data_len as usize, 0);
    std::fs::write(path, wav).unwrap();
}
