import requests
import json

with open('apikey.json', 'r') as f:
    apikey = json.load(f)['apikey']

# replace the "demo" apikey below with your own key from https://www.alphavantage.co/support/#api-key
url = 'https://www.alphavantage.co/query?function=TIME_SERIES_INTRADAY&symbol=IBM&interval=5min&apikey=' + apikey
r = requests.get(url)
data = r.json()

with open('data.json', 'w') as f:
    json.dump(data, f)
