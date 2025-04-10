use reqwest::Client;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct YahooChartResponse {
    chart: Chart,
}

#[derive(Deserialize, Debug)]
struct Chart {
    result: Option<Vec<ChartResult>>,
}

#[derive(Deserialize, Debug)]
struct ChartResult {
    indicators: Indicators,
}

#[derive(Deserialize, Debug)]
struct Indicators {
    quote: Vec<Quote>,
}

#[derive(Deserialize, Debug)]
struct Quote {
    close: Vec<Option<f64>>, // 有些時間點可能為 null
}

pub async fn get_price_from_yahoo(symbol: &str) -> Result<f64, String> {
    let url = format!(
        "https://query1.finance.yahoo.com/v8/finance/chart/{}?interval=1d&range=1d",
        symbol
    );

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;

    let response = client.get(&url).send().await.map_err(|e| {
        format!("[Yahoo] 查詢 {} 失敗：{}", symbol, e)
    })?;

    if !response.status().is_success() {
        return Err(format!("[Yahoo] HTTP 錯誤：{}", response.status()));
    }

    let data: YahooChartResponse = response.json().await.map_err(|e| {
        format!("[Yahoo] JSON 格式錯誤：{}", e)
    })?;

    let close = data
        .chart
        .result
        .as_ref()
        .and_then(|r| r.get(0))
        .and_then(|r| r.indicators.quote.get(0))
        .and_then(|q| q.close.last().copied().flatten());

    match close {
        Some(price) => Ok(price),
        None => Err(format!("[Yahoo] 無法取得 {} 收盤價", symbol)),
    }
}
