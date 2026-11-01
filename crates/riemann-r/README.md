# RiemannFinMath（R パッケージ）

Rust の金融計算ライブラリー RiemannFinMath を、R から直接呼ぶパッケージです。計算はすべて Rust にあり、R は日付の変換と表の組み立てだけを行います。仕様は `docs/spec_r.md` にあります。

リポジトリ名と Rust の crate 名は `riemann`、`riemann-r` のままです。CRAN には公開していません。

## 導入

### Windows バイナリ（zip）：Rust も Rtools も不要

R 4.6 用です。依存パッケージの `tibble` は、zip からの導入では自動で入らないので、先に入れます。

```r
install.packages("tibble")
install.packages("RiemannFinMath_0.1.0.zip", repos = NULL, type = "binary")
```

### ソースパッケージ（tar.gz）：リポジトリもネットワークも不要

中核の crate と依存する Rust の crate を同梱しています。Windows では Rtools 4.5 と Rust が必要です。

```
rustup target add x86_64-pc-windows-gnu
R CMD INSTALL RiemannFinMath_0.1.0.tar.gz
```

**パスが長いと失敗します。** ビルドのパスが Windows の 260 文字の制限を超えると、`LNK1104` で止まります。短いパスのフォルダーで実行してください。

### リポジトリから（開発用）

```
R CMD INSTALL crates/riemann-r
```

## 配布ファイルの作り方

Riemann のルートで実行します。Rtools の `tar` と `xz` が PATH に必要です。

```
Rscript tools/package/build_source_package.R dist
R CMD check --as-cran dist/RiemannFinMath_0.1.0.tar.gz
R CMD INSTALL --build dist/RiemannFinMath_0.1.0.tar.gz
```

## Positron の R コンソールで試す

サンプルを全部実行します。

```r
source(system.file("examples", "quickstart.R", package = "RiemannFinMath"))
```

ヘルプは `?RiemannFinMath::bond_values`、全体の説明は `?RiemannFinMath::RiemannFinMath` です。

```r
bond <- RiemannFinMath::fixed_rate_bond(
  settlement_days = 1L, face_amount = 100,
  effective_date = "2020-03-20", termination_date = as.Date("2030-03-20"),
  tenor = "6M", calendar = "japan",
  convention = "following", termination_convention = "following", rule = "backward", end_of_month = FALSE,
  coupon_rate = 0.015, day_count = "act_act_isma", payment_convention = "following", redemption = 100
)
RiemannFinMath::bond_values(bond, rate = 0.02, day_count = "act_act_isma", compounding = "compounded",
                            frequency = 2L, settlement_date = "2026-11-04")
```

サンプルの値はすべて合成で、規約も例であり、市場の規約ではありません。

## 注意

- **カーブの再現誤差を確かめてください。** 大きく振れる気配では、割引係数がほぼ 0 になり、入力の気配を再現しないことがあります（`quote_repricing()`）。
- **カレンダーの一致は QuantLib とのものです。** 実際の市場休日とは照合していません。年別データの収録期間にも制限があります（`?RiemannFinMath::is_business_day`）。
- `library(RiemannFinMath)` すると `duration` などが他のパッケージと衝突することがあるので、`RiemannFinMath::` を付けて呼ぶのが安全です。
- 同梱した Rust の crate のライセンスは `system.file("RUST_DEPENDENCIES", package = "RiemannFinMath")`、QuantLib の表示は `THIRD_PARTY_NOTICES` にあります（配布ファイルだけ）。
