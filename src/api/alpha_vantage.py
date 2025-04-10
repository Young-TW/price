import requests
from config import read_api_keys

def get_price_from_alpha_vantage(symbol):
    url = f"https://www.alphavantage.co/query?function=GLOBAL_QUOTE&symbol={symbol}&apikey={api_key}"
    data = requests.get(url).json()
    try:
        price = data['Global Quote']['05. price']
        return float(price)
    except (KeyError, ValueError):
        raise Exception(f"無法取得價格(通常為 API 限制 key: {api_key} 或錯誤的 symbol: {symbol})")
