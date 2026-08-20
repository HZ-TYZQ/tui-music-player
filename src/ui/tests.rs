use std::path::PathBuf;
use std::time::Duration;

use ratatui::prelude::Color;
use unicode_width::UnicodeWidthStr;

use crate::player::PlayState;
use crate::theme::DEFAULT_THEME;
use crate::track::Track;

use super::library::{
    FORMAT_COLUMN_WIDTH, INACTIVE_ICON, LIST_GAP_WIDTH, LIST_ICON_WIDTH, MIN_ALBUM_WIDTH,
    MIN_ARTIST_WIDTH, MIN_TITLE_WIDTH, PAUSE_ACTION_ICON, PLAY_ACTION_ICON, STOPPED_ICON,
    playback_action_indicator, track_row_columns, track_row_layout,
};
use super::text::{ascii_progress_bar, fmt_duration, now_playing_text, truncate_display};
use super::visualizer::{frequency_color, resample_spectrum, spectrum_block, visualizer_height};

#[test]
fn formats_short_and_long_durations() {
    assert_eq!(fmt_duration(Duration::from_secs(65)), "1:05");
    assert_eq!(fmt_duration(Duration::from_secs(3661)), "1:01:01");
}

#[test]
fn visualizer_uses_available_height_without_breaking_base_layout() {
    assert_eq!(visualizer_height(12, true), 0);
    assert_eq!(visualizer_height(13, true), 2);
    assert_eq!(visualizer_height(16, true), 5);
    assert_eq!(visualizer_height(40, true), 5);
    assert_eq!(visualizer_height(40, false), 0);
}

#[test]
fn spectrum_resampling_handles_narrow_wide_and_invalid_input() {
    assert_eq!(resample_spectrum(&[0.1, 0.8, 0.3, 0.6], 2), vec![0.8, 0.6]);
    assert_eq!(
        resample_spectrum(&[0.25, 0.75], 4),
        vec![0.25, 0.25, 0.75, 0.75]
    );
    assert_eq!(resample_spectrum(&[f32::NAN, 2.0], 2), vec![0.0, 1.0]);
    assert!(resample_spectrum(&[], 10).is_empty());
    assert!(resample_spectrum(&[0.5], 0).is_empty());
}

#[test]
fn spectrum_blocks_cover_empty_fractional_and_full_cells() {
    assert_eq!(spectrum_block(0.0), ' ');
    assert_eq!(spectrum_block(0.01), '▁');
    assert_eq!(spectrum_block(0.5), '▄');
    assert_eq!(spectrum_block(1.0), '█');
}

#[test]
fn playback_icon_describes_the_space_key_action() {
    assert_eq!(
        playback_action_indicator(PlayState::Playing, &DEFAULT_THEME).0,
        PAUSE_ACTION_ICON
    );
    assert_eq!(
        playback_action_indicator(PlayState::Paused, &DEFAULT_THEME).0,
        PLAY_ACTION_ICON
    );
    assert_eq!(
        playback_action_indicator(PlayState::Stopped, &DEFAULT_THEME).0,
        STOPPED_ICON
    );
}

#[test]
fn playback_icons_have_fixed_display_width() {
    for state in [PlayState::Playing, PlayState::Paused, PlayState::Stopped] {
        assert_eq!(
            playback_action_indicator(state, &DEFAULT_THEME)
                .0
                .chars()
                .count(),
            3
        );
    }
    assert_eq!(INACTIVE_ICON.chars().count(), 3);
}

#[test]
fn ascii_progress_bar_has_fixed_width_and_expected_charset() {
    for width in 0..=40 {
        for ratio in [0.0, 0.25, 0.5, 0.99, 1.0] {
            let bar = ascii_progress_bar(ratio, width);
            assert_eq!(bar.chars().count(), width);
            assert!(bar.chars().all(|cell| matches!(cell, '=' | '>' | '-')));
        }
    }
}

#[test]
fn ascii_progress_bar_renders_head_and_completion() {
    assert_eq!(ascii_progress_bar(0.0, 10), ">---------");
    assert_eq!(ascii_progress_bar(0.5, 10), "=====>----");
    assert_eq!(ascii_progress_bar(0.999, 10), "=========>");
    assert_eq!(ascii_progress_bar(1.0, 10), "==========");
    assert_eq!(ascii_progress_bar(0.5, 1), ">");
    assert_eq!(ascii_progress_bar(1.0, 1), "=");
    assert_eq!(ascii_progress_bar(0.5, 0), "");
}

#[test]
fn truncate_display_limits_by_terminal_width() {
    assert_eq!(truncate_display("hello", 10), "hello");
    assert_eq!(truncate_display("hello world", 8), "hello w…");
    assert_eq!(truncate_display("你好世界", 5), "你好…");
    assert_eq!(truncate_display("ab你好cd", 6), "ab你…");
    assert_eq!(truncate_display("abc", 1), "…");
    assert_eq!(truncate_display("abc", 0), "");
}

#[test]
fn track_row_layout_tiers_follow_derived_min_widths() {
    let duration = 5;
    let full_min = LIST_ICON_WIDTH
        + MIN_TITLE_WIDTH
        + MIN_ARTIST_WIDTH
        + MIN_ALBUM_WIDTH
        + FORMAT_COLUMN_WIDTH
        + duration
        + LIST_GAP_WIDTH * 4;
    let layout = track_row_layout(full_min, duration);
    assert!(layout.artist.is_some() && layout.album.is_some() && layout.format);
    let layout = track_row_layout(full_min - 1, duration);
    assert!(layout.artist.is_some() && layout.album.is_some() && !layout.format);

    let no_album_min =
        LIST_ICON_WIDTH + MIN_TITLE_WIDTH + MIN_ARTIST_WIDTH + duration + LIST_GAP_WIDTH * 2;
    let layout = track_row_layout(no_album_min, duration);
    assert!(layout.artist.is_some() && layout.album.is_none() && !layout.format);
    let layout = track_row_layout(no_album_min - 1, duration);
    assert!(layout.artist.is_none() && layout.album.is_none() && !layout.format);
}

#[test]
fn track_row_columns_respect_the_layout_width() {
    let track = Track {
        path: PathBuf::from("/music/长标题.flac"),
        relative_path: PathBuf::from("长标题.flac"),
        title: "这是一首名字特别特别长的歌曲标题".to_owned(),
        artist: Some("一个名字同样很长的歌手".to_owned()),
        album: Some("一张名字也非常非常长的专辑".to_owned()),
        duration: Some(Duration::from_secs(3_661)),
        format: Some("MPEG-4 AAC".to_owned()),
        file_size: 1,
        modified_ns: 1,
    };
    let duration_width = 7;
    for usable in [22usize, 30, 42, 50, 66, 90] {
        let layout = track_row_layout(usable, duration_width);
        let columns = track_row_columns(&track, &layout);
        let text_width = columns
            .iter()
            .map(|column| UnicodeWidthStr::width(column.as_str()))
            .sum::<usize>()
            + LIST_GAP_WIDTH * (columns.len() - 1);
        assert_eq!(text_width + LIST_ICON_WIDTH, usable, "usable={usable}");
        assert!(!columns.iter().any(|column| column.contains("MPEG-4")));
    }
}

#[test]
fn now_playing_text_truncates_title_and_artist_as_a_whole() {
    let (title, artist) = now_playing_text("春日影", Some("MyGO!!!!!"), 30);
    assert_eq!(title, "春日影");
    assert_eq!(artist, " — MyGO!!!!!");

    let (title, artist) = now_playing_text("春日影", Some("MyGO!!!!!"), 12);
    assert_eq!(title, "春日影");
    assert_eq!(
        UnicodeWidthStr::width(title.as_str()) + UnicodeWidthStr::width(artist.as_str()),
        12
    );

    let (title, artist) = now_playing_text("这是一首名字特别特别长的歌", Some("X"), 9);
    assert_eq!(artist, "");
    assert!(UnicodeWidthStr::width(title.as_str()) <= 9);

    let (title, artist) = now_playing_text("歌", None, 30);
    assert_eq!(title, "歌");
    assert_eq!(artist, "");
}

#[test]
fn playback_actions_share_primary_color_and_stopped_is_muted() {
    let playing = playback_action_indicator(PlayState::Playing, &DEFAULT_THEME).1;
    let paused = playback_action_indicator(PlayState::Paused, &DEFAULT_THEME).1;
    let stopped = playback_action_indicator(PlayState::Stopped, &DEFAULT_THEME).1;

    assert_eq!(playing, paused);
    assert_eq!(playing.fg, Some(DEFAULT_THEME.primary));
    assert_eq!(stopped.fg, Some(DEFAULT_THEME.muted));
}

#[test]
fn spectrum_gradient_uses_theme_endpoints() {
    assert_eq!(
        frequency_color(0, 32, &DEFAULT_THEME),
        Color::Rgb(168, 168, 168)
    );
    assert_eq!(
        frequency_color(31, 32, &DEFAULT_THEME),
        Color::Rgb(242, 242, 242)
    );
}
