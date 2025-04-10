from rich import print

from get import get_current_price
import config

if __name__ == "__main__":

    api_key = config.read_api_key("config/api_key.toml")
    portfolio = config.read_portfolio("config/portfolio.toml")
    # print(portfolio)
    print(get_current_price(api_key, "amd"))
