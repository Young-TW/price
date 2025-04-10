import tomllib

def read_portfolio(file_path):
    with open(file_path, "rb") as f:  # tomllib 只接受 binary 模式
        return tomllib.load(f)

def read_api_key(file_path):
    with open(file_path, "rb") as f:
        return tomllib.load(f)["api_key"]
