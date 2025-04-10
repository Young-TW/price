import tomllib

def read_portfolio(filepath):
    with open(filepath, "rb") as f:  # tomllib 只接受 binary 模式
        return tomllib.load(f)
