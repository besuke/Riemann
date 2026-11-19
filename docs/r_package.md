# rmql（R パッケージ）

Rust の金融計算ライブラリー rmql を、R から直接呼ぶパッケージです。計算はすべて Rust にあり、R は日付の変換と表の組み立てだけを行います。仕様は `docs/spec_r.md` にあります。

リポジトリ名と Rust の crate 名は `rmql`、`rmql-r` のままです。CRAN には公開していません。

## 導入

### Windows：Rust も Rtools も不要（推奨）

R 4.6 用のバイナリを、GitHub Pages の R リポジトリから入れます。依存の `purrr` と `tibble` も CRAN から自動で入ります。

```r
install.packages(
  "rmql",
  repos = c("https://besuke.github.io/RiemannQL", "https://cloud.r-project.org")
)
```

**`remotes::install_github()` は使えません。** `install_github()` は常にソースからビルドするので、Windows でも Rust が要ります（Rust がない PC では `configure.win` が上の入れ方を示して止まります）。

zip を直接入れる場合は、GitHub の Releases（https://github.com/besuke/RiemannQL/releases）にあります。`purrr` と `tibble` は先に入れます。

```r
install.packages(c("purrr", "tibble"))
install.packages(
  "https://github.com/besuke/RiemannQL/releases/download/v0.3.4/rmql_0.3.4.zip",
  repos = NULL, type = "binary"
)
```

### GitHub から（ソース）：Rust が必要（Windows では Rtools も）

```r
remotes::install_github("besuke/RiemannQL")
```

R パッケージはリポジトリの直下にあるので、`subdir` は要りません。remotes がリポジトリ全体を取り、中核の crate（`crates/rmql`）が隣にあるので、そのままビルドできます。`configure` は、中核が見つからないときだけ止まります。

### ソースパッケージ（tar.gz）：リポジトリもネットワークも不要

中核の crate と依存する Rust の crate を同梱しています。Windows では Rtools 4.5 と Rust が必要です。

```
rustup target add x86_64-pc-windows-gnu
R CMD INSTALL rmql_0.3.4.tar.gz
```

**パスが長いと失敗します。** ビルドのパスが Windows の 260 文字の制限を超えると、`LNK1104` で止まります。短いパスのフォルダーで実行してください。

### リポジトリから（開発用）

```
R CMD INSTALL .
```

## 配布ファイルの作り方

rmql のルートで実行します。Rtools の `tar` と `xz` が PATH に必要です。

```
Rscript tools/package/build_source_package.R dist
R CMD check --as-cran dist/rmql_0.3.4.tar.gz
R CMD INSTALL --build dist/rmql_0.3.4.tar.gz
```

## Positron の R コンソールで試す

サンプルを全部実行します。

```r
source(system.file("examples", "quickstart.R", package = "rmql"))
```

ヘルプは `?rmql::compute_bond_values`、全体の説明は `?rmql::rmql` です。

```r
bond <- rmql::create_fixed_rate_bond(
  settlement_days = 1L, face_amount = 100,
  effective_date = "2020-03-20", termination_date = as.Date("2030-03-20"),
  tenor = "6M", calendar = "japan",
  convention = "following", termination_convention = "following", rule = "backward", end_of_month = FALSE,
  coupon_rate = 0.015, day_count = "act_act_isma", payment_convention = "following", redemption = 100
)
rmql::compute_bond_values(bond, rate = 0.02, day_count = "act_act_isma", compounding = "compounded",
                            frequency = 2L, settlement_date = "2026-11-04")
```

導入の確認は `Rscript --vanilla tools/package/smoke_test.R 0.3.0 <ライブラリー>` で行う（リポジトリから）。

サンプルの値はすべて合成で、規約も例であり、市場の規約ではありません。

## 注意

- **カーブの再現誤差を確かめてください。** 大きく振れる気配では、割引係数がほぼ 0 になり、入力の気配を再現しないことがあります（`get_quote_repricing()`）。
- **カレンダーは、実際の市場休日とは照合していません。** 年別データの収録期間にも制限があります（`?rmql::is_business_day`）。
- `library(rmql)` すると `duration` などが他のパッケージと衝突することがあるので、`rmql::` を付けて呼ぶのが安全です。
- 同梱した Rust の crate のライセンスは `system.file("RUST_DEPENDENCIES", package = "rmql")`、移したコードの著作権表示は `THIRD_PARTY_NOTICES` にあります（配布ファイルだけ）。
