# rmql

Rust で書いた金融計算ライブラリー（Rust Mathematical Quantitative Library）。R から呼ぶ。GitHub のリポジトリ名は `RiemannQL`（`besuke/RiemannQL`）。

債券、金利カーブ、OIS、オプション、信用、金利モデル、リスクの計算を、評価日・市場データ・計算規約を引数として与える関数で行う。計算は Rust の中核にあり、R からは値を渡して、結果の表（tibble）を受け取る。

## 設計

オブジェクト指向とテンプレートの設計を使わず、値と純粋関数で組み立てる。

- **値と純粋関数**：評価日・市場データ・計算規約は、すべて引数で渡す。グローバルな評価日や、書き換わる状態を持たない。同じ入力は、同じ結果になる。
- **規約に既定値がない**：カレンダー、日数計算、複利の方式などは、呼ぶときに必ず指定する。
- **継承・オブザーバーを使わない**：金融上の選択肢は、名前（文字列）で選ぶ。名前は、下の「選べるもの」のとおり。
- **公開の API は単純**：総称型は内部（自動微分など）に留め、公開しない。`unsafe` は使わない。

## インストール

**Windows（バイナリ。Rust も Rtools も要らない）**：R 4.4・4.5・4.6 用。依存の `purrr` と `tibble` も入る。

```r
install.packages(
  "rmql",
  repos = c("https://besuke.github.io/RiemannQL", "https://cloud.r-project.org")
)
```

**GitHub から（ソースからビルド。Rust と、Windows では Rtools と `x86_64-pc-windows-gnu` のターゲットが要る）**：

```r
remotes::install_github("besuke/RiemannQL")
```

**このリポジトリから**：直下で `R CMD INSTALL .`。入れ替えのときは、その DLL を使っている R のプロセスを先に止める。詳しくは [docs/client_guide.md](docs/client_guide.md)。

## 使い方

```r
library(rmql)

# 年率 2%（連続複利）の平らなカーブで、5 年後の割引係数
curve <- create_flat_curve("2026-11-02", 0.02, "actual_365_fixed", "continuous")
compute_discount(curve, as.Date("2031-11-02"))   # 0.9047878393

# 営業日かどうか（カレンダーは名前で指定）
is_business_day("japan", c("2026-11-02", "2026-11-03"))
```

日付は `Date` か ISO 形式の文字列（`YYYY-MM-DD`）。`on_error` を持つ関数は、`"stop"` で最初の失敗行で止まり、`"na"` でその行を `NA` にする。使用例は `system.file("examples", "quickstart.R", package = "rmql")`、関数の一覧は `?rmql`。

## 選べるもの

規約や方式は、次の名前（文字列）で指定する。

### カレンダー（71）

- **全般**（2）：`null_calendar`、`weekends_only`
- **米州**（14）：`argentina_merval`、`brazil_settlement`、`brazil_exchange`、`canada_settlement`、`canada_tsx`、`chile_sse`、`mexico_bmv`、`us_settlement`、`us_nyse`、`us_government_bond`、`us_nerc`、`us_libor_impact`、`us_federal_reserve`、`us_sofr`
- **欧州**（31）：`target`、`austria_settlement`、`austria_exchange`、`czech_republic_pse`、`denmark`、`finland`、`france_settlement`、`france_exchange`、`germany_settlement`、`germany_frankfurt_stock_exchange`、`germany_xetra`、`germany_eurex`、`germany_euwax`、`hungary`、`iceland_icex`、`italy_settlement`、`italy_exchange`、`norway`、`poland_settlement`、`poland_wse`、`romania_public`、`romania_bvb`、`russia_settlement`、`russia_moex`、`slovakia_bsse`、`sweden`、`switzerland`、`ukraine_use`、`uk_settlement`、`uk_exchange`、`uk_metals`
- **中東・アフリカ**（7）：`botswana`、`israel_settlement`、`israel_tase`、`israel_shir`、`saudi_arabia_tadawul`、`south_africa`、`turkey`
- **アジア・太平洋**（17）：`japan`、`australia_settlement`、`australia_asx`、`china_sse`、`china_ib`、`hong_kong_hkex`、`india_nse`、`indonesia_bej`、`indonesia_jsx`、`indonesia_idx`、`new_zealand_wellington`、`new_zealand_auckland`、`singapore_sgx`、`south_korea_settlement`、`south_korea_krx`、`taiwan_tsec`、`thailand`

営業日の判定と祝日の一覧は `is_business_day()`、`generate_holidays()`。

### 日数計算（22 種類）

`actual_360`、`actual_360_include_last_day`、`actual_364`、`actual_36525`、`actual_36525_include_last_day`、`actual_365_fixed`、`actual_365_fixed_canadian`、`actual_365_fixed_no_leap`、`actual_366`、`actual_366_include_last_day`、`act_act_isda`、`act_act_isma`、`act_act_afb`、`thirty_360_usa`、`thirty_360_bond_basis`、`thirty_360_european`、`thirty_360_italian`、`thirty_360_isda`、`thirty_360_nasd`、`thirty_365`、`one`、`simple`

`business_252:<カレンダー名>` で、そのカレンダー上の Business/252。年数は `compute_year_fraction()`。

### 複利の方式（5 種類）

`simple`、`compounded`、`continuous`、`simple_then_compounded`、`compounded_then_simple`

### 営業日の調整（7 種類）

`following`、`modified_following`、`half_month_modified_following`、`preceding`、`modified_preceding`、`unadjusted`、`nearest`

### 日程の生成規則（10 種類）

`backward`、`forward`、`zero`、`third_wednesday`、`third_wednesday_inclusive`、`twentieth`、`twentieth_imm`、`old_cds`、`cds`、`cds2015`

期間は `"3M"`、`"1Y"` のような文字列。日程は `generate_schedule()`、日付の計算は `compute_adjusted_date()`、`compute_advanced_date()`、`compute_end_of_month()`。

### そのほか

- **翌日物金利の指数**：`tonar`、`estr`、`sofr`、`sonia`
- **OIS カーブの補間**：`log_linear`、`monotonic_log_cubic`
- **価格の種類**：`clean`、`dirty`。**デュレーション**：`simple`、`modified`、`macaulay`
- **オプション**：`call`、`put`、`european`、`american`、バリア（`down_in`、`up_in`、`down_out`、`up_out`）、モデル（`black`、`bachelier`）、SABR の種類（`lognormal`、`normal`）
- **CDS**：`buyer`、`seller`、計算方式（`midpoint`、`isda`）。**スワップ**：`payer`、`receiver`

## 計算できるもの（関数）

### 金利カーブ

- 作る：`create_flat_curve()`、`create_zero_curve()`、`create_discount_curve()`、`create_zero_spreaded_curve()`、`with_extrapolation()`
- 市場の気配から作る（ブートストラップ）：`create_ois_instruments()`（OIS の気配）と `bootstrap_ois_curve()`（TONAR・€STR・SOFR・SONIA）、`bootstrap_bond_curve()`、`bootstrap_xccy_curve()`、`bootstrap_hazard_curve()`
- 読む：`compute_discount()`、`compute_zero_rate()`、`compute_forward_rate()`、`compute_curve_values()`、`get_discount_nodes()`、`get_curve_reference_date()`、`get_quote_repricing()`
- 感応度：`compute_quote_sensitivity()`、`compute_curve_node_sensitivity()`

### 債券

- `create_fixed_rate_bond()`、`get_bond_cashflows()`、`compute_settlement_date()`、`compute_accrued_amount()`
- 価格と利回り：`compute_bond_values()`（件数が多いときは `compute_bond_values_batch()`）、`compute_clean_price()`、`compute_dirty_price()`、`compute_yield_from_price()`、`compute_z_spread_from_price()`、`compute_bond_value_on_curve()`、`compute_bond_value_with_zero_spread()`、`compute_cashflow_npv()`（キャッシュフローの現在価値）
- 日本式の利回り：`compute_price_from_simple_rate()`、`compute_simple_rate_from_price()`、`compute_compound_yield_from_price()`、`compute_price_from_compound_yield()`、`compute_remaining_years()`
- リスク：`compute_duration()`、`compute_convexity()`、`compute_basis_point_value()`、`compute_yield_sensitivity()`、`compute_yield_value_basis_point()`、`compute_risk_from_compound_yield()`、`compute_risk_from_simple_rate()`

### スワップとアセットスワップ

- OIS スワップ：`create_ois_swap()`、`compute_ois_swap_value()`、`compute_ois_swap_cashflows()`、`compute_ois_swap_dates()`（期間から日付を作る）、`compute_ois_swap_daily_rates()`、`compute_ois_swap_quote_sensitivity()`
- アセットスワップ：`compute_ois_asset_swap()`、`compute_yen_asset_swap()`、`compute_yen_asset_swap_on_basis_curve()`、`compute_asset_swap_quote_sensitivity()`
- 国債先物：`create_ctd_curve()`、`compute_future_basis()`、`compute_future_basket()`、`compute_ctd_time_value()`
- 通貨ベーシス：`create_xccy_instruments()`、`bootstrap_xccy_curve()`
- TRS：`create_trs_funding_leg()`、`create_trs_return_leg()`、`compute_trs_funding_cashflows()`、`compute_trs_return_cashflows()`、`get_trs_funding_schedule()`、`get_trs_funding_observations()`、`get_trs_return_schedule()`

### オプション

- 価格：`compute_european_option_value()`、`compute_forward_option_value()`（Black、Bachelier）、`compute_barrier_option_value()`、`compute_american_approximation_value()`、`compute_binomial_option_value()`、`compute_monte_carlo_european_value()`、`compute_longstaff_schwartz_value()`
- インプライド・ボラティリティ：`compute_implied_volatility()`
- 乱数：`generate_black_scholes_paths()`、`generate_sobol_sequence()`、`generate_correlated_normals()`、`compute_correlated_normals()`
- FX スマイル：`create_fx_smile()`、`compute_fx_smile_volatility()`、`compute_fx_strike_from_delta()`、`compute_fx_delta_from_strike()`、`compute_fx_atm_strike()`、`compute_fx_smile_strikes()`

### 金利モデル

- Hull-White：`compute_hull_white_swaption_value()`、`compute_hull_white_bermudan_swaption_value()`、`compute_hull_white_bond_option_value()`、`compute_hull_white_callable_bond_value()`、`compute_hull_white_callable_bond_decisions()`（コール日ごとの行使判断）、`calibrate_hull_white()`
- SABR：`compute_sabr_volatility()`、`calibrate_sabr()`
- LMM：`create_lmm()`、`generate_lmm_paths()`、`compute_lmm_caplet_value()`、`compute_lmm_swaption_value()`、`compute_lmm_bermudan_swaption()`、`calibrate_lmm_volatilities()`、`calibrate_lmm_correlation()`、相関行列の作成（`create_exponential_correlation()`、`create_reduced_correlation()`）、`compute_rebonato_swaption_volatility()`
- キャップレット：`compute_caplet_value()`、`compute_caplet_from_normals()`。後決め金利：`compute_backward_caplet_value()`、`compute_backward_forward_rate()`、`compute_backward_period_state()`

### 信用

- CDS：`create_credit_default_swap()`、`compute_cds_value()`、`compute_cds_maturity()`、`compute_cds_quote_sensitivity()`
- ハザードと生存確率：`create_hazard_curve()`、`compute_hazard_rate()`、`compute_conditional_pd()`、`create_survival_probability_curve()`、`compute_survival_probability()`
- ポートフォリオの損失：`compute_credit_risk()`、`compute_credit_loss_distribution()`、`compute_asrf_var()`、`compute_irb_capital()`、`simulate_credit_losses()`
- トランシェ：`compute_tranche_loss()`、`compute_tranche_risk()`、`compute_tranche_legs()`、`compute_lhp_expected_tranche_loss()`

### 住宅ローン債（MBS）

`create_mbs_pass_through()`、`compute_mbs_cashflows()`、`compute_mbs_path_values()`、`compute_mbs_oas()`（静的 OAS とダイナミック OAS）、`generate_hull_white_monthly_rates()`、`compute_pac_schedule()`、`compute_sequential_cmo()`、`compute_pac_support_cmo()`

### 仕組債（PRDC）

`compute_prdc_value()`、`compute_prdc_smile_value()`、`compute_prdc_three_factor_value()`、`calibrate_prdc_three_factor()`

### リスクと規制

- IRRBB：`get_irrbb_shock_sizes()`、`compute_irrbb_shocks()`、`compute_irrbb_curve_points()`、`create_irrbb_shocked_curve()`
- 市場リスク：`compute_portfolio_risk()`、`compute_loss_risk()`、`compute_rolling_var()`、`compute_var_backtest()`、`compute_traffic_light()`、`compute_monte_carlo_risk()`、`compute_return_moments()`
- ポートフォリオ：`compute_optimal_portfolio()`、`compute_efficient_frontier()`、`create_portfolio_constraints()`

### 自動微分と並列

債券とカーブの感応度は、自動微分で求める。債券とカーブの評価は、`compute_bond_values_batch()` と `compute_curve_values_batch()` で、スレッドを使って並列に行える（結果はスレッド数によらない）。

## まだないもの

- TypeScript（Node 版）、WebAssembly（ブラウザー版）、Excel の Office アドイン
- 合成カレンダー（複数カレンダーの合成）と、利用者が定義するカレンダー
- カレンダーは、年ごとのデータの収録期間に限りがある（`?is_business_day`）。実際の市場の休日とは、まだ突き合わせていない。

## リポジトリの構成

| 場所 | 内容 |
|---|---|
| 直下（`DESCRIPTION`、`NAMESPACE`、`R/`、`man/`、`inst/`、`src/`、`configure*`、`tests/testthat/`） | R パッケージ `rmql`。`src/rust` が R と Rust の境界（extendr）で、計算は持たない |
| `crates/rmql/` | Rust の中核（計算はすべてここ）と、そのテスト |
| `docs/` | 設計の方針（`specification.md`）、機能ごとの記録（`spec_*.md`）、クライアント向けの手引き |
| `tests/golden/`、`tools/golden/` | テスト用の参照値と、それを作るスクリプト |
| `tools/package/` | 配布用のスクリプト（ソースパッケージ、R リポジトリの索引、スモークテスト） |
| `tools/verify/`、`comparison/` | 参照値との比較資料（Quarto） |
| `.github/workflows/` | 配布物のビルドと公開 |

## 開発

```powershell
cargo test --workspace                    # Rust の中核のテスト
cargo clippy --workspace --all-targets
R CMD INSTALL .                           # R パッケージ（Rust と Rtools が要る）
```

配布は、GitHub の Actions の「Build the R repository」を手で実行する（`publish` を入れると公開）か、`v` で始まるタグを push する。Windows の R 4.4・4.5・4.6 ごとにビルドしてスモークテストを通し、R リポジトリ（`gh-pages` ブランチ）を更新する。作業の手順と引継ぎは `CLAUDE.md`。

## 文書

- 設計の方針（草案）：[docs/specification.md](docs/specification.md)
- R パッケージ：[docs/client_guide.md](docs/client_guide.md)（入れ方と使い方）、[docs/r_package.md](docs/r_package.md)、[docs/spec_r.md](docs/spec_r.md)
- 機能ごとの記録：`docs/spec_*.md`

## 著作権

Copyright (c) 2026 Eiichi Horigome。ただし、QuantLib から派生した部分の著作権は、[THIRD_PARTY_NOTICES](THIRD_PARTY_NOTICES) と各ファイルの先頭に記載した著作権者にある。

## ライセンス

次のどちらかを選んで利用できる（MIT と Apache-2.0 の併用）。

- MIT License（[LICENSE-MIT](LICENSE-MIT)）
- Apache License 2.0（[LICENSE-APACHE](LICENSE-APACHE)）

明示的な断りがない限り、本リポジトリへの貢献は、Apache License 2.0 の定義に従い、追加の条件なしに上記の併用ライセンスで提供されたものとする。

## 参考にしたライブラリー

- [QuantLib](https://github.com/lballabio/QuantLib)（修正 BSD ライセンス）
- [finmath-lib](https://github.com/finmath/finmath-lib)（Apache License 2.0）

数式、用語、検証の参考にしている。これらのオブジェクト指向の構成は引き継いでいない（上の「設計」）。移したコードの著作権表示とライセンスは [THIRD_PARTY_NOTICES](THIRD_PARTY_NOTICES) に載せている。扱いの決まりは `docs/specification.md` の「ライセンス」を参照。
