//! 同步歌词：宽终端在曲库右侧显示滚动面板，窄终端只在播放区边框上显示当前行。

use std::ops::Range;

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Paragraph};

use crate::app::App;
use crate::lyrics::Lyrics;
use crate::theme::Theme;

use super::text::truncate_display;

/// 曲库区域至少这么宽才分出歌词面板，保证曲库仍能显示标题、歌手和时长。
const MIN_SPLIT_WIDTH: u16 = 96;
const MAX_PANE_WIDTH: u16 = 48;

/// 歌词面板的宽度；不显示面板时为 0。
pub(super) fn lyrics_pane_width(area_width: u16, has_lyrics: bool) -> u16 {
    if !has_lyrics || area_width < MIN_SPLIT_WIDTH {
        return 0;
    }
    (area_width * 2 / 5).min(MAX_PANE_WIDTH)
}

/// 当前行及与它同一时刻的行（如译文），一起高亮。
fn current_range(lyrics: &Lyrics, position: std::time::Duration) -> Option<Range<usize>> {
    let last = lyrics.current_index(position)?;
    let time = lyrics.lines()[last].time;
    let first = lyrics.lines()[..last]
        .iter()
        .rposition(|line| line.time != time)
        .map_or(0, |index| index + 1);
    Some(first..last + 1)
}

pub(super) fn draw_lyrics(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(theme.border))
        .title(Span::styled(
            " 歌词 ",
            Style::new().fg(theme.primary).bold(),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Some(lyrics) = app.lyrics() else {
        return;
    };
    if inner.is_empty() {
        return;
    }

    let lines = lyrics.lines();
    let height = usize::from(inner.height);
    let current = current_range(lyrics, app.player.position());
    // 当前行尽量停在面板中间；开头和结尾不留多余空白。
    let anchor = current.as_ref().map_or(0, |range| range.start);
    let start = anchor
        .saturating_sub(height.saturating_sub(1) / 2)
        .min(lines.len().saturating_sub(height));
    let width = usize::from(inner.width);
    let text: Vec<Line> = lines
        .iter()
        .enumerate()
        .skip(start)
        .take(height)
        .map(|(index, line)| {
            let style = if current.as_ref().is_some_and(|range| range.contains(&index)) {
                Style::new().fg(theme.primary).bold()
            } else {
                Style::new().fg(theme.muted)
            };
            Line::styled(truncate_display(&line.text, width), style).centered()
        })
        .collect();
    frame.render_widget(Paragraph::new(text), inner);
}

/// 窄终端下放在播放区底边的当前行；没有正在唱的非空行时为 None。
pub(super) fn inline_lyric(app: &App, max_width: usize) -> Option<String> {
    let lyrics = app.lyrics()?;
    let range = current_range(lyrics, app.player.position())?;
    let text = lyrics.lines()[range]
        .iter()
        .map(|line| line.text.as_str())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" / ");
    (!text.is_empty()).then(|| format!(" {} ", truncate_display(&text, max_width)))
}
