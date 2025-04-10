from api.pyth import get_price_from_pyth
from api.alpha_vantage import get_price_from_alpha_vantage
from api.binance import get_price_from_binance
from api.redstone import get_price_from_redstone
from api.yahoo import get_price_from_yahoo

def get_price(symbol: str, category: str, api_key: str) -> float:
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

        return get_price_from_alpha_vantage(api_key, symbol)

    if category == "us-stock" or category == "us-etf":
        pyth_price = get_price_from_pyth(symbol)
        if pyth_price is not None:
            return pyth_price

        """
        alpha_vantage_price = get_price_from_alpha_vantage(api_key, symbol)
        if alpha_vantage_price is not None:
            return alpha_vantage_price
        """

        return get_price_from_yahoo(symbol)

    elif category == "tw-stock":
        return get_price_from_yahoo(symbol)

    raise ValueError(f"未知的資產類別：{symbol}")
