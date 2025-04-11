use colored::*;
use crossterm::{cursor, execute, terminal};
use futures::stream::{FuturesUnordered, StreamExt};
use std::{io::stdout, thread, time};

use crate::config::read_portfolio;
use crate::get::get_price;

pub async fn stream(cycle: u64) {
    let mut stdout = stdout();

    // 初始化終端機
    execute!(stdout, terminal::Clear(terminal::ClearType::All)).unwrap();

    loop {
        let portfolio = read_portfolio("config/portfolio.toml").expect("無法讀取資產組合檔案");
        let mut total_value = 0.0;
        let mut tasks = FuturesUnordered::new();

        for category in ["crypto", "us-stock", "us-etf", "tw-stock", "tw-etf"] {
            if let Some(items) = portfolio.get(category) {
                for (symbol, amount) in items {
                    let symbol = symbol.clone();
                    let category = category.to_string();
                    let amount = *amount;

                    tasks.push(async move {
                        match get_price(&symbol, &category).await {
                            Ok(price) => Some((symbol, amount, price, category)),
                            Err(_) => None,
                        }
                    });
                }
            }
        }

        // 清除畫面並將游標移到左上角
        execute!(
            stdout,
            terminal::Clear(terminal::ClearType::All),
            cursor::MoveTo(0, 0)
        )
        .unwrap();

        while let Some(result) = tasks.next().await {
            if let Some((symbol, amount, price, _category)) = result {
                println!("{}: {} 股 x ${:.2}", symbol, amount, price);
                total_value += amount * price;
            } else {
                println!("{}", "查詢失敗".red());
            }
        }

        println!(
            "\n{}",
            format!("總資產 (USD)：${:.2}", total_value).bold().green()
        );

        thread::sleep(time::Duration::from_millis(cycle * 1000));
    }
}
