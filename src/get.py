import requests
import json

def api_key():
    with open('apikey.json', 'r') as f:
        return json.load(f)['apikey']

if __name__ == "__main__":
    apikey = api_key()

    symbol = "AMD"
    interval = "5min"
    function = "GLOBAL_QUOTE"
    # replace the "demo" apikey below with your own key from https://www.alphavantage.co/support/#api-key
    url = 'https://www.alphavantage.co/query?function=TIME_SERIES_INTRADAY&symbol=VOO&interval=5min&apikey=' + apikey
    r = requests.get(url)
    data = r.json()

    url = f'https://www.alphavantage.co/query?function={function}&symbol={symbol}&interval={interval}&apikey={apikey}'

    functions = [
        "GLOBAL_QUOTE",
        "TIME_SERIES_INTERADAY",
        "TIME_SERIES_DAILY",
        "TIME_SERIES_DAILY_ADJUSTED",
        "TIME_SERIES_WEEKLY",
        "TIME_SERIES_WEEKLY_ADJUSTED",
        "TIME_SERIES_MONTHLY",
        "TIME_SERIES_MONTHLY_ADJUSTED",
        "CURRENCY_EXCHANGE_RATE",
        "WTI", # West Texas Intermediate
        "BRENT", # Brent Crude
        "NATURAL_GAS", # Natural Gas
    ]

    time_series_intraday = [
        "1min",
        "5min",
        "15min",
        "30min",
        "60min",
    ]

    try:
        r = requests.get(url)
        r.raise_for_status()
        data = r.json()

        if "Error Message" in data or "Note" in data:
            print("Request failed. Please check your input or try again later.")
        else:
            filename = f'json/{function}_{symbol}_{interval}.json'
            with open(filename, 'w') as f:
                json.dump(data, f, indent=4)
            print(f"Data saved to {filename}")
    except requests.exceptions.RequestException as e:
        print(f"An error occurred: {e}")
