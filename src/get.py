import requests
import json

def get_stock_current_price(apikey, symbol):
    url = f"https://www.alphavantage.co/query?function=GLOBAL_QUOTE&symbol={symbol}&apikey={apikey}"
    data = requests.get(url).json()
    try:
        price = data['Global Quote']['05. price']
        return float(price)
    except (KeyError, ValueError):
        return None  # or raise Exception("無法取得價格")

def get_crypto_current_price():
    # use pyth network
    # ETH/USD 0xff61491a931112ddf1bd8147cd1b641375f79f5825126d665480874634fd0ace
    # url = f"
