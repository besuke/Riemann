# 公開関数の旧名と新名の対応表

命名の方針は `docs/specification.md` 4.4 節にある。配布版の更新の前に、R と Rust の公開トップレベル関数をまとめて改名した（2026-11-03）。計算と数値の判定基準は変えていない。旧名の別名は残していない。

古い仕様書の本文（`docs/spec_bonds.md` など）には、当時の関数名がそのまま残っている。その時点の記録として直していないので、この表で読み替える。

## R の公開関数（64 個）

| 旧名 | 新名 |
|---|---|
| `accrued_amount` | `compute_accrued_amount` |
| `asset_swap_quote_sensitivity` | `compute_asset_swap_quote_sensitivity` |
| `barrier_option` | `compute_barrier_option_value` |
| `basis_point_value` | `compute_basis_point_value` |
| `binomial_option` | `compute_binomial_option_value` |
| `bond_cashflows` | `get_bond_cashflows` |
| `bond_values` | `compute_bond_values` |
| `bond_values_batch` | `compute_bond_values_batch` |
| `build_ois_curve` | `bootstrap_ois_curve` |
| `cds_maturity` | `compute_cds_maturity` |
| `cds_quote_sensitivity` | `compute_cds_quote_sensitivity` |
| `cds_value` | `compute_cds_value` |
| `clean_price` | `compute_clean_price` |
| `compound_yield_from_price` | `compute_compound_yield_from_price` |
| `convexity` | `compute_convexity` |
| `credit_default_swap` | `create_credit_default_swap` |
| `curve_node_sensitivity` | `compute_curve_node_sensitivity` |
| `curve_values` | `compute_curve_values` |
| `curve_values_batch` | `compute_curve_values_batch` |
| `dirty_price` | `compute_dirty_price` |
| `discount` | `compute_discount` |
| `discount_nodes` | `get_discount_nodes` |
| `duration` | `compute_duration` |
| `european_option` | `compute_european_option_value` |
| `fixed_rate_bond` | `create_fixed_rate_bond` |
| `forward_option` | `compute_forward_option_value` |
| `forward_rate` | `compute_forward_rate` |
| `future_basis` | `compute_future_basis` |
| `future_basket` | `compute_future_basket` |
| `hazard_curve` | `create_hazard_curve` |
| `holidays` | `generate_holidays` |
| `hull_white_bermudan_swaption` | `compute_hull_white_bermudan_swaption_value` |
| `hull_white_bond_option` | `compute_hull_white_bond_option_value` |
| `hull_white_callable_bond` | `compute_hull_white_callable_bond_value` |
| `hull_white_swaption` | `compute_hull_white_swaption_value` |
| `implied_volatility` | `compute_implied_volatility` |
| `monte_carlo_european` | `compute_monte_carlo_european_value` |
| `ois_asset_swap` | `compute_ois_asset_swap` |
| `ois_instruments` | `create_ois_instruments` |
| `ois_swap` | `create_ois_swap` |
| `ois_swap_cashflows` | `compute_ois_swap_cashflows` |
| `ois_swap_daily_rates` | `compute_ois_swap_daily_rates` |
| `ois_swap_quote_sensitivity` | `compute_ois_swap_quote_sensitivity` |
| `ois_swap_value` | `compute_ois_swap_value` |
| `prdc_value` | `compute_prdc_value` |
| `price_from_compound_yield` | `compute_price_from_compound_yield` |
| `price_from_simple_rate` | `compute_price_from_simple_rate` |
| `quote_repricing` | `get_quote_repricing` |
| `quote_sensitivity` | `compute_quote_sensitivity` |
| `remaining_years` | `compute_remaining_years` |
| `risk_from_compound_yield` | `compute_risk_from_compound_yield` |
| `risk_from_simple_rate` | `compute_risk_from_simple_rate` |
| `sabr_volatility` | `compute_sabr_volatility` |
| `settlement_date` | `compute_settlement_date` |
| `simple_rate_from_price` | `compute_simple_rate_from_price` |
| `survival_probability` | `compute_survival_probability` |
| `value_on_curve` | `compute_bond_value_on_curve` |
| `value_with_zero_spread` | `compute_bond_value_with_zero_spread` |
| `yen_asset_swap` | `compute_yen_asset_swap` |
| `yield_from_price` | `compute_yield_from_price` |
| `yield_sensitivity` | `compute_yield_sensitivity` |
| `yield_value_basis_point` | `compute_yield_value_basis_point` |
| `z_spread_from_price` | `compute_z_spread_from_price` |
| `zero_rate` | `compute_zero_rate` |

### 変えなかった R の関数

| 関数 | 理由 |
|---|---|
| `bootstrap_hazard_curve` | `bootstrap_` は意味が明確なので認める |
| `calibrate_sabr` | `calibrate_` は意味が明確なので認める |
| `is_business_day` | `is_` は意味が明確なので認める |
| `with_extrapolation` | 元の値を変更せず、設定を変えた新しい値を返すので `with_` を認める |

### 判断の根拠

| 新名 | 根拠 |
|---|---|
| `bootstrap_ois_curve` | 実際にブートストラップする（`crates/riemann/src/ois.rs`） |
| `compute_ois_swap_cashflows` | 翌日物レグの額をカーブと Fixing から新たに算出する |
| `get_bond_cashflows` | 債券を作ったときに持つキャッシュフローを返す。市場の入力を使わない |
| `get_discount_nodes`、`get_quote_repricing` | カーブを作ったときの節点と再現結果を返す |
| `generate_holidays` | カレンダーの規則から日付列を作る |
| `compute_settlement_date` | カレンダーで日付を進めて算出する |
| `compute_discount` | 節点の間を補間して算出する |

## Rust の公開トップレベル関数

メソッド（`curve.discount(d)`、`bond.clean_price(...)`、トレイトの `x.normal_pdf()` など）は変えない。

| 旧名 | 新名 |
|---|---|
| `accrued_amount` | `compute_accrued_amount` |
| `asset_swap_quote_sensitivity` | `compute_asset_swap_quote_sensitivity` |
| `barrier_greeks` | `compute_barrier_greeks` |
| `barrier_value` | `compute_barrier_value` |
| `basis_point_value` | `compute_basis_point_value` |
| `bermudan_swaption` | `compute_bermudan_swaption` |
| `binomial_greeks` | `compute_binomial_greeks` |
| `binomial_value` | `compute_binomial_value` |
| `bond_values_batch` | `compute_bond_values_batch` |
| `bootstrap_hazard` | `bootstrap_hazard_curve` |
| `brent` | `solve_brent` |
| `brent_with_step` | `solve_brent_with_step` |
| `build_ois_curve` | `bootstrap_ois_curve` |
| `callable_bond` | `compute_callable_bond` |
| `cds_maturity` | `compute_cds_maturity` |
| `cds_sensitivity` | `compute_cds_sensitivity` |
| `cds_value` | `compute_cds_value` |
| `close` | `is_close` |
| `close_enough` | `is_close_enough` |
| `close_with` | `is_close_with` |
| `compound_yield_from_price` | `compute_compound_yield_from_price` |
| `convexity` | `compute_convexity` |
| `curve_sensitivity` | `compute_curve_sensitivity` |
| `curve_values_batch` | `compute_curve_values_batch` |
| `duration` | `compute_duration` |
| `easter_monday_day_of_year` | `compute_easter_monday_day_of_year` |
| `estr` | `create_estr_index` |
| `european_greeks` | `compute_european_greeks` |
| `european_value` | `compute_european_value` |
| `finite_difference_newton_safe` | `solve_finite_difference_newton_safe` |
| `fixed_rate_leg` | `create_fixed_rate_leg` |
| `forward_greeks` | `compute_forward_greeks` |
| `forward_value` | `compute_forward_value` |
| `future_basis` | `compute_future_basis` |
| `implied_volatility` | `compute_implied_volatility` |
| `inverse` | `compute_inverse` |
| `inverse_normal_cdf` | `compute_inverse_normal_cdf` |
| `irr` | `compute_irr` |
| `isda_value` | `compute_isda_value` |
| `jamshidian_swaption` | `compute_jamshidian_swaption` |
| `jamshidian_swaption_sensitivity` | `compute_jamshidian_swaption_sensitivity` |
| `midpoint_value` | `compute_midpoint_value` |
| `monte_carlo_european` | `compute_monte_carlo_european` |
| `newton_safe` | `solve_newton_safe` |
| `normal_cdf` | `compute_normal_cdf` |
| `normal_pdf` | `compute_normal_pdf` |
| `normal_quadrature` | `generate_normal_quadrature` |
| `npv` | `compute_npv` |
| `npv_on_curve` | `compute_npv_on_curve` |
| `ois_asset_swap` | `compute_ois_asset_swap` |
| `orthodox_easter_monday_day_of_year` | `compute_orthodox_easter_monday_day_of_year` |
| `overnight_coupon_amount` | `compute_overnight_coupon_amount` |
| `overnight_days` | `compute_ois_swap_daily_rates` |
| `prdc_analytic` | `compute_prdc_analytic` |
| `prdc_analytic_value` | `compute_prdc_analytic_value` |
| `prdc_monte_carlo` | `compute_prdc_monte_carlo` |
| `price_from_compound_yield` | `compute_price_from_compound_yield` |
| `price_from_simple_rate` | `compute_price_from_simple_rate` |
| `quote_sensitivity` | `compute_quote_sensitivity` |
| `remaining_years` | `compute_remaining_years` |
| `risk_from_compound_yield` | `compute_risk_from_compound_yield` |
| `risk_from_simple_rate` | `compute_risk_from_simple_rate` |
| `sabr_volatility` | `compute_sabr_volatility` |
| `simple_rate_from_price` | `compute_simple_rate_from_price` |
| `sofr` | `create_sofr_index` |
| `sonia` | `create_sonia_index` |
| `standard_normal` | `generate_standard_normal` |
| `tonar` | `create_tonar_index` |
| `uniform` | `generate_uniform` |
| `value_and_gradient` | `compute_value_and_gradient` |
| `value_gradient_hessian` | `compute_value_gradient_hessian` |
| `yen_asset_swap` | `compute_yen_asset_swap` |
| `yield_sensitivity` | `compute_yield_sensitivity` |
| `yield_value_basis_point` | `compute_yield_value_basis_point` |

`OisSwap::new` は `create_ois_swap` に置き換えた。

### `names` モジュール（18 個）

選択肢の文字列を型に変換する関数で、`names::parse_<旧名>` にした。保存された値の取得ではなく変換なので、`parse_` を使う。対象：
`barrier_type`、`business_day_convention`、`calendar`、`cds_engine`、`compounding`、`date_generation_rule`、`day_count`、`duration_type`、`exercise`、`forward_model`、`ois_interpolation`、`option_type`、`overnight_index`、`period`、`price_type`、`protection_side`、`sabr_volatility_type`、`swap_side`。

### 変えなかった Rust の関数

| 関数 | 理由 |
|---|---|
| `calibrate_sabr`、`is_leap` | `calibrate_`、`is_` は認める |
| `time::schedule::generate`（公開名 `generate_schedule`） | すでに動詞 |
| `batch::map_parallel`、`math::linear::solve`、`yen_bond::truncate_decimal` | すでに動詞 |

`math::solver1d` の解法は `solve_` に、比較の述語は `is_close` などにした。指標は `create_tonar_index` などにした。

## R と Rust で名前が違うもの

R の関数が Rust の複数の関数を束ねる場合は、R の名前を商品の単位にした。

| R | Rust |
|---|---|
| `compute_european_option_value` | `compute_european_value`、`compute_european_greeks` |
| `compute_barrier_option_value` | `compute_barrier_value`、`compute_barrier_greeks` |
| `compute_binomial_option_value` | `compute_binomial_value`、`compute_binomial_greeks` |
| `compute_forward_option_value` | `compute_forward_value`、`compute_forward_greeks` |
| `compute_monte_carlo_european_value` | `compute_monte_carlo_european` |
| `compute_hull_white_swaption_value` | `compute_jamshidian_swaption`、`compute_jamshidian_swaption_sensitivity` |
| `compute_hull_white_bermudan_swaption_value`、`compute_hull_white_callable_bond_value` | `compute_bermudan_swaption`、`compute_callable_bond` |
| `compute_prdc_value` | `compute_prdc_analytic`、`compute_prdc_monte_carlo` |
| `compute_bond_value_on_curve` など | `FixedRateBond` のメソッド（`bond.value_on_curve(...)`） |

