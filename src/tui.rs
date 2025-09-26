use ratatui::{
    widgets::{Block, Paragraph, Borders},
    Terminal,
    backend::CrosstermBackend,
    style::{Style, Color},
    text::{Span, Line},
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

        // Build display text with colored totals
        let mut display_lines: Vec<Line> = lines
            .iter()
            .map(|line| Line::from(Span::raw(line.clone())))
            .collect();

        // Add colored total lines
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

        let block = Block::default()
            .title("Portfolio")
            .borders(Borders::ALL);
        let paragraph = Paragraph::new(display_lines).block(block);

        f.render_widget(paragraph, area);
    }).unwrap();
}
