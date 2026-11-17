# RiemannFinMath（R パッケージ）

Rust の金融計算ライブラリー RiemannFinMath を、R から直接呼ぶパッケージです。計算はすべて Rust にあり、R は日付の変換と表の組み立てだけを行います。仕様は `docs/spec_r.md` にあります。

リポジトリ名と Rust の crate 名は `riemann`、`riemann-r` のままです。CRAN には公開していません。

## 導入

### Windows バイナリ（zip）：Rust も Rtools も不要

R 4.6 用です。GitHub の Releases（https://github.com/besuke/Riemann/releases）に置いてあります。依存パッケージの `purrr` と `tibble` は、zip からの導入では自動で入らないので、先に入れます。

```r
install.packages(c("purrr", "tibble"))
install.packages(
  "https://github.com/besuke/Riemann/releases/download/v0.3.2/RiemannFinMath_0.3.2.zip",
  repos = NULL, type = "binary"
)
```

### GitHub から（ソース）：Rust と Rtools が必要

```r
remotes::install_github("besuke/Riemann", subdir = "crates/riemann-r")
```

R パッケージは `crates/riemann-r` にあるので、`subdir` が必要です。中核の crate（`crates/riemann`）はこのフォルダーの外にあるため、`configure` が DESCRIPTION の `RemoteSha`（remotes が書くコミット）を読み、同じコミットの中核を cargo が GitHub から取ります。Windows では Rtools 4.5 と Rust（`rustup target add x86_64-pc-windows-gnu`）、ネットワークが必要です。

### ソースパッケージ（tar.gz）：リポジトリもネットワークも不要

中核の crate と依存する Rust の crate を同梱しています。Windows では Rtools 4.5 と Rust が必要です。

```
rustup target add x86_64-pc-windows-gnu
R CMD INSTALL RiemannFinMath_0.3.0.tar.gz
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
R CMD check --as-cran dist/RiemannFinMath_0.3.0.tar.gz
R CMD INSTALL --build dist/RiemannFinMath_0.3.0.tar.gz
```

## Positron の R コンソールで試す

サンプルを全部実行します。

```r
source(system.file("examples", "quickstart.R", package = "RiemannFinMath"))
```

ヘルプは `?RiemannFinMath::compute_bond_values`、全体の説明は `?RiemannFinMath::RiemannFinMath` です。

```r
bond <- RiemannFinMath::create_fixed_rate_bond(
  settlement_days = 1L, face_amount = 100,
  effective_date = "2020-03-20", termination_date = as.Date("2030-03-20"),
  tenor = "6M", calendar = "japan",
  convention = "following", termination_convention = "following", rule = "backward", end_of_month = FALSE,
  coupon_rate = 0.015, day_count = "act_act_isma", payment_convention = "following", redemption = 100
)
RiemannFinMath::compute_bond_values(bond, rate = 0.02, day_count = "act_act_isma", compounding = "compounded",
                            frequency = 2L, settlement_date = "2026-11-04")
```

導入の確認は `Rscript --vanilla tools/package/smoke_test.R 0.3.0 <ライブラリー>` で行う（リポジトリから）。

サンプルの値はすべて合成で、規約も例であり、市場の規約ではありません。

## 注意

- **カーブの再現誤差を確かめてください。** 大きく振れる気配では、割引係数がほぼ 0 になり、入力の気配を再現しないことがあります（`get_quote_repricing()`）。
- **カレンダーの一致は QuantLib とのものです。** 実際の市場休日とは照合していません。年別データの収録期間にも制限があります（`?RiemannFinMath::is_business_day`）。
- `library(RiemannFinMath)` すると `duration` などが他のパッケージと衝突することがあるので、`RiemannFinMath::` を付けて呼ぶのが安全です。
- 同梱した Rust の crate のライセンスは `system.file("RUST_DEPENDENCIES", package = "RiemannFinMath")`、QuantLib の表示は `THIRD_PARTY_NOTICES` にあります（配布ファイルだけ）。
