//! Terminal UI rendering: the live portfolio/allocation screen and the
//! historical value and allocation-ratio charts.

use ratatui::{
    Terminal,
    backend::Backend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    symbols,
    text::{Line, Span},
    widgets::{Axis, Block, Borders, Chart, Dataset, GraphType, Paragraph},
};

use crate::history::compute_category_values;
use crate::types::{Portfolio, PortfolioSnapshot};
use chrono::{TimeZone, Utc};
use std::collections::HashMap;

/// Which screen the TUI is currently showing.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    /// The live portfolio and asset-allocation screen.
    Live,
    /// The historical value and allocation-ratio screen.
    History,
}

impl ViewMode {
    /// Switch to the other screen. Used by the single-key page toggle so the
    /// live page (the main page) and the history page swap back and forth.
    pub fn toggle(self) -> ViewMode {
        match self {
            ViewMode::Live => ViewMode::History,
            ViewMode::History => ViewMode::Live,
        }
    }
}

/// Stable palette shared by the allocation view and the history charts so a
/// category keeps the same colour across screens.
const PALETTE: [Color; 6] = [
    Color::Blue,
    Color::Red,
    Color::Yellow,
    Color::Magenta,
    Color::Cyan,
    Color::Green,
];

/// Render one frame to `terminal` for the current `view_mode`.
///
/// In [`ViewMode::History`] it draws the history charts from `history`.
/// Otherwise it draws the portfolio lines plus the total value in USD (and, when
/// a `USD/<target_forex>` rate is present in `map`, the total converted to the
/// target currency), with the asset-allocation panel below.
pub fn render_portfolio<B: Backend>(
    terminal: &mut Terminal<B>,
    lines: &[String],
    total_value: f64,
    map: &HashMap<String, f64>,
    target_forex: &str,
    portfolio: &Portfolio,
    history: &[PortfolioSnapshot],
    view_mode: ViewMode,
) {
    if view_mode == ViewMode::History {
        terminal
            .draw(|f| render_history(f, f.area(), history))
            .unwrap();
        return;
    }

    terminal
        .draw(|f| {
            let area = f.area();

            // Split screen into upper (portfolio) and lower (asset allocation)
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(70), // Upper 70%
                    Constraint::Percentage(30), // Lower 30%
                ])
                .split(area);

            // Upper part: Portfolio display
            let mut display_lines: Vec<Line> = lines
                .iter()
                .map(|line| Line::from(Span::raw(line.clone())))
                .collect();

            display_lines.push(Line::from(Span::styled(
                format!("Total assets (USD): ${:.2}", total_value),
                Style::default().fg(Color::Green),
            )));

            if let Some(forex_price) = map.get(&format!("USD/{}", target_forex)) {
                let converted_value = total_value * forex_price;
                display_lines.push(Line::from(Span::styled(
                    format!("Total assets ({}): ${:.2}", target_forex, converted_value),
                    Style::default().fg(Color::Green),
                )));
            }

            let portfolio_block = Block::default()
                .title("Portfolio (Tab: history  e: export csv  q: quit)")
                .borders(Borders::ALL);
            let portfolio_paragraph = Paragraph::new(display_lines).block(portfolio_block);
            f.render_widget(portfolio_paragraph, chunks[0]);

            // Lower part: Asset allocation
            render_asset_allocation(f, chunks[1], portfolio, map, total_value);
        })
        .unwrap();
}

fn render_asset_allocation(
    f: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    portfolio: &Portfolio,
    map: &HashMap<String, f64>,
    total_value: f64,
) {
    // Calculate asset category values using the shared helper.
    let colors = PALETTE;
    let (categories, _total) = compute_category_values(portfolio, map);

    // Sort categories by value (largest to smallest), dropping any non-finite
    // values (NaN or Infinity) so the sort never receives a None from partial_cmp.
    let mut sorted_categories: Vec<(&str, f64)> =
        categories.iter().map(|(k, &v)| (k.as_str(), v)).collect();
    sorted_categories.retain(|(_, v)| v.is_finite());
    sorted_categories.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // Create asset allocation display with sorted order
    let mut allocation_lines = Vec::new();
    let mut bars_data = Vec::new();

    for (i, (category, value)) in sorted_categories.iter().enumerate() {
        let percentage = if total_value > 0.0 {
            value / total_value * 100.0
        } else {
            0.0
        };
        let color = colors[i % colors.len()];

        allocation_lines.push(Line::from(vec![
            Span::styled("█", Style::default().fg(color)),
            Span::raw(format!(" {}: ${:.0} ({:.1}%)", category, value, percentage)),
        ]));

        bars_data.push((percentage, color));
    }

    // Split allocation area into text and single bar
    let allocation_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(allocation_lines.len() as u16 + 2), // Text area
            Constraint::Length(3),                                 // Single combined bar area
        ])
        .split(area);

    // Render allocation text
    let allocation_block = Block::default()
        .title("Asset Allocation")
        .borders(Borders::ALL);
    let allocation_paragraph = Paragraph::new(allocation_lines).block(allocation_block);
    f.render_widget(allocation_paragraph, allocation_chunks[0]);

    // Render single combined bar
    if !bars_data.is_empty() {
        let bar_area = ratatui::layout::Rect {
            x: allocation_chunks[1].x + 1,
            y: allocation_chunks[1].y + 1,
            width: allocation_chunks[1].width.saturating_sub(2),
            height: 1,
        };

        // Create combined bar using colored spans
        let mut bar_spans = Vec::new();
        let total_width = bar_area.width as f64;

        for (percentage, color) in bars_data {
            let segment_width = ((percentage / 100.0) * total_width) as usize;
            if segment_width > 0 {
                let segment = "█".repeat(segment_width);
                bar_spans.push(Span::styled(segment, Style::default().fg(color)));
            }
        }

        let combined_bar = Line::from(bar_spans);
        let bar_paragraph = Paragraph::new(vec![combined_bar]);
        f.render_widget(bar_paragraph, bar_area);
    }
}

/// History screen: total portfolio value over time (top) and per-category
/// allocation ratio over time (bottom).
fn render_history(
    f: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    history: &[PortfolioSnapshot],
) {
    if history.len() < 2 {
        let msg = Paragraph::new(
            "Collecting history... (need at least 2 data points)\nPress Tab for live view, 'q' to quit",
        )
        .block(Block::default().title("History").borders(Borders::ALL));
        f.render_widget(msg, area);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);

    render_total_value_chart(f, chunks[0], history);
    render_ratio_chart(f, chunks[1], history);
}

fn date_labels(x_min: f64, x_max: f64) -> Vec<Span<'static>> {
    let fmt = |ts: f64| {
        Utc.timestamp_opt(ts as i64, 0)
            .single()
            .map(|d| d.format("%m/%d").to_string())
            .unwrap_or_default()
    };
    let mid = (x_min + x_max) / 2.0;
    vec![
        Span::raw(fmt(x_min)),
        Span::raw(fmt(mid)),
        Span::raw(fmt(x_max)),
    ]
}

fn render_total_value_chart(
    f: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    history: &[PortfolioSnapshot],
) {
    let data: Vec<(f64, f64)> = history
        .iter()
        .map(|s| (s.timestamp as f64, s.total_value_usd))
        .collect();

    let x_min = data.first().map(|p| p.0).unwrap_or(0.0);
    let x_max = data.last().map(|p| p.0).unwrap_or(1.0);
    let y_max = data.iter().map(|(_, y)| *y).fold(0.0_f64, f64::max);
    let y_hi = if y_max > 0.0 { y_max * 1.1 } else { 1.0 };

    let datasets = vec![
        Dataset::default()
            .name("Total (USD)")
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Green))
            .data(&data),
    ];

    let chart = Chart::new(datasets)
        .block(
            Block::default()
                .title("Total Value History (USD)  Tab: live  q: quit")
                .borders(Borders::ALL),
        )
        .x_axis(
            Axis::default()
                .bounds([x_min, x_max])
                .labels(date_labels(x_min, x_max)),
        )
        .y_axis(Axis::default().bounds([0.0, y_hi]).labels(vec![
            Span::raw("$0".to_string()),
            Span::raw(format!("${:.0}", y_hi / 2.0)),
            Span::raw(format!("${:.0}", y_hi)),
        ]));

    f.render_widget(chart, area);
}

fn render_ratio_chart(
    f: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    history: &[PortfolioSnapshot],
) {
    // Stable, alphabetically-ordered union of category names.
    let mut cats: Vec<String> = history
        .iter()
        .flat_map(|s| s.category_values.keys().cloned())
        .collect();
    cats.sort();
    cats.dedup();

    let x_min = history.first().map(|s| s.timestamp as f64).unwrap_or(0.0);
    let x_max = history.last().map(|s| s.timestamp as f64).unwrap_or(1.0);

    // Build and keep the per-category point series alive for the datasets.
    let series: Vec<(String, Vec<(f64, f64)>)> = cats
        .iter()
        .map(|cat| {
            let pts: Vec<(f64, f64)> = history
                .iter()
                .filter(|s| s.total_value_usd > 0.0)
                .map(|s| {
                    let v = s.category_values.get(cat).copied().unwrap_or(0.0);
                    (s.timestamp as f64, v / s.total_value_usd * 100.0)
                })
                .collect();
            (cat.clone(), pts)
        })
        .collect();

    let datasets: Vec<Dataset> = series
        .iter()
        .enumerate()
        .map(|(i, (name, data))| {
            Dataset::default()
                .name(name.clone())
                .marker(symbols::Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(PALETTE[i % PALETTE.len()]))
                .data(data)
        })
        .collect();

    let chart = Chart::new(datasets)
        .block(
            Block::default()
                .title("Allocation Ratio History (%)")
                .borders(Borders::ALL),
        )
        .x_axis(
            Axis::default()
                .bounds([x_min, x_max])
                .labels(date_labels(x_min, x_max)),
        )
        .y_axis(Axis::default().bounds([0.0, 100.0]).labels(vec![
            Span::raw("0%"),
            Span::raw("50%"),
            Span::raw("100%"),
        ]));

    f.render_widget(chart, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn item(symbol: &str, category: &str, quantity: f64) -> crate::types::PortfolioItem {
        crate::types::PortfolioItem {
            symbol: symbol.to_string(),
            category: category.to_string(),
            quantity,
        }
    }

    /// Verifies that render_asset_allocation does not panic when the price map
    /// contains NaN values (price * quantity produces NaN for that asset).
    #[test]
    fn render_asset_allocation_nan_price_does_not_panic() {
        let portfolio = Portfolio(vec![item("BTC", "Crypto", 1.0), item("ETH", "Crypto", 2.0)]);

        let mut map = HashMap::new();
        map.insert("BTC".to_string(), f64::NAN);
        map.insert("ETH".to_string(), 2000.0);

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                render_asset_allocation(f, f.area(), &portfolio, &map, 2000.0);
            })
            .unwrap();
    }

    /// Verifies that non-finite category values (NaN, Infinity) are stripped
    /// before sorting so the sort never receives a None from partial_cmp.
    #[test]
    fn render_asset_allocation_infinity_does_not_panic() {
        let portfolio = Portfolio(vec![item("AAPL", "US-Stock", 10.0)]);

        let mut map = HashMap::new();
        map.insert("AAPL".to_string(), f64::INFINITY);

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                render_asset_allocation(f, f.area(), &portfolio, &map, 0.0);
            })
            .unwrap();
    }
}
