import matplotlib.pyplot as plt
from matplotlib.dates import DateFormatter, date2num
import matplotlib.dates as mdates
from mplfinance.original_flavor import candlestick_ohlc
import datetime
import json

# 數據資料處理
data = json.load(open('data.json'))
data = data['Time Series (5min)']

# 將資料轉換為繪圖格式
data_list = []
for timestamp, values in data.items():
    date_time = datetime.datetime.strptime(timestamp, "%Y-%m-%d %H:%M:%S")
    ohlc = [
        date2num(date_time),  # 日期轉換為數值
        float(values['1. open']),
        float(values['2. high']),
        float(values['3. low']),
        float(values['4. close'])
    ]
    data_list.append(ohlc)

# 繪製 K 線圖
fig, ax = plt.subplots(figsize=(10, 6))
ax.xaxis_date()
ax.xaxis.set_major_formatter(DateFormatter('%H:%M'))
ax.xaxis.set_major_locator(mdates.HourLocator(interval=1))
ax.set_title('K-line Chart')
ax.set_xlabel('Time')
ax.set_ylabel('Price')

candlestick_ohlc(ax, data_list, width=0.001, colorup='g', colordown='r')
plt.grid()

plt.savefig('k_line.png')
