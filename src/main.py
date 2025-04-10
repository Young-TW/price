from rich import print

from get import get_current_price
from config import read_api_key, read_portfolio

if __name__ == "__main__":
    api_key = read_api_key("config/api_key.toml")
    portfolio = read_portfolio("config/portfolio.toml")
    # print(portfolio)
    # print(get_current_price(api_key, "amd"))

    total_value = 0.0

    # 處理股票與 ETF（假設都可用 Alpha Vantage）
    for category in ["stock", "ETF"]:
        for symbol, amount in portfolio.get(category, {}).items():
            symbol_clean = symbol.split("-")[-1].upper()  # e.g., nasdaq-amd -> AMD
            price = get_current_price(api_key, symbol_clean)
            print(f"{symbol}: {amount} 股 x ${price}")
            total_value += amount * price

    # 處理現金（只算 USD，TWD 可換匯處理）
    total_value += portfolio["cash"].get("usd", 0)

    print(f"\n[bold green]總資產 (USD)：${total_value:.2f}[/bold green]")
