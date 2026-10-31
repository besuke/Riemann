# riemann（R パッケージ）

Rust の金融計算ライブラリー Riemann を、R から直接呼ぶパッケージです。計算はすべて Rust にあり、R は日付の変換と表の組み立てだけを行います。仕様は `docs/spec_r.md` にあります。

## 導入（Windows）

Rtools 4.5 と Rust を入れて、GNU の target を追加します。

```
rustup target add x86_64-pc-windows-gnu
```

Riemann のルートで導入します。

```
R CMD INSTALL crates/riemann-r
```

## Positron の R コンソールで試す

サンプルを全部実行します。

```r
source(system.file("examples", "quickstart.R", package = "riemann"))
```

1 行ずつ試すときは、`crates/riemann-r/inst/examples/quickstart.R` を開いて、Ctrl+Enter で実行します。

```r
bond <- riemann::fixed_rate_bond(
  settlement_days = 1L, face_amount = 100,
  effective_date = "2020-03-20", termination_date = as.Date("2030-03-20"),
  tenor = "6M", calendar = "japan",
  convention = "following", termination_convention = "following", rule = "backward", end_of_month = FALSE,
  coupon_rate = 0.015, day_count = "act_act_isma", payment_convention = "following", redemption = 100
)
riemann::bond_values(bond, rate = 0.02, day_count = "act_act_isma", compounding = "compounded",
                     frequency = 2L, settlement_date = "2026-11-04")
```

サンプルの値はすべて合成で、規約も例であり、市場の規約ではありません。

## 注意

- **カーブの再現誤差を確かめてください。** 大きく振れる気配では、割引係数がほぼ 0 になり、入力の気配を再現しないことがあります（`quote_repricing()`）。
- **カレンダーの一致は QuantLib とのものです。** 実際の市場休日とは照合していません。年別データの収録期間にも制限があります（`docs/spec_dates_calendars.md` 3.5 節）。
- `library(riemann)` すると `duration` などが他のパッケージと衝突することがあるので、`riemann::` を付けて呼ぶのが安全です。
