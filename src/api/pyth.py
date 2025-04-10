import requests

BASE_URL = "https://hermes.pyth.network"
feed_id_cache = {}

def call_api(endpoint, params=None):
    """通用函式：呼叫 Hermes API 並回傳解析後的 JSON 資料（若失敗則回傳 None）。"""
    url = BASE_URL + endpoint
    try:
        response = requests.get(url, params=params, timeout=5)
    except requests.RequestException as e:
        print(f"[pyth]: Request error for {url}: {e}")
        return None
    # 檢查 HTTP 狀態碼是否成功
    if response.status_code != 200:
        # 嘗試解析 JSON 錯誤訊息
        err_message = ""
        try:
            err_data = response.json()
            if isinstance(err_data, dict):
                err_message = err_data.get("error") or err_data.get("message") or str(err_data)
            else:
                err_message = str(err_data)
        except ValueError:
            # 非 JSON 格式錯誤
            err_message = response.text[:100]  # 取前100字元以避免過長
        print(f"[pyth]: HTTP {response.status_code} error for {url}: {err_message}")
        return None
    # 嘗試解析回傳的 JSON 內容
    try:
        return response.json()
    except ValueError:
        text_snippet = response.text[:100]
        print(f"[pyth]: Non-JSON response from {url}: {text_snippet}")
        return None

def get_feed_id(symbol):
    """
    取得指定資產符號對應的 Pyth feed ID，並將結果快取。
    支援直接輸入如 "ETH"（預設會查詢 ETH/USD）或 "ETH/USD" 格式。
    """
    if not symbol or not isinstance(symbol, str):
        print("[pyth]: Invalid symbol:", symbol)
        return None
    symbol = symbol.strip().upper()
    # 若未帶斜線，預設查詢對 USD 的價格
    query_symbol = symbol if '/' in symbol else symbol + '/USD'
    # 檢查快取
    if symbol in feed_id_cache:
        return feed_id_cache[symbol]
    # 呼叫 Hermes API 查詢 feed 列表
    params = {"query": query_symbol}
    feeds = call_api("/v2/price_feeds", params)
    if feeds is None:
        return None  # 錯誤情況已在 call_api 輸出
    if not isinstance(feeds, list):
        print("[pyth]: Unexpected response format for price_feeds query.")
        return None
    found_id = None
    for feed in feeds:
        if not isinstance(feed, dict):
            continue
        f_id = feed.get("id")
        # 取得 feed 的符號名稱（可能在頂層或 attributes 中）
        f_symbol = None
        if "symbol" in feed:
            f_symbol = str(feed["symbol"]).upper()
        elif "attributes" in feed and isinstance(feed["attributes"], dict):
            f_symbol = str(feed["attributes"].get("symbol", "")).upper()
        if f_symbol == query_symbol:
            found_id = f_id
            break
    if not found_id:
        print(f"[pyth]: Price feed for symbol '{symbol}' not found.")
        return None
    # 保存快取，加速下次查詢
    feed_id_cache[symbol] = found_id
    if query_symbol != symbol:
        feed_id_cache[query_symbol] = found_id  # 亦快取帶 /USD 的鍵
    return found_id

def get_price_from_pyth(symbol):
    """
    查詢指定資產當前價格和信賴區間等資訊。
    回傳包含 price、confidence 和 publish_time 的字典。
    """
    feed_id = get_feed_id(symbol)
    if not feed_id:
        return None
    # 呼叫 Hermes API 查詢最新價格更新（使用 parsed 參數取得解析後結果）
    params = [("ids[]", feed_id), ("parsed", "true")]
    data = call_api("/v2/updates/price/latest", params)
    if data is None:
        return None
    # 提取解析後的價格資料
    parsed_list = data.get("parsed") if isinstance(data, dict) else data
    if not parsed_list or len(parsed_list) == 0:
        print("[pyth]: No parsed price data available for feed:", feed_id)
        return None
    feed_data = parsed_list[0]
    if not isinstance(feed_data, dict):
        print("[pyth]: Unexpected parsed data format.")
        return None
    price_info = feed_data.get("price")
    if not price_info or not isinstance(price_info, dict):
        print("[pyth]: Missing price information in update.")
        return None
    price_int = price_info.get("price")
    conf_int = price_info.get("conf")
    expo = price_info.get("expo", 0)
    publish_time = price_info.get("publish_time")
    # 確認數值型別並計算實際價格與區間
    try:
        expo_val = int(expo)
        price_val = int(price_int)
        conf_val = int(conf_int) if conf_int is not None else None
    except Exception as e:
        print("[pyth]: Invalid price data format:", e)
        return None
    if expo_val < 0:
        scale = 10 ** -expo_val
        actual_price = price_val / scale
        actual_conf = conf_val / scale if conf_val is not None else None
    else:
        scale = 10 ** expo_val
        actual_price = price_val * scale
        actual_conf = conf_val * scale if conf_val is not None else None
    return {
        "price": actual_price,
        "confidence": actual_conf,
        "publish_time": publish_time,
        "feed_id": feed_id
    }

# 範例使用：
if __name__ == "__main__":
    symbols = ["ETH", "SOL", "BTC"]
    for sym in symbols:
        data = get_price_from_pyth(sym)
        if data:
            conf_str = f"±{data['confidence']}" if data.get("confidence") is not None else ""
            print(f"[pyth]: {sym}/USD price: {data['price']} {conf_str} (publish time: {data['publish_time']})")
