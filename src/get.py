from api.pyth import get_price_from_pyth
from api.alpha_vantage import get_price_from_alpha_vantage

def get_price(symbol: str, category: str, api_key: str) -> float:
    # 先判斷是什麼類型（crypto / stock / etf / 台股）
    # 然後依照優先順序 fallback
    if category == "crypto" or category == "us-stock" or category == "us-etf":
        pyth_price = get_price_from_pyth(symbol)
        if pyth_price is not None:
            return pyth_price

        return  get_price_from_alpha_vantage(api_key, symbol)
    elif category == "tw-stock":
        return 0.0 # not done yet
        # return get_price_from_yahoo(symbol)

    raise ValueError(f"未知的資產類別：{symbol}")
