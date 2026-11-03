# 公開関数の命名の移行一覧

命名の方針は `docs/specification.md` 4.4 節にある。OIS スワップの関数はすでに適用した（2026-11-03）。ここには、方針に合わない既存の公開 API と、改名の案を挙げる。**まだ改名していない。案であり、確定ではない。**

判断の基準：保存済みの値を取り出すだけなら `get_`、新たに算出するなら `compute_`。「要判断」は、方針だけでは決まらないものである（最後の節）。

## 適用済み（OIS スワップ）

| 旧名 | 新名 | 備考 |
|---|---|---|
| `ois_swap` | `create_ois_swap` | Rust の `OisSwap::new` も同じ名前の関数にした |
| `ois_swap_value` | `compute_ois_swap_value` | |
| `ois_swap_cashflows` | `get_ois_swap_cashflows` | ユーザーの指定どおり。ただし下の注意を参照 |
| `ois_swap_daily_rates` | `compute_ois_swap_daily_rates` | Rust の `overnight_days` も同じ名前にした |
| `ois_swap_quote_sensitivity` | `compute_ois_swap_quote_sensitivity` | |
| Rust `overnight_coupon_amount` | `compute_overnight_coupon_amount` | R には出していない |

**`get_ois_swap_cashflows` の注意**：固定レグの額は保存済みの条件から決まるが、翌日物レグの額はカーブと Fixing から算出する。実際の処理だけで見れば `compute_` に当たる。指定の例に従って `get_` にしたので、判断をお願いしたい。

## R の公開関数

### 名詞だけのもの：オブジェクトの作成

| 現在 | 案 |
|---|---|
| `fixed_rate_bond` | `create_fixed_rate_bond` |
| `credit_default_swap` | `create_credit_default_swap` |
| `hazard_curve` | `create_hazard_curve` |
| `ois_instruments` | `create_ois_instruments` |

### 取得（保存済みの値）

| 現在 | 案 | 根拠 |
|---|---|---|
| `bond_cashflows` | `get_bond_cashflows` | 債券を作ったときのキャッシュフローを返す |
| `discount_nodes` | `get_discount_nodes` | カーブの節点を返す |
| `quote_repricing` | `get_quote_repricing` | カーブを作ったときの結果を返す |

### 生成

| 現在 | 案 | 根拠 |
|---|---|---|
| `holidays` | `generate_holidays` | 規則から日付列を作る |

### 計算（`value_` と名詞だけのもの）

| 現在 | 案 |
|---|---|
| `value_on_curve` | `compute_bond_value_on_curve` |
| `value_with_zero_spread` | `compute_bond_value_with_zero_spread` |
| `bond_values`、`bond_values_batch` | `compute_bond_values`、`compute_bond_values_batch` |
| `clean_price`、`dirty_price`、`accrued_amount` | `compute_clean_price`、`compute_dirty_price`、`compute_accrued_amount` |
| `duration`、`convexity` | `compute_duration`、`compute_convexity` |
| `basis_point_value`、`yield_value_basis_point` | `compute_basis_point_value`、`compute_yield_value_basis_point` |
| `settlement_date`、`remaining_years` | `compute_settlement_date`、`compute_remaining_years` |
| `yield_from_price`、`z_spread_from_price` | `compute_yield_from_price`、`compute_z_spread_from_price` |
| `price_from_simple_rate`、`simple_rate_from_price` | `compute_price_from_simple_rate`、`compute_simple_rate_from_price` |
| `price_from_compound_yield`、`compound_yield_from_price` | `compute_price_from_compound_yield`、`compute_compound_yield_from_price` |
| `risk_from_simple_rate`、`risk_from_compound_yield` | `compute_risk_from_simple_rate`、`compute_risk_from_compound_yield` |
| `yield_sensitivity`、`curve_node_sensitivity`、`quote_sensitivity` | `compute_yield_sensitivity`、`compute_curve_node_sensitivity`、`compute_quote_sensitivity` |
| `discount`、`zero_rate`、`forward_rate` | `compute_discount`、`compute_zero_rate`、`compute_forward_rate` |
| `curve_values`、`curve_values_batch` | `compute_curve_values`、`compute_curve_values_batch` |
| `ois_asset_swap`、`yen_asset_swap` | `compute_ois_asset_swap`、`compute_yen_asset_swap` |
| `asset_swap_quote_sensitivity` | `compute_asset_swap_quote_sensitivity` |
| `future_basis`、`future_basket` | `compute_future_basis`、`compute_future_basket` |
| `european_option`、`barrier_option`、`binomial_option`、`forward_option` | `compute_european_option_value`、`compute_barrier_option_value`、`compute_binomial_option_value`、`compute_forward_option_value` |
| `monte_carlo_european` | `compute_monte_carlo_european_value` |
| `implied_volatility` | `compute_implied_volatility` |
| `cds_value`、`cds_maturity`、`cds_quote_sensitivity` | `compute_cds_value`、`compute_cds_maturity`、`compute_cds_quote_sensitivity` |
| `survival_probability` | `compute_survival_probability` |
| `hull_white_bond_option`、`hull_white_swaption` | `compute_hull_white_bond_option_value`、`compute_hull_white_swaption_value` |
| `hull_white_bermudan_swaption`、`hull_white_callable_bond` | `compute_hull_white_bermudan_swaption_value`、`compute_hull_white_callable_bond_value` |
| `sabr_volatility` | `compute_sabr_volatility` |
| `prdc_value` | `compute_prdc_value` |

### 要判断：動詞だが、採用する接頭辞にないもの

| 現在 | 案 | 論点 |
|---|---|---|
| `build_ois_curve` | `create_ois_curve` | 気配からカーブを作る。ブートストラップの計算を含む |
| `bootstrap_hazard_curve` | `create_bootstrapped_hazard_curve` | `hazard_curve` の案と区別が要る |
| `calibrate_sabr` | そのまま、または `compute_sabr_calibration` | 「校正」は動詞として明確 |
| `with_extrapolation` | `create_curve_with_extrapolation` | 新しいカーブを返す |
| `is_business_day` | そのまま | 真偽を返す述語で、`is_` は R でも Rust でも慣用 |

## Rust の公開関数

R と同じものは、上の案に揃える（`cashflow_analysis::npv` など R に出ていないものも同じ規則）。Rust だけの論点を挙げる。

| 場所 | 現在 | 案・論点 |
|---|---|---|
| `autodiff` | `value_and_gradient`、`value_gradient_hessian` | `value_` で始まる。`compute_value_and_gradient`、`compute_value_gradient_hessian` |
| `batch` | `map_parallel` | 動詞で始まる汎用の道具。そのまま |
| `cashflow_analysis` | `npv`、`npv_on_curve`、`irr`、`duration` など | `compute_` を付ける |
| `cashflows` | `fixed_rate_leg`、`accrued_amount` | `create_fixed_rate_leg`、`compute_accrued_amount` |
| `cds` | `midpoint_value`、`isda_value`、`cds_value`、`bootstrap_hazard`、`cds_sensitivity` | `compute_` を付ける。`bootstrap_hazard` は R の結論に揃える |
| `hull_white` | `jamshidian_swaption`、`callable_bond`、`bermudan_swaption`、`normal_quadrature` | `compute_` を付ける。`normal_quadrature` は `generate_normal_quadrature` |
| `math::linear` | `solve`、`inverse` | 数学の道具。`solve` は動詞。`inverse` は `compute_inverse` |
| `math::normal` | `normal_pdf`、`normal_cdf`、`inverse_normal_cdf` | 数学の慣用名。そのままにするかは要判断 |
| `math::random` | `uniform`、`standard_normal` | `generate_uniform`、`generate_standard_normal` |
| `math::solver1d` | `close`、`close_with`、`close_enough`、`brent`、`newton_safe` など | 比較の述語と解法の名前。そのままにするかは要判断 |
| `names` | `calendar`、`day_count`、`swap_side` など 18 個 | 文字列を型に変換する。`get_calendar` など。全体で揃える |
| `ois` | `tonar`、`estr`、`sofr`、`sonia`、`build_ois_curve` | 指標は `create_tonar_index` など。`build_ois_curve` は R の結論に揃える |
| `options` | `european_value`、`barrier_greeks` など | `compute_` を付ける |
| `prdc` | `prdc_analytic_value`、`prdc_analytic`、`prdc_monte_carlo` | `compute_` を付ける |
| `risk`、`sabr`、`yen_bond`、`asset_swap` | R と同じ | R の案に揃える |
| `time::schedule` | `generate`、`cds_maturity` | `generate` はすでに動詞（再公開は `generate_schedule`）。`cds_maturity` は `compute_cds_maturity` |
| `time::date` など | `is_leap` | 述語。そのまま |

### Rust のメソッド

メソッドは、型が主語になるので、この一覧では扱いを分ける（`curve.discount(d)`、`bond.clean_price(...)`、`date.year()` など）。`new`、`flat` などのコンストラクターも同じである。方針を関数だけに適用するか、メソッドにも広げるかは要判断である。`FixedRateBond::value_on_curve` と `value_with_zero_spread` は `value_` で始まるので、広げる場合は最初の対象になる。

## 改名するときの影響

R の改名は既存の利用者のコードを壊す。配布版の更新（0.3.0）の前にまとめて行えば、配布済みの 0.2.0 との差が 1 回で済む。比較資料、`quickstart.R`、テスト、`tools/package/smoke_test.R`、ヘルプも同時に変える必要がある。比較先のライブラリー（QuantLib、GaussQuant）の関数名は変えない。
