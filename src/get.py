from api.pyth import get_price_from_pyth
from api.alpha_vantage import get_price_from_alpha_vantage
from api.binance import get_price_from_binance
from api.redstone import get_price_from_redstone
from api.yahoo import get_price_from_yahoo

from api.exchangerate import get_rate

def get_price(symbol: str, category: str) -> float:
    # 先判斷是什麼類型（crypto / stock / etf / 台股）
    # 然後依照優先順序 fallback
    if category == "crypto":
        pyth_price = get_price_from_pyth(symbol)
        if pyth_price is not None:
            return pyth_price

        redstone_price = get_price_from_redstone(symbol)
        if redstone_price is not None:
            return redstone_price

        binance_price = get_price_from_binance(symbol)
        if binance_price is not None:
            return binance_price

        return get_price_from_alpha_vantage(symbol)

    if category == "us-stock" or category == "us-etf":
        pyth_price = get_price_from_pyth(symbol)
        if pyth_price is not None:
            return pyth_price

        return get_price_from_yahoo(symbol)

    elif category == "tw-stock" or category == "tw-etf":
        tw_price = get_price_from_yahoo(symbol)
        us_price = tw_price * get_rate("TWD", "USD")
        return us_price

    raise ValueError(f"未知的資產類別：{symbol}")
