use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::config::{AppConfig, AppPaths};
use crate::track::{RepeatMode, SortKey, Track};

use super::playback::short_reason;
use super::{App, BagUpdate, Overlay};

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
        track_number: None,
        disc_number: None,
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

    // 曲库会按配置排序，扫描顺序不再能制造位移；用少一首曲目让下标真正错位。
    let rescanned: Vec<Track> = [0, 2, 3]
        .iter()
        .map(|index| track(paths[*index].clone()))
        .collect();
    app.apply_scan_finished(rescanned, Vec::new());

    assert_eq!(app.selected, 1);
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

    // 2.wav 重扫后位于可见结果第 1 项，确保错误回退到第 0 项无法通过测试。
    let rescanned: Vec<Track> = [0, 2, 3]
        .iter()
        .map(|index| track(paths[*index].clone()))
        .collect();
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

#[test]
fn queue_panel_edits_reorder_remove_and_clear_the_queue() {
    let (_temp, mut app) = test_app(AppConfig::default());
    let paths: Vec<PathBuf> = (0..3)
        .map(|index| PathBuf::from(format!("/music/{index}.wav")))
        .collect();
    app.queue = paths.iter().cloned().collect();

    app.handle_key(KeyEvent::new(KeyCode::Char('Q'), KeyModifiers::NONE));
    assert_eq!(app.overlay, Overlay::Queue);

    // J 把首项下移一位，选择跟着它走。
    app.handle_key(KeyEvent::new(KeyCode::Char('J'), KeyModifiers::NONE));
    assert_eq!(app.queue_selected, 1);
    assert_eq!(app.queue[0], paths[1]);
    assert_eq!(app.queue[1], paths[0]);

    // K 移回原位。
    app.handle_key(KeyEvent::new(KeyCode::Char('K'), KeyModifiers::NONE));
    assert_eq!(app.queue_selected, 0);
    assert_eq!(app.queue[0], paths[0]);

    app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
    assert_eq!(app.queue.len(), 2);
    assert_eq!(app.queue[1], paths[2]);

    app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));
    assert!(app.queue.is_empty());
    assert_eq!(app.queue_selected, 0);

    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(app.overlay, Overlay::None);
}

#[test]
fn queue_selection_stays_in_range_when_the_last_entry_is_removed() {
    let (_temp, mut app) = test_app(AppConfig::default());
    app.queue = (0..2)
        .map(|index| PathBuf::from(format!("/music/{index}.wav")))
        .collect();
    app.overlay = Overlay::Queue;
    app.queue_selected = 1;

    app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));

    assert_eq!(app.queue.len(), 1);
    assert_eq!(app.queue_selected, 0);
}

#[test]
fn playing_a_queue_entry_drops_the_entries_before_it() {
    let (temp, mut app) = test_app(AppConfig::default());
    let music = temp.path().join("music");
    let skipped = music.join("a.wav");
    let wanted = music.join("b.wav");
    let rest = music.join("c.wav");
    write_test_wav(&skipped);
    write_test_wav(&wanted);
    write_test_wav(&rest);
    app.tracks = vec![
        track(skipped.clone()),
        track(wanted.clone()),
        track(rest.clone()),
    ];
    app.search.replace_tracks(&app.tracks);
    app.queue = vec![skipped, wanted.clone(), rest.clone()].into();
    app.overlay = Overlay::Queue;
    app.queue_selected = 1;

    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.overlay, Overlay::None);
    assert_eq!(app.playing_index, Some(1));
    let current_path = app.player.current_path().unwrap().canonicalize().unwrap();
    assert_eq!(current_path, wanted.canonicalize().unwrap());
    assert_eq!(app.queue.len(), 1);
    assert_eq!(app.queue.front(), Some(&rest));
}

#[test]
fn queue_keys_do_not_leak_to_the_library_while_the_panel_is_open() {
    let (_temp, mut app) = test_app(AppConfig::default());
    app.tracks = vec![track(PathBuf::from("/music/a.wav"))];
    app.search.replace_tracks(&app.tracks);
    app.overlay = Overlay::Queue;

    // c 在曲库里没有绑定，但 q 会退出程序、s 会切随机，都必须被弹层吃掉。
    app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));

    assert!(!app.should_quit);
    assert!(!app.config.shuffle);
    assert_eq!(app.overlay, Overlay::Queue);
}

#[test]
fn queue_panel_right_aligns_durations_and_keeps_every_entry_visible() {
    let (_temp, mut app) = test_app(AppConfig::default());
    let mut tracks: Vec<Track> = (0..12)
        .map(|index| {
            let mut item = track(PathBuf::from(format!("/music/{index:02}.wav")));
            item.title = format!("第{index}首很长很长很长很长很长很长的中文歌名 Extra Long Tail");
            item.artist = Some("某位歌手名字也不短".to_owned());
            item.duration = Some(Duration::from_secs(61 * (index as u64 + 1)));
            item
        })
        .collect();
    // 覆盖三种排版分支：缺歌手、标题短到留白多、路径已不在曲库中。
    tracks[3].artist = None;
    tracks[4].title = "Short".to_owned();
    app.tracks = tracks;
    let missing = PathBuf::from("/gone/不在库里的一首歌.flac");
    app.queue = app
        .tracks
        .iter()
        .map(|track| track.path.clone())
        .chain(std::iter::once(missing))
        .collect();
    app.overlay = Overlay::Queue;

    let backend = ratatui::backend::TestBackend::new(100, 22);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal.draw(|frame| crate::ui::draw(frame, &app)).unwrap();
    let buffer = terminal.backend().buffer();
    let rendered: Vec<String> = (0..22)
        .map(|row| {
            (0..100)
                .map(|column| buffer[(column, row)].symbol())
                .collect::<Vec<_>>()
                .join("")
        })
        .collect();

    // 每首曲目的时长都必须落在同一列右端，长中文标题也不例外。
    // 按缓冲区列测量：字节偏移会被中文字符的多字节编码带偏。
    let duration_ends: Vec<u16> = rendered
        .iter()
        .enumerate()
        .filter(|(_, line)| line.contains("Extra Long Tail") || line.contains("5. Short"))
        .map(|(row, _)| {
            (0..100)
                .rev()
                .find(|column| {
                    let symbol = buffer[(*column, row as u16)].symbol();
                    symbol.len() == 1
                        && symbol.starts_with(|character: char| character.is_ascii_digit())
                })
                .expect("时长列缺失")
        })
        .collect();
    assert_eq!(duration_ends.len(), 12);
    assert!(duration_ends.windows(2).all(|pair| pair[0] == pair[1]));

    // 最后一条和底部提示行必须同时可见，提示行不能盖住任何条目。
    // TestBackend 把宽字符的第二格填成空格，所以只能用 ASCII 片段定位这两行。
    let last_entry = rendered
        .iter()
        .position(|line| line.contains(".flac"))
        .expect("队列末项被遮挡");
    let help = rendered
        .iter()
        .position(|line| line.contains("J/K"))
        .expect("提示行缺失");
    assert!(last_entry < help);
}

#[test]
fn skipping_a_broken_track_leaves_a_notice_after_the_next_track_starts() {
    let (temp, mut app) = test_app(AppConfig::default());
    let music = temp.path().join("music");
    let first = music.join("a.wav");
    let broken = music.join("b.wav");
    let good = music.join("c.wav");
    write_test_wav(&first);
    std::fs::write(&broken, b"not audio").unwrap();
    write_test_wav(&good);
    let mut broken_track = track(broken);
    broken_track.title = "坏文件".to_owned();
    app.tracks = vec![track(first), broken_track, track(good)];
    app.search.replace_tracks(&app.tracks);
    app.playing_index = Some(0);

    app.play_next(false);

    // 成功切歌会清空 message，跳过原因必须在那之后重新写回来。
    assert_eq!(app.playing_index, Some(2));
    let message = app.message.clone().expect("跳过提示丢失");
    assert!(message.contains("已跳过"), "{message}");
    assert!(message.contains("坏文件"), "{message}");
}

#[test]
fn several_skips_in_one_advance_collapse_into_a_counted_notice() {
    let (temp, mut app) = test_app(AppConfig::default());
    let music = temp.path().join("music");
    let good = music.join("c.wav");
    write_test_wav(&good);
    let mut broken = Vec::new();
    for name in ["b1.wav", "b2.wav"] {
        let path = music.join(name);
        std::fs::write(&path, b"not audio").unwrap();
        broken.push(path);
    }
    app.tracks = vec![
        track(music.join("start.wav")),
        track(broken[0].clone()),
        track(broken[1].clone()),
        track(good),
    ];
    app.search.replace_tracks(&app.tracks);
    app.playing_index = Some(0);

    app.play_next(false);

    assert_eq!(app.playing_index, Some(3));
    let message = app.message.clone().expect("跳过提示丢失");
    assert!(message.starts_with("已跳过 2 首"), "{message}");
}

#[test]
fn a_queue_entry_missing_from_the_library_is_named_in_the_notice() {
    let (temp, mut app) = test_app(AppConfig::default());
    let music = temp.path().join("music");
    let good = music.join("c.wav");
    write_test_wav(&good);
    app.tracks = vec![track(good)];
    app.search.replace_tracks(&app.tracks);
    app.queue = vec![PathBuf::from("/gone/不在库里.flac")].into();
    app.overlay = Overlay::Queue;
    // 列表循环让“下一首”与光标是否已经落到曲库上无关，避免依赖搜索线程的时序。
    app.config.repeat = RepeatMode::All;

    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.playing_index, Some(0));
    let message = app.message.clone().expect("跳过提示丢失");
    assert!(message.contains("不在库里.flac"), "{message}");
    assert!(message.contains("不在曲库中"), "{message}");
}

#[test]
fn playing_a_track_directly_reports_the_failure_instead_of_a_skip() {
    let (temp, mut app) = test_app(AppConfig::default());
    let broken = temp.path().join("music").join("broken.wav");
    std::fs::write(&broken, b"not audio").unwrap();
    let mut broken_track = track(broken);
    broken_track.title = "坏文件".to_owned();
    app.tracks = vec![broken_track];
    app.search.replace_tracks(&app.tracks);

    app.play_selected();

    assert_eq!(app.playing_index, None);
    let message = app.message.clone().expect("失败提示丢失");
    assert!(message.starts_with("无法播放“坏文件”"), "{message}");
}

#[test]
fn a_clean_track_change_still_clears_the_previous_message() {
    let (temp, mut app) = test_app(AppConfig::default());
    let good = temp.path().join("music").join("a.wav");
    write_test_wav(&good);
    app.tracks = vec![track(good)];
    app.search.replace_tracks(&app.tracks);
    app.message = Some("音量 50%".to_owned());

    app.play_selected();

    assert_eq!(app.playing_index, Some(0));
    assert_eq!(app.message, None);
}

#[test]
fn short_reason_drops_the_path_from_every_player_error_shape() {
    let path = Path::new("/music/曲目 名.mp3");
    let cases = [
        ("音频文件不存在: /music/曲目 名.mp3", "音频文件不存在"),
        ("空文件: /music/曲目 名.mp3", "空文件"),
        ("暂不支持 Opus: /music/曲目 名.mp3", "暂不支持 Opus"),
        (
            "无法识别或解码 /music/曲目 名.mp3: end of stream",
            "无法识别或解码: end of stream",
        ),
        (
            "无法打开 /music/曲目 名.mp3: Permission denied",
            "无法打开: Permission denied",
        ),
    ];
    for (error, expected) in cases {
        assert_eq!(short_reason(error, path), expected, "{error}");
    }
}

fn titled_track(path: PathBuf, title: &str) -> Track {
    let mut item = track(path);
    item.title = title.to_owned();
    item
}

/// 三首歌的路径顺序（a、b、c）和标题顺序（alpha、mid、zeta）刻意相反，
/// 这样排序切换一定会移动下标，测试不会因为两种顺序恰好一致而失效。
fn sort_fixture(app: &mut App, music: &Path) -> Vec<PathBuf> {
    let paths: Vec<PathBuf> = ["a.wav", "b.wav", "c.wav"]
        .iter()
        .map(|name| music.join(name))
        .collect();
    for path in &paths {
        write_test_wav(path);
    }
    app.tracks = vec![
        titled_track(paths[0].clone(), "zeta"),
        titled_track(paths[1].clone(), "alpha"),
        titled_track(paths[2].clone(), "mid"),
    ];
    app.search.replace_tracks(&app.tracks);
    paths
}

#[test]
fn cycling_the_sort_key_reorders_the_library_and_keeps_the_playing_track() {
    let (temp, mut app) = test_app(AppConfig::default());
    let paths = sort_fixture(&mut app, &temp.path().join("music"));
    app.play_index(0, false, BagUpdate::Reanchor).unwrap();
    assert_eq!(app.playing_index, Some(0));

    app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));

    assert_eq!(app.config.sort.key, SortKey::Title);
    let order: Vec<&str> = app
        .tracks
        .iter()
        .map(|track| track.title.as_str())
        .collect();
    assert_eq!(order, ["alpha", "mid", "zeta"]);
    // 正在播放的是同一个文件，只是它在列表里的位置变了。
    assert_eq!(app.playing_index, Some(2));
    assert_eq!(
        app.player.current_path().unwrap().canonicalize().unwrap(),
        paths[0].canonicalize().unwrap()
    );
}

#[test]
fn toggling_the_sort_direction_reverses_the_library() {
    let (temp, mut app) = test_app(AppConfig::default());
    sort_fixture(&mut app, &temp.path().join("music"));
    app.config.sort.key = SortKey::Title;

    app.handle_key(KeyEvent::new(KeyCode::Char('O'), KeyModifiers::NONE));

    assert!(app.config.sort.descending);
    let order: Vec<&str> = app
        .tracks
        .iter()
        .map(|track| track.title.as_str())
        .collect();
    assert_eq!(order, ["zeta", "mid", "alpha"]);
    assert!(app.message.as_deref().unwrap().contains("标题 ↓"));
}

#[test]
fn resorting_reanchors_the_shuffle_bag_onto_the_new_indices() {
    let (temp, mut app) = test_app(AppConfig::default());
    sort_fixture(&mut app, &temp.path().join("music"));
    app.config.shuffle = true;
    app.play_index(0, false, BagUpdate::Reanchor).unwrap();

    app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));

    // 重排让所有下标失效，随机袋必须重建到新下标上，并从当前曲目开始。
    assert_eq!(app.shuffle_order.first().copied(), app.playing_index);
    let mut covered = app.shuffle_order.clone();
    covered.sort_unstable();
    assert_eq!(covered, (0..app.tracks.len()).collect::<Vec<_>>());
}

#[test]
fn a_rescan_applies_the_configured_sort() {
    let (_temp, mut app) = test_app(AppConfig::default());
    app.config.sort.key = SortKey::Title;

    app.apply_scan_finished(
        vec![
            titled_track(PathBuf::from("/music/a.wav"), "zeta"),
            titled_track(PathBuf::from("/music/b.wav"), "alpha"),
        ],
        Vec::new(),
    );

    let order: Vec<&str> = app
        .tracks
        .iter()
        .map(|track| track.title.as_str())
        .collect();
    assert_eq!(order, ["alpha", "zeta"]);
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
