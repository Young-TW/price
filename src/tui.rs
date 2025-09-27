use ratatui::{
    widgets::{Block, Paragraph, Borders, Gauge},
    Terminal,
    backend::CrosstermBackend,
    style::{Style, Color},
    text::{Span, Line},
    layout::{Layout, Constraint, Direction},
};

use std::collections::HashMap;
use crate::types::Portfolio;

pub fn render_portfolio(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    lines: &[String],
    total_value: f64,
    map: &HashMap<String, f64>,
    target_forex: &str,
    portfolio: &Portfolio,
) {
    terminal.draw(|f| {
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
            Style::default().fg(Color::Green)
        )));

        if let Some(forex_price) = map.get(&format!("USD/{}", target_forex)) {
            let converted_value = total_value * forex_price;
            display_lines.push(Line::from(Span::styled(
                format!("Total assets ({}): ${:.2}", target_forex, converted_value),
                Style::default().fg(Color::Green)
            )));
        }

        let portfolio_block = Block::default()
            .title("Portfolio")
            .borders(Borders::ALL);
        let portfolio_paragraph = Paragraph::new(display_lines).block(portfolio_block);
        f.render_widget(portfolio_paragraph, chunks[0]);

        // Lower part: Asset allocation
        render_asset_allocation(f, chunks[1], portfolio, map, total_value);
    }).unwrap();
}

fn render_asset_allocation(
    f: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    portfolio: &Portfolio,
    map: &HashMap<String, f64>,
    total_value: f64,
) {
    // Calculate asset category values using Portfolio data
    let mut categories = HashMap::new();
    let colors = [Color::Blue, Color::Red, Color::Yellow, Color::Magenta, Color::Cyan, Color::Green];

    // Use Portfolio items directly instead of parsing strings
    let grouped_portfolio = portfolio.group_by_category();
    for (category, items) in grouped_portfolio.iter() {
        let mut category_value = 0.0;

        for item in items {
            let usd_value = if category == "TW-Stock" || category == "TW-ETF" {
                // Convert TWD to USD
                if let Some(price) = map.get(&item.symbol) {
                    let asset_value = price * item.quantity;
                    if let Some(rate) = map.get("USD/TWD") {
                        asset_value / rate
                    } else {
                        0.0
                    }
                } else {
                    0.0
                }
            } else if category == "Forex" {
                // Handle forex conversion - forex assets don't need price lookup
                if item.symbol == "USD" {
                    item.quantity
                } else {
                    let forex_key = format!("USD/{}", item.symbol);
                    if let Some(forex_rate) = map.get(&forex_key) {
                        item.quantity / forex_rate
                    } else {
                        0.0
                    }
                }
            } else {
                // Crypto, US-Stock, US-ETF are already in USD
                if let Some(price) = map.get(&item.symbol) {
                    price * item.quantity
                } else {
                    0.0
                }
            };

            category_value += usd_value;
        }

        if category_value > 0.0 {
            // Merge cash categories
            let final_category = if category == "Forex" {
                "Cash"
            } else {
                category.as_str()
            };
            *categories.entry(final_category).or_insert(0.0) += category_value;
        }
    }

    // Sort categories by value (largest to smallest)
    let mut sorted_categories: Vec<(&str, f64)> = categories.iter()
        .map(|(k, &v)| (*k, v))
        .collect();
    sorted_categories.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    // Create asset allocation display with sorted order
    let mut allocation_lines = Vec::new();
    let mut bars_data = Vec::new();

    for (i, (category, value)) in sorted_categories.iter().enumerate() {
        let percentage = if total_value > 0.0 { value / total_value * 100.0 } else { 0.0 };
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
            Constraint::Length(3), // Single combined bar area
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
