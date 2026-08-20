//! 曲库列表、播放指示器和响应式列宽。

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, List, ListItem, ListState, Paragraph};

use crate::app::App;
use crate::player::PlayState;
use crate::theme::Theme;
use crate::track::Track;

use super::text::{column_text, fmt_duration};

pub(super) const PAUSE_ACTION_ICON: &str = "|| ";
pub(super) const PLAY_ACTION_ICON: &str = ">  ";
pub(super) const STOPPED_ICON: &str = "■  ";
pub(super) const INACTIVE_ICON: &str = "   ";
pub(super) const LIST_ICON_WIDTH: usize = 3;
pub(super) const LIST_GAP_WIDTH: usize = 2;
pub(super) const FORMAT_COLUMN_WIDTH: usize = 6;
pub(super) const MIN_TITLE_WIDTH: usize = 12;
pub(super) const MIN_ARTIST_WIDTH: usize = 8;
pub(super) const MIN_ALBUM_WIDTH: usize = 8;
const HIGHLIGHT_SYMBOL_WIDTH: usize = 2;

pub(super) fn draw_library(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let scan = if app.scanning {
        format!(" · 扫描中 {}/{} ", app.scan_progress.0, app.scan_progress.1)
    } else {
        format!(" · {} 首 ", app.visible_indices().len())
    };
    let title = Line::from(vec![
        Span::styled(" ♪ Music Player ", Style::new().fg(theme.primary).bold()),
        Span::styled(
            format!("· {}", app.library_dir.display()),
            Style::new().fg(theme.muted),
        ),
        Span::styled(scan, Style::new().fg(theme.muted)),
    ]);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(theme.border))
        .title(title);

    if app.tracks.is_empty() {
        let text = if app.scanning {
            "  正在后台扫描音乐库……\n  界面仍可响应，扫描完成后歌曲会自动出现"
        } else {
            "  音乐库中没有支持的音频文件\n  按 r 重新扫描，或用 --set-library PATH 更换主库"
        };
        frame.render_widget(
            Paragraph::new(text)
                .style(Style::new().fg(theme.muted))
                .block(block),
            area,
        );
        return;
    }

    let usable = usize::from(area.width).saturating_sub(2 + HIGHLIGHT_SYMBOL_WIDTH);
    let layout = track_row_layout(usable, app.duration_column_width as usize);
    let muted = Style::new().fg(theme.muted);
    let items = app.visible_indices().iter().filter_map(|index| {
        let track = app.tracks.get(*index)?;
        let current = app.playing_index == Some(*index);
        let (icon, icon_style) = if current {
            playback_action_indicator(app.player.state(), theme)
        } else {
            (INACTIVE_ICON, Style::new().fg(theme.primary))
        };
        let title_style = if current {
            Style::new().fg(theme.primary).bold()
        } else {
            Style::new().fg(theme.primary)
        };
        let mut columns = track_row_columns(track, &layout).into_iter();
        let title = columns.next()?;
        let mut spans = vec![
            Span::styled(icon, icon_style),
            Span::styled(title, title_style),
        ];
        for column in columns {
            spans.push(Span::styled("  ", muted));
            spans.push(Span::styled(column, muted));
        }
        Some(ListItem::new(Line::from(spans)))
    });

    let list = List::new(items)
        .block(block)
        .highlight_style(Style::new().bg(theme.selection_bg).bold())
        .highlight_symbol(Span::styled("▸ ", Style::new().fg(theme.primary)));
    let selected = (!app.visible_indices().is_empty()).then_some(app.selected);
    let mut state = ListState::default().with_selected(selected);
    frame.render_stateful_widget(list, area, &mut state);
}

pub(super) fn playback_action_indicator(state: PlayState, theme: &Theme) -> (&'static str, Style) {
    match state {
        PlayState::Playing => (PAUSE_ACTION_ICON, Style::new().fg(theme.primary).bold()),
        PlayState::Paused => (PLAY_ACTION_ICON, Style::new().fg(theme.primary).bold()),
        PlayState::Stopped => (STOPPED_ICON, Style::new().fg(theme.muted)),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TrackRowLayout {
    pub(super) title: usize,
    pub(super) artist: Option<usize>,
    pub(super) album: Option<usize>,
    pub(super) format: bool,
    pub(super) duration: usize,
}

pub(super) fn track_row_layout(usable: usize, duration_width: usize) -> TrackRowLayout {
    let tiers = [
        (true, true, true),
        (true, true, false),
        (true, false, false),
        (false, false, false),
    ];
    for &(show_artist, show_album, show_format) in &tiers {
        let content_columns =
            2 + usize::from(show_artist) + usize::from(show_album) + usize::from(show_format);
        let minimum = LIST_ICON_WIDTH
            + duration_width
            + if show_format { FORMAT_COLUMN_WIDTH } else { 0 }
            + MIN_TITLE_WIDTH
            + if show_artist { MIN_ARTIST_WIDTH } else { 0 }
            + if show_album { MIN_ALBUM_WIDTH } else { 0 }
            + LIST_GAP_WIDTH * (content_columns - 1);
        if usable >= minimum {
            return distribute_row_width(
                usable,
                duration_width,
                show_artist,
                show_album,
                show_format,
            );
        }
    }
    distribute_row_width(usable, duration_width, false, false, false)
}

fn distribute_row_width(
    usable: usize,
    duration_width: usize,
    show_artist: bool,
    show_album: bool,
    show_format: bool,
) -> TrackRowLayout {
    let content_columns =
        2 + usize::from(show_artist) + usize::from(show_album) + usize::from(show_format);
    let fixed = LIST_ICON_WIDTH
        + duration_width
        + LIST_GAP_WIDTH * (content_columns - 1)
        + if show_format { FORMAT_COLUMN_WIDTH } else { 0 };
    let variable_area = usable.saturating_sub(fixed);
    if show_artist && show_album {
        let extra =
            variable_area.saturating_sub(MIN_TITLE_WIDTH + MIN_ARTIST_WIDTH + MIN_ALBUM_WIDTH);
        let mut title = MIN_TITLE_WIDTH + extra * 45 / 100;
        let artist = MIN_ARTIST_WIDTH + extra * 30 / 100;
        let album = MIN_ALBUM_WIDTH + extra * 25 / 100;
        title += variable_area.saturating_sub(title + artist + album);
        TrackRowLayout {
            title,
            artist: Some(artist),
            album: Some(album),
            format: show_format,
            duration: duration_width,
        }
    } else if show_artist {
        let extra = variable_area.saturating_sub(MIN_TITLE_WIDTH + MIN_ARTIST_WIDTH);
        let mut title = MIN_TITLE_WIDTH + extra * 60 / 100;
        let artist = MIN_ARTIST_WIDTH + extra * 40 / 100;
        title += variable_area.saturating_sub(title + artist);
        TrackRowLayout {
            title,
            artist: Some(artist),
            album: None,
            format: show_format,
            duration: duration_width,
        }
    } else {
        TrackRowLayout {
            title: variable_area,
            artist: None,
            album: None,
            format: show_format,
            duration: duration_width,
        }
    }
}

pub(super) fn track_row_columns(track: &Track, layout: &TrackRowLayout) -> Vec<String> {
    let mut columns = vec![column_text(track.display_title(), layout.title, false)];
    if let Some(width) = layout.artist {
        columns.push(column_text(
            track.artist.as_deref().unwrap_or("未知歌手"),
            width,
            false,
        ));
    }
    if let Some(width) = layout.album {
        columns.push(column_text(
            track.album.as_deref().unwrap_or("未知专辑"),
            width,
            false,
        ));
    }
    if layout.format {
        columns.push(column_text(
            track.format.as_deref().unwrap_or("?"),
            FORMAT_COLUMN_WIDTH,
            false,
        ));
    }
    let duration = track
        .duration
        .map(fmt_duration)
        .unwrap_or_else(|| "--:--".into());
    columns.push(column_text(&duration, layout.duration, true));
    columns
}
