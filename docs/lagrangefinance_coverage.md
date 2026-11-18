# LagrangeFinance の計算の取り込み状況（2026-11-07）

LagrangeFinance（コミット `05e1932`）の第II部と第III部の各節について、R のコードが行う計算が rmql に入っているかを確かめた。調べ方は次のとおり。

- 各節のコードブロックが呼ぶ関数（QuantLib、GaussQuant、torch、章の中で定義した関数）を抜き出した。
- それを rmql の R の公開関数（123 個）と Rust の公開関数に照らした。

判定は次の 4 つ。

| 判定 | 意味 |
|---|---|
| 対応 | 同じ計算を rmql の関数で行える |
| 一部 | 計算の中心はあるが、節の表や条件の一部がない |
| 未対応 | rmql にない |
| 対象外 | 説明だけでライブラリの計算がない節、または方針で外したもの（EONIA など） |

`comparison/chapters/appendix_c_gaps.qmd` は 2026-11-02 時点の一覧で、その後に実装したものは反映されていない。ゼロカーブ・割引係数カーブ、LMM、FMM、PRDC の確率金利・コール・ターゲット償還・ノックアウトなどがそれにあたる。

## 1. 節ごとの判定

### 第II部

| 節 | 計算 | rmql | 判定 |
|---|---|---|---|
| II-1.1〜1.2 | パーレート・ディスカウントファクター・スポット・フォワード、TONAR の三曲線 | `bootstrap_ois_curve`、`compute_discount`、`compute_zero_rate`、`compute_forward_rate` | 対応 |
| II-1.3 | 固定利付債、利払スケジュール、キャッシュフロー表 | `create_fixed_rate_bond`、`generate_schedule`、`get_bond_cashflows` | 対応 |
| II-1.4〜1.5 | 単利・複利利回りと単価、経過利子、フラットイールド・連続複利による評価 | `compute_price_from_simple_rate` ほか、`compute_bond_values` | 対応 |
| II-1.6 | カーブによる評価、Z スプレッド、アセットスワップ | `compute_bond_value_on_curve`、`compute_z_spread_from_price`、`compute_ois_asset_swap` | 対応 |
| II-1.7 | デュレーション、修正デュレーション、コンベクシティ、BPV | `compute_duration`、`compute_convexity`、`compute_basis_point_value`、`compute_risk_from_compound_yield` | 対応 |
| II-1.8 | 複利利回りの数値解法（ニュートン、二分法、Brent、多項式の根）と速度比較 | 利回りの計算は `compute_compound_yield_from_price` で行える。解法を選んで比べる関数はない | 一部 |
| II-2.1 | 国債利回りのスプライン補間と年限・時間方向の変化 | R の `spline`。ライブラリの計算ではない | 対象外 |
| II-2.2〜2.3 | ゼロカーブ、割引係数カーブ、補間とフォワード、参照日と評価日 | `create_zero_curve`、`create_discount_curve` | 対応（参照日が動くカーブは設計上の違い） |
| II-2.4.1 | 預金と債券によるブートストラップ | `bootstrap_bond_curve` は債券だけ。預金のヘルパーがない | 一部 |
| II-2.4.2 | 預金と IBOR スワップによるブートストラップ | なし | 未対応 |
| II-2.5.1 | Implied term structure（参照日を移したカーブ） | なし | 未対応 |
| II-2.5.2〜2.5.3 | カーブの平行シフト、検証 | `create_zero_spreaded_curve`、`compute_discount` | 対応 |
| II-2.6〜2.7 | 国債先物の受渡適格銘柄、Gross・Net Basis、CTD、6% 標準物、8% 複利での再評価 | `compute_future_basket`、`compute_future_basis` | 対応 |
| II-3.1〜3.3 | TONA の OIS カーブ、短期 OIS、フェアレート、日次複利、Fixing | `bootstrap_ois_curve`、`create_ois_swap`、`compute_ois_swap_value`、`compute_ois_swap_daily_rates` | 対応 |
| II-3.4.1〜3.4.3 | SOFR の OIS カーブ、マルチカーブ評価 | `compute_ois_swap_value` の `discount_curve` | 対応 |
| II-3.4.4 | IBOR のバニラスワップとシングルカーブとの比較 | なし | 未対応 |
| II-3.5〜3.6 | EONIA カーブ、シナリオ別の再評価 | OIS は TONAR・€STR・SOFR・SONIA に限る（決定事項） | 対象外 |
| II-3.7 | 4 通貨 OIS カーブ、OIS アセットスワップ、円ベーシス込み ASW、担保 CTD | `bootstrap_ois_curve`、`compute_ois_asset_swap`、`compute_yen_asset_swap`、`compute_ctd_time_value` | 対応 |
| II-4.1〜4.3 | Black76 と BSM の価格・グリークス、二項ツリー | `compute_forward_option_value`、`compute_european_option_value`、`compute_binomial_option_value` | 対応 |
| II-4.4 | モンテカルロ評価とパスの表 | `compute_monte_carlo_european_value`、`generate_black_scholes_paths` | 対応 |
| II-4.5 | アメリカン put（二項、BAW、Bjerksund–Stensland） | `compute_binomial_option_value`、`compute_american_approximation_value` | 対応 |
| II-4.6 | Sobol 系列と Longstaff–Schwartz | `generate_sobol_sequence`、`compute_longstaff_schwartz_value` | 対応 |
| II-5.1〜5.4 | 生存確率、フラットハザード、MidPoint、Premium・Protection Leg、Fair Spread、ハザードの推定と感応度 | `create_hazard_curve`、`compute_cds_value`、`bootstrap_hazard_curve`、`compute_cds_quote_sensitivity` | 対応 |
| II-5.2.5、5.6.4 | CDS のキャッシュフロー表と確率表 | 値と各レグは返すが、支払日ごとの表は返さない | 一部 |
| II-5.5〜5.6 | 標準 CDS の満期・スケジュール、ISDA エンジン、Accrual rebate、Fair Upfront、MidPoint との比較 | `compute_cds_maturity`、`compute_cds_value`（`"isda"`） | 対応 |
| II-6.2〜6.4 | Normal ボラティリティ、SABR の校正、Hull–White の債券オプションと校正 | `compute_forward_option_value`（Bachelier）、`calibrate_sabr`、`compute_hull_white_bond_option_value`、`calibrate_hull_white` | 対応 |
| II-6.5 | BGM のキャップレットと相関の適合（Rebonato の近似） | `compute_rebonato_swaption_volatility`、`calibrate_lmm_correlation` | 対応 |
| II-7.2 | Hull–White の状態遷移（厳密な OU）と割引係数の確認（年 24 ステップ、平坦なカーブ） | `generate_hull_white_monthly_rates` は月次の短期金利だけ。割引係数の検証の表はない | 一部 |
| II-7.3 | HJM | 説明だけでコードがない | 対象外 |
| II-7.4〜7.5 | LMM のキャップレット・相関ショック、後決め金利と FMM | `compute_lmm_caplet_value`、`compute_caplet_from_normals`、`compute_backward_caplet_value` ほか | 対応 |
| II-8.1〜8.7 | 満期債と発行体コール、Hull–White のバックワード・インダクション、コール判定、Bermudan | `compute_hull_white_callable_bond_value`、`compute_hull_white_callable_bond_decisions`、`compute_hull_white_bermudan_swaption_value` | 対応（行使判断の表は `compute_hull_white_callable_bond_decisions`、2026-11-19） |
| II-8.8 | MBS のプリペイメント、PSA 別の OAS | `compute_mbs_cashflows`、`compute_mbs_path_values`、`compute_mbs_oas` | 対応 |
| II-9.1〜9.3.3 | PRDC の条件、クーポン日、カーブ、FX 過程、クーポン、MC の現在価値 | `generate_schedule`、`compute_prdc_value` | 対応 |
| II-9.3.4、9.4 | 期待クーポンとキャッシュフロー、期待クーポンの期間構造、キャップ・フロアへの到達確率、クーポンの分布 | 返さない | 未対応 |
| II-9.5 | FX スポット・ボラティリティの感応度、MC の標準誤差 | `compute_prdc_value` の `fx_delta`、`fx_vega`、`standard_error` | 対応 |

### 第III部

| 節 | 計算 | rmql | 判定 |
|---|---|---|---|
| III-1〜2 | R の並列計算（future・furrr）、ジョブとタスクの分割 | Rust の並列バッチ（`compute_bond_values_batch`、`compute_curve_values_batch`）。R 側の並列化は R の機能 | 対応（R の並列化そのものは対象外） |
| III-3.4〜3.5 | シナリオの格子で PRDC の近似式を並列に評価、集計 | PRDC のシナリオを一括で評価する関数はない（R から呼び直せば可能） | 一部 |
| III-4.4 | torch による一般の関数の自動微分 | 自動微分はライブラリの計算の内部だけ | 対象外（方針：torch ではなく自前の自動微分） |
| III-5.2〜5.3 | ブラック・ショールズの解析解と、モンテカルロのグリークスを自動微分で | `compute_european_option_value`、`compute_monte_carlo_european_value`（デルタ、ガンマ、ベガ、ロー、シータ） | 対応 |
| III-5.4 | 微分できないペイオフを平滑化して自動微分 | 平滑化の仕組みはない | 未対応 |
| III-6.1〜6.3 | 金利スワップ（固定対変動、フラット・ゼロカーブ）の評価と DV01 を自動微分で | OIS スワップの値と気配の感応度（`compute_ois_swap_quote_sensitivity`） | 一部（ゼロカーブの節点に対するスワップの感応度はない） |
| III-6.4 | バケット DV01 と GPS（カーブの各節点の金利に対する感応度） | 債券は `compute_curve_node_sensitivity`、OIS スワップは気配に対して | 一部 |
| III-7 | PRDC の多因子モンテカルロと、FX・ボラティリティ・カーブ節点の感応度 | `compute_prdc_value`（`curve_sensitivity`）、`compute_prdc_three_factor_value` | 対応 |

## 2. 未対応と一部の一覧（優先順の案）

ライブラリの計算として足すと LagrangeFinance の節がそのまま再現できるもの。

1. **PRDC の期待クーポン・到達確率・分布**（II-9.3.4、9.4）。1 章の簡略モデルでは閉じた式で出せる。期待クーポンは各コールの値で、キャップ・フロアへの到達確率は `N(d2)`。分布は経路から出る。
2. **CDS の支払日ごとの表**（II-5.2.5、5.6.4）。既存の計算の中間値を返すだけでよい。
3. **ゼロカーブの節点に対するスワップの感応度、バケット DV01・GPS**（III-6）。債券と同じ仕組みを OIS スワップに広げる。
4. **Hull–White の経路と割引係数の検証**（II-7.2.2）。3 因子 PRDC の経路の仕組みが使える。
5. **Implied term structure**（II-2.5.1）。参照日を移したカーブ。
6. **預金のヘルパー**（II-2.4.1）。債券カーブのブートストラップに預金を加える。
7. **微分できないペイオフの平滑化**（III-5.4）。
8. **IBOR のバニラスワップとスワップカーブ**（II-2.4.2、II-3.4.4）。決定事項（OIS は 4 指標）の範囲外なので、入れるかはユーザーの判断。

**対象外のまま**：

- II-2.1（R のスプライン）
- II-3.5〜3.6（EONIA）
- II-7.3（HJM、説明のみ）
- III-4.4（torch による一般の自動微分）
- II-1.8 の解法の比較（教材としての比較で、利回りの計算そのものは対応済み）
