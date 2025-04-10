import yfinance as yf

def get_price_from_yahoo(symbol: str) -> float | None:
    try:
        stock = yf.Ticker(symbol)
        data = stock.history(period="1d")
        return data["Close"].iloc[-1]  # 最新收盤價
    except Exception as e:
        print(f"[Yahoo] 查詢 {symbol} 失敗：{e}")
        return None

if __name__ == "__main__":
    print(get_price_from_yahoo("AAPL"))     # 美股
    print(get_price_from_yahoo("2330.TW"))  # 台積電
    print(get_price_from_yahoo("0050.TW"))  # 台灣 50 ETF
