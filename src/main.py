from rich import print

from get import get_price
from config import read_api_keys, read_portfolio

if __name__ == "__main__":
    portfolio = read_portfolio("config/portfolio.toml")
    total_value = 0.0

    # 處理所有資產
    for category in ["crypto", "us-stock", "us-etf", "tw-stock", "tw-etf"]:
        for symbol, amount in portfolio.get(category, {}).items():
            price = get_price(symbol , category)
            print(f"{symbol}: {amount} 股 x ${price}")
            total_value += amount * price

    print(f"\n[bold green]總資產 (USD)：${total_value:.2f}[/bold green]")
