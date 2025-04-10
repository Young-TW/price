import requests

from config import read_api_keys

def get_rate(currency, target_currency) -> float | None:
    try:
        url = f"https://v6.exchangerate-api.com/v6/{read_api_keys("config/api_key.toml")["exchangerate_api_key"]}/latest/{currency.upper()}"
        response = requests.get(url, timeout=5)
        response.raise_for_status()
        data = response.json()

        if data["result"] != "success":
            print("[ExchangeRate] 回應失敗：", data.get("error-type"))
            return None

        return data["conversion_rates"][target_currency.upper()]

    except Exception as e:
        print(f"[ExchangeRate] 查詢失敗：{e}")
        return None

if __name__ == "__main__":
    rate = get_rate("USD", "TWD")
    if rate:
        print(f"1 USD = {rate:.4f} TWD")
