# price

This is a program that calculates the price of all your stocks/ETFs/cryptos in your portfolio.

## Features

- Get your total portfolio value in any currency

## Usage

In the config/ dir. You can add a `portfolio.toml` to create your assets list.

### Example

`config/portfolio.toml`

This toml file is used to store your portfolio. You can add as many assets as you want. The program will automatically fetch the price of each asset and calculate the total value of your portfolio in the target currencies.

```toml
[us-stock]
amd = 10

[us-etf]
QQQ = 20

[tw-stock]
"2330.TW" = 10

[tw-etf]
"0050.TW" = 20

[crypto]
eth = 0.5
sol = 0.5

[forex]
usd = 100
twd = 10000
```

`config/api_key.toml`

This file is used to store your API keys. You can add as many API keys as you want. The program will automatically fetch the price of each asset and calculate the total value of your portfolio in the target currencies.

```toml
"alpha_vantage_api_key" = "XXXXXXXXXXXXXXXX"
"exchangerate_api_key" = "xxxxxxxxxxxxxxxxxxxxxxxx"
```

## Development

### Progress

- [x] Fetch stock prices
- [x] Fetch ETF prices
- [x] Fetch crypto prices
- [x] Fetch forex prices
- [x] Calculate total portfolio value in any currency

- [x] alpha_vantage API
- [x] binance API
- [x] exchange_rate API
- [ ] pyth(pyth network) API
- [x] redstone API
- [x] yahoo finance API
