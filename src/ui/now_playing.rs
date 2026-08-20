//! 当前播放状态、模式与进度条。

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Paragraph};

use crate::app::App;
use crate::theme::Theme;

use super::library::{LIST_ICON_WIDTH, playback_action_indicator};
use super::text::{ascii_progress_bar, fmt_duration, now_playing_text};

pub(super) fn draw_now_playing(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(theme.border))
        .title(Span::styled(
            " ♫ 正在播放 ",
            Style::new().fg(theme.primary).bold(),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(inner);
    let top = Layout::horizontal([Constraint::Min(10), Constraint::Length(38)]).split(rows[0]);
    let now = match app.current_track() {
        Some(track) => {
            let (icon, style) = playback_action_indicator(app.player.state(), theme);
            let budget = usize::from(top[0].width).saturating_sub(LIST_ICON_WIDTH);
            let (title, artist) =
                now_playing_text(track.display_title(), track.artist.as_deref(), budget);
            Line::from(vec![
                Span::styled(icon, style),
                Span::styled(title, Style::new().fg(theme.primary).bold()),
                Span::styled(artist, Style::new().fg(theme.muted)),
            ])
        }
        None => Line::from(Span::styled("■ 未在播放", Style::new().fg(theme.muted))),
    };
    frame.render_widget(Paragraph::new(now), top[0]);

    let volume = if app.player.is_muted() {
        "静音".to_owned()
    } else {
        format!("{}%", app.player.volume())
    };
    frame.render_widget(
        Paragraph::new(format!(
            "{} · {} · 队列 {}",
            app.playback_mode().label(),
            volume,
            app.queue.len()
        ))
        .alignment(Alignment::Right)
        .style(Style::new().fg(theme.muted)),
        top[1],
    );

    let position = app.player.position();
    let duration = app
        .player
        .duration()
        .or_else(|| app.current_track()?.duration);
    let ratio = duration
        .filter(|duration| !duration.is_zero())
        .map(|duration| position.as_secs_f64() / duration.as_secs_f64())
        .unwrap_or_default()
        .clamp(0.0, 1.0);
    let position_label = fmt_duration(position);
    let duration_label = duration.map(fmt_duration).unwrap_or_else(|| "--:--".into());
    let bar_width =
        (rows[1].width as usize).saturating_sub(position_label.len() + duration_label.len() + 3);
    let bar = ascii_progress_bar(ratio, bar_width);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!(" {position_label} "), Style::new().fg(theme.muted)),
            Span::styled(bar, Style::new().fg(theme.primary)),
            Span::styled(format!(" {duration_label}"), Style::new().fg(theme.muted)),
        ])),
        rows[1],
    );
}
