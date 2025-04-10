import requests

def get_price_from_redstone(symbol: str) -> float | None:
    symbol = symbol.upper()
    try:
        url = f"https://api.redstone.finance/prices/?symbol={symbol}&provider=redstone&limit=1"
        response = requests.get(url, timeout=5)
        response.raise_for_status()
        data = response.json()

        if isinstance(data, list) and len(data) > 0:
            return data[0]["value"]

        print(f"[RedStone] 沒有取得 {symbol} 的價格資料")
        return None

    except Exception as e:
        print(f"[RedStone] 查詢 {symbol} 價格失敗：{e}")
        return None

if __name__ == "__main__":
    from rich import print
    print(get_price_from_redstone("eth"))
