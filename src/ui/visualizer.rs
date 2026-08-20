//! 频谱区域高度、重采样和绘制。

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::App;
use crate::theme::Theme;

const BASE_LAYOUT_HEIGHT: u16 = 11;
const MAX_VISUALIZER_HEIGHT: u16 = 5;
const MIN_VISUALIZER_HEIGHT: u16 = 2;

pub(super) fn visualizer_height(terminal_height: u16, enabled: bool) -> u16 {
    if !enabled {
        return 0;
    }
    let available = terminal_height.saturating_sub(BASE_LAYOUT_HEIGHT);
    if available < MIN_VISUALIZER_HEIGHT {
        0
    } else {
        available.min(MAX_VISUALIZER_HEIGHT)
    }
}

pub(super) fn draw_visualizer(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::new().fg(theme.border));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.is_empty() {
        return;
    }

    let spectrum = app.spectrum_bars();
    let bar_count = spectrum.len().min(usize::from(inner.width).div_ceil(2));
    let bars = resample_spectrum(spectrum, bar_count);
    let plot_width = bars.len().saturating_mul(2).saturating_sub(1);
    let left_padding = (usize::from(inner.width).saturating_sub(plot_width)) / 2;
    let right_padding = usize::from(inner.width)
        .saturating_sub(left_padding)
        .saturating_sub(plot_width);
    for row in 0..inner.height {
        let remaining_rows = f32::from(inner.height - row - 1);
        let mut spans = Vec::with_capacity(bars.len().saturating_mul(2) + 2);
        spans.push(Span::raw(" ".repeat(left_padding)));
        for (index, value) in bars.iter().enumerate() {
            let cell_fill = (value * f32::from(inner.height) - remaining_rows).clamp(0.0, 1.0);
            spans.push(Span::styled(
                spectrum_block(cell_fill).to_string(),
                Style::new().fg(frequency_color(index, bars.len(), theme)),
            ));
            if index + 1 < bars.len() {
                spans.push(Span::raw(" "));
            }
        }
        spans.push(Span::raw(" ".repeat(right_padding)));
        let row_area = Rect::new(inner.x, inner.y + row, inner.width, 1);
        frame.render_widget(Paragraph::new(Line::from(spans)), row_area);
    }
}

pub(super) fn resample_spectrum(values: &[f32], width: usize) -> Vec<f32> {
    if values.is_empty() || width == 0 {
        return Vec::new();
    }
    (0..width)
        .map(|column| {
            let start = column * values.len() / width;
            let end = ((column + 1) * values.len()).div_ceil(width);
            values[start..end.max(start + 1).min(values.len())]
                .iter()
                .copied()
                .filter(|value| value.is_finite())
                .fold(0.0_f32, f32::max)
                .clamp(0.0, 1.0)
        })
        .collect()
}

pub(super) fn spectrum_block(fill: f32) -> char {
    match (fill.clamp(0.0, 1.0) * 8.0).ceil() as u8 {
        0 => ' ',
        1 => '▁',
        2 => '▂',
        3 => '▃',
        4 => '▄',
        5 => '▅',
        6 => '▆',
        7 => '▇',
        _ => '█',
    }
}

pub(super) fn frequency_color(index: usize, count: usize, theme: &Theme) -> Color {
    let ratio = if count <= 1 {
        0.0
    } else {
        index as f32 / (count - 1) as f32
    };
    let (Color::Rgb(low_r, low_g, low_b), Color::Rgb(high_r, high_g, high_b)) =
        (theme.spectrum_low, theme.spectrum_high)
    else {
        return theme.spectrum_low;
    };
    let mix =
        |low: u8, high: u8| (f32::from(low) + (f32::from(high) - f32::from(low)) * ratio) as u8;
    Color::Rgb(mix(low_r, high_r), mix(low_g, high_g), mix(low_b, high_b))
}
