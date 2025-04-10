from rich import print

from get import get_price
from config import read_api_key, read_portfolio

if __name__ == "__main__":
    api_key = read_api_key("config/api_key.toml")
    portfolio = read_portfolio("config/portfolio.toml")
    total_value = 0.0

    # 處理所有資產
    for category in ["crypto", "us-stock", "us-etf", "tw-stock"]:
        for symbol, amount in portfolio.get(category, {}).items():
            price = get_price(symbol , category, api_key)
            print(f"{symbol}: {amount} 股 x ${price}")
            total_value += amount * price

    # 處理現金（只算 USD，TWD 可換匯處理）
    total_value += portfolio["forex"].get("usd", 0)

    print(f"\n[bold green]總資產 (USD)：${total_value:.2f}[/bold green]")
