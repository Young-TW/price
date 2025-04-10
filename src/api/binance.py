import requests

def get_price_from_binance(symbol: str) -> float | None:
    pair = symbol.upper() + "USDT"
    url = f"https://api.binance.com/api/v3/ticker/price?symbol={pair}"
    try:
        response = requests.get(url, timeout=10)
        response.raise_for_status()
        data = response.json()
        return float(data["price"])
    except requests.exceptions.RequestException as e:
        print(f"[Binance] 查詢 {symbol} 價格失敗：{e}")
        return None

if __name__ == "__main__":
    from rich import print
    print(get_price_from_binance("eth"))
