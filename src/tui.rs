use ratatui::{
    widgets::{Block, Paragraph, Borders, Gauge},
    Terminal,
    backend::CrosstermBackend,
    style::{Style, Color},
    text::{Span, Line},
    layout::{Layout, Constraint, Direction},
};

use std::collections::HashMap;

pub fn render_portfolio(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    lines: &[String],
    total_value: f64,
    map: &HashMap<String, f64>,
    target_forex: &str,
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
        render_asset_allocation(f, chunks[1], lines, total_value);
    }).unwrap();
}

fn render_asset_allocation(
    f: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    lines: &[String],
    total_value: f64,
) {
    // Calculate asset category values
    let mut categories = HashMap::new();
    let colors = [Color::Blue, Color::Red, Color::Yellow, Color::Magenta, Color::Cyan, Color::Green];

    // Parse each line to extract actual USD values
    let mut i = 0;
    while i < lines.len() {
        let line = &lines[i];

        // Skip converted lines and warnings
        if line.contains("Converted") || line.contains("Warning") {
            i += 1;
            continue;
        }

        // Parse asset lines that contain "=" (actual asset holdings)
        if line.contains("=") && (line.contains("$") || line.contains("NT$")) {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 2 {
                let symbol = parts[0].trim();

                // Determine category based on symbol and line content
                let category = if line.contains("NT$") {
                    "TW Assets"  // Taiwan stocks/ETFs
                } else if symbol == "USD" {
                    "Cash (USD)"       // USD cash
                } else if symbol == "TWD" {
                    "Cash (TWD)"       // TWD cash
                } else if symbol.contains("USDT") || symbol.contains("ETH") || symbol.contains("SOL") ||
                         symbol.contains("BTC") || symbol.contains("AVAX") || symbol.contains("ADA") ||
                         symbol.contains("WBETH") {
                    "Crypto"     // Cryptocurrencies
                } else {
                    "US Assets"  // US stocks/ETFs
                };

                let usd_value = if line.contains("NT$") {
                    // For Taiwan assets, look for the converted USD value in the next line
                    if i + 1 < lines.len() && lines[i + 1].contains("Converted to USD") {
                        let converted_line = &lines[i + 1];
                        if let Some(equal_pos) = converted_line.rfind('=') {
                            let value_part = &converted_line[equal_pos + 1..].trim();
                            if let Some(usd_value_str) = value_part.strip_prefix("$") {
                                usd_value_str.parse::<f64>().unwrap_or(0.0)
                            } else { 0.0 }
                        } else { 0.0 }
                    } else { 0.0 }
                } else if symbol == "TWD" {
                    // For TWD, look for the converted USD value in the next line
                    if i + 1 < lines.len() && lines[i + 1].contains("Converted to USD") {
                        let converted_line = &lines[i + 1];
                        if let Some(equal_pos) = converted_line.rfind('=') {
                            let value_part = &converted_line[equal_pos + 1..].trim();
                            if let Some(usd_value_str) = value_part.strip_prefix("$") {
                                usd_value_str.parse::<f64>().unwrap_or(0.0)
                            } else { 0.0 }
                        } else { 0.0 }
                    } else { 0.0 }
                } else {
                    // For USD assets, extract the USD value directly
                    if let Some(equal_pos) = line.rfind('=') {
                        let value_part = &line[equal_pos + 1..].trim();
                        if let Some(usd_value_str) = value_part.strip_prefix("$") {
                            usd_value_str.parse::<f64>().unwrap_or(0.0)
                        } else { 0.0 }
                    } else { 0.0 }
                };

                if usd_value > 0.0 {
                    // Merge TWD and USD cash into single "Cash" category
                    let final_category = if category == "Cash (USD)" || category == "Cash (TWD)" {
                        "Cash"
                    } else {
                        category
                    };
                    *categories.entry(final_category).or_insert(0.0) += usd_value;
                }
            }
        }
        i += 1;
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
