from rich import print

# from get import get_current_price, api_key
from config import read_portfolio

if __name__ == "__main__":

    portfolio = read_portfolio("config/portfolio.toml")
    print(portfolio)
    # print(get_current_price(api_key("config/apikey.json"), "amd"))
