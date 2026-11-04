# QuantLib Guide と公式 Examples の棚卸し（2026-11-04）

QuantLib Guide（https://www.quantlibguide.com/ 、全 39 ページ、2026-11-04 に取得）と、QuantLib v1.42 の公式 Examples（`Examples/` の 20 例）を読み、各例が使う機能と計算条件を RiemannFinMath の現在の機能（R の公開関数 81 個）と突き合わせた。Guide は特定の QuantLib の版を宣言していない（本文中に 1.32〜1.42 の記述がある）。

**判定**：対応済み／一部対応（不足を明記）／未対応／設計上対象外。「設計上対象外」は次の二つである。

- **IBOR の予測カーブと IBOR 商品**（Euribor、USD LIBOR、FRA、IBOR スワップ、IBOR 変動利付債）：金利は OIS に限る方針（`CLAUDE.md`）。
- **Observer、Handle、Relinkable、freeze、グローバルな評価日**：RiemannFinMath は値と日付を引数で明示して渡す設計で、通知の仕組みを持たない。同じ計算は、引数を変えて呼び直すことで得られる。

この文書は棚卸しであり、未対応の項目をすべて実装する計画ではない。

## 0. 集計の対象と件数

| 対象 | 件数 | 内訳 |
|---|---|---|
| Guide で取得したページ | 41 | 目次の 39 ページ、表紙、Feedback |
| 　例のないページ | 7 | 章の導入 5（3、15、30、35、38）、表紙（1）、Feedback（41） |
| 　例のあるページ（2 章の表） | 34 | 対応済み 1、一部対応 19、未対応 10、設計上対象外 4 |
| 公式 Examples（3 章の表） | 20 | 一部再現 7、未対応 10、対象外 3（再現済みは、サンプルを作るまで 0） |

ページの番号は 2 章の表の `#` で、取得した順（表紙が 1、Feedback が 41）である。

**廃止済み指標**：LIBOR（USD LIBOR など）、EONIA などの廃止済み金利指標は、再現、商品対応、fixing の管理、専用カーブの構築のいずれも対象外とする（2026-11-04 のユーザーの指示）。過去取引の再現や移行対応も、現時点では行わない。Guide や Examples にこれらが出てきても機能漏れには数えず、「廃止済み指標のため対象外」と記録する。現行の TONA、SOFR、€STR、SONIA を優先する。

**IBOR 系を対象外とする範囲と、LMM・Caplet との関係**：対象外にするのは、IBOR **指数**を前提とする機能である。すなわち IBOR の予測カーブ（IBOR スワップによるブートストラップ）、IBOR の fixing の管理、IBOR に連動するクーポンと商品（FRA、IBOR スワップ、IBOR 変動利付債、IBOR のアセットスワップ）である。P11-6 の Caplet と相関ショック（`compute_caplet_value` など）はこれに含まれない。これらは、利用者が与えた**期間フォワード金利**とボラティリティ、割引係数から計算する汎用の部品で、IBOR の指数・fixing・予測カーブを使わない。LagrangeFinance II-7.4 の「LIBOR マーケットモデル」も、この意味での期間フォワード金利のモデルとして扱った。LMM 全体（共通測度のドリフト、共同経路、校正）と、RFR の後決め金利を扱う FMM は、IBOR とは別の理由（範囲の判断）で未対応のままである。

## 1. 横断的な機能

| 機能 | Guide・Examples での使われ方 | RiemannFinMath | 判定 | 不足 |
|---|---|---|---|---|
| 日付と期間の演算 | `Date + Period`、月末の扱い、Period の比較 | Rust の内部にある。R では R の日付を使う | 一部対応 | 期間の演算を R に公開していない |
| カレンダー | `isHoliday`、`isBusinessDay`、`adjust`、`advance`、`endOfMonth`、`addHoliday`、`JointCalendar`、`BespokeCalendar` | `is_business_day`、`generate_holidays`、`compute_adjusted_date`、`compute_advanced_date`、`compute_end_of_month`（約 70 のカレンダー） | 一部対応 | 休日の追加、結合カレンダー、自作カレンダー |
| スケジュール | `MakeSchedule`（前進・後退、月末、短期・長期の初回、CDS2015） | `generate_schedule`（同じ規則）と、債券・OIS スワップ・CDS の内部 | 対応済み | |
| 日数計算 | 30/360 の各種、Act/Act ISMA（参照期間・スケジュールつき）、`yearFraction` | `compute_year_fraction`（参照期間つき、30 種類以上） | 一部対応 | Act/Act ISMA にスケジュール全体を渡す形はない（参照期間は渡せる） |
| 複利・利率の換算 | Simple、Compounded、Continuous、**SimpleThenCompounded**、`InterestRate.equivalentRate` | Simple、Compounded、Continuous、SimpleThenCompounded、CompoundedThenSimple（2026-11-04 に追加） | 一部対応 | 利率の換算を R に公開していない |
| 評価日・参照日 | グローバルな評価日、参照日が動くカーブと固定のカーブ | 評価日と参照日を引数で明示する | 設計上の違い | 評価日を変えた再計算は、引数を変えて呼び直す |
| fixing | `addFixing`、欠けた fixing のエラー、当日の fixing の扱い | OIS スワップの `fixings` 引数 | 一部対応 | IBOR の fixing は対象外。当日 fixing を予測するかの切り替えはない |
| キャッシュフロー表 | `cashflows()`、利率・期間・金額、`CashFlows.npv`、`CashFlows.bps` | `get_bond_cashflows`、`compute_ois_swap_cashflows`、`compute_cashflow_npv`（NPV と BPS、各フローの値） | 対応済み | 償還の BPS は QuantLib が `nan`、ここは 0 |
| 既定のカーブ | `FlatForward`、`ZeroCurve`、`DiscountCurve`、`ZeroSpreadedTermStructure` | `create_flat_curve`、`create_zero_curve`、`create_discount_curve`、`create_zero_spreaded_curve`（2026-11-04 に公開） | 対応済み | 参照日は固定（評価日とともに動かない。設計上の違い） |
| ブートストラップ | OIS、SOFR 先物、債券、IBOR スワップ、ベーシス、クロスカレンシー。補間は LogCubicDiscount、LinearZero、FlatForward、CubicZero | OIS（単調対数 3 次、対数線形） | 一部対応 | 先物、債券のヘルパー、線形ゼロ・フラットフォワード・3 次ゼロの補間、ジャンプがない |
| 外挿 | `enableExtrapolation` | `with_extrapolation` | 対応済み | |
| カーブの加工 | `ZeroSpreaded`、`PiecewiseZeroSpreaded`、`ImpliedTermStructure` | 債券の Z スプレッド計算の内部のみ | 一部対応 | 区分的なスプレッド、参照日を移したカーブがない |
| 感応度 | 気配のバンプ、平行・ツイストのシフト、数値 Greeks、数値 Theta | 自動微分による気配・節点の感応度（債券、OIS スワップ、CDS）、オプションの Greeks | 一部対応 | 任意の形のカーブシフト、Theta の方式の切り替えはない |
| 通知の仕組み | Observer、Handle、Relinkable、freeze | なし | 設計上対象外 | |

## 2. QuantLib Guide の章ごとの対応

| # | ページ | 主な例と機能 | 判定 | 不足 |
|---|---|---|---|---|
| 2 | A taste of QuantLib | 固定利付債の利回り価格・経過利息、ゼロカーブでの価格、スプレッドつきカーブ、Brent で Z スプレッド | 一部対応 | ゼロカーブを R で作れない |
| 4 | Dates and periods | 日付・期間の演算 | 一部対応 | 期間の演算を R に公開していない（R の日付で代替できる例が多い） |
| 5 | Calendars and holidays | TARGET、米国債、調整、前進、休日の追加、結合・自作カレンダー | 一部対応 | 休日の追加、結合・自作カレンダー。それ以外は再現した（`dates.R`） |
| 6 | Schedules | 前進・後退、短期・長期の初回、月末、CDS2015 | 対応済み | 再現した（`dates.R`） |
| 7 | Cash flows and bonds | 固定利付債のキャッシュフロー、利回り、償還元本が変わる債券 | 対応済み | 汎用の Bond の代わりに、`create_fixed_rate_bond` の `notionals` でアモチ債を作る |
| 8 | The global evaluation date | 割引債、評価日の効果、Euribor の fixing | 一部対応 | 割引債の専用の作り方（クーポン 0 の固定利付債で表せるかは未確認）。IBOR は対象外 |
| 9 | Handling dependencies | 割引債を発行体カーブで評価、カーブの差し替え | 一部対応 | R でゼロカーブを作れない。差し替えは呼び直しで代替 |
| 10 | Instruments and pricing engines | European（解析解と Greeks）、Heston、MC | 一部対応 | Heston。評価日を変える再計算は呼び直し |
| 11 | The Observer pattern | 二重カーブの IBOR スワップ（C++） | 設計上対象外 | |
| 12 | Updating multiple market quotes | 債券ヘルパーと Nelson–Siegel のフィット、気配の更新 | 未対応 | 当てはめカーブ、債券ヘルパー |
| 13 | Term structures | フラットカーブ、参照日が動く・固定 | 一部対応 | 参照日が動くカーブ（設計上の違い。新しい日付で作り直す）。Observer は対象外 |
| 14 | Cash-flow analysis | 固定利付債のキャッシュフロー表、`CashFlows.npv`、`bps` | 対応済み | 再現した（`bonds.R`）。`as_coupon` などの型の判定は R の列で代わりに見る |
| 16 | Common interface of term structures | フラット・割引係数カーブ、割引・フォワード、スプレッドつき | 一部対応 | Euribor の予測は対象外。それ以外は再現した（`curves.R`） |
| 17 | Fitted bond curves | Nelson–Siegel、指数スプライン、B スプライン、Svensson | 未対応 | 当てはめカーブ |
| 18 | Curve bootstrapping | SOFR 先物と OIS、IBOR スワップ、ベーシス、ジャンプ | 一部対応 | 先物のヘルパー、ジャンプ。IBOR は対象外 |
| 19 | Cross-currency curves | MtM クロスカレンシーベーシスでのブートストラップ | 未対応 | クロスカレンシーのブートストラップ |
| 20 | Interest-rate indexes | 指数の規約、fixing の登録 | 一部対応 | OIS の fixing は引数で渡す。IBOR は対象外 |
| 21 | Effect of today's fixing | 当日 fixing の有無で IBOR カーブが変わる | 設計上対象外 | |
| 22 | Dangerous day-count conventions | 30/360 の分割、Act/Act ISMA の参照期間 | 一部対応 | スケジュール全体を渡す Act/Act ISMA。それ以外は再現した（`dates.R`） |
| 23 | Vanilla bonds | 固定利付債の価格・利回り・デュレーション・コンベクシティ、評価日の変更、ステップアップ、アモチ、割引債、IBOR 変動利付債、キャップ・フロア、OIS 変動利付債 | 一部対応 | 割引債、OIS 変動利付債。IBOR は対象外。それ以外は再現した（`bonds.R`。SimpleThenCompounded の利回りは QuantLib 1.42 と一致し、Guide とは版の差で 1.1e-6） |
| 24 | Different kinds of swaps | SOFR OIS（fair rate、BPS、varying notional、spread、payment lag、lookback・lockout）、IBOR スワップ、ベーシス | 一部対応 | 想定元本の変化、lookback・lockout・観測シフト、R のゼロカーブ。IBOR は対象外 |
| 25 | Asset swaps | Euribor の par・market アセットスワップ | 設計上対象外 | Riemann は OIS のアセットスワップ（別の規約） |
| 26 | Cross-currency swaps | 一定元本と MtM のクロスカレンシースワップ | 未対応 | Riemann の円建てアセットスワップは別の商品 |
| 27 | Coupons with multiple resets | 複数回のリセットを持つクーポン | 未対応 | |
| 28 | Payment-in-kind bonds | 元本に組み入れるクーポン | 未対応 | |
| 29 | Spread calculations | i スプレッド、Z スプレッド（SOFR ブートストラップ）、利率の換算、コール債の OAS | 一部対応 | 利率の換算、OAS（Hull–White 木のコール債）。i スプレッドと Z スプレッドは再現した（`spreads.R`） |
| 31 | Numerical Greeks | バリアの数値 Greeks、European の差分 | 対応済み | Riemann は自動微分で Greeks を返す。数値差分の比較は可能 |
| 32 | Approaches to numerical Theta | IBOR カーブの Theta の 3 方式 | 設計上対象外 | IBOR。方式の比較自体は OIS で行える |
| 33 | Pricing over a range of days | 日ごとの再評価、過去・将来の fixing | 一部対応 | R のゼロカーブ、OIS 変動利付債。IBOR は対象外 |
| 34 | Assessing duration risk | 債券ヘルパーで米国債カーブ、平行・ツイスト・ストレス | 未対応 | 債券ヘルパー、線形ゼロ補間、区分的スプレッド |
| 36 | Inflation indexes and curves | HICP、ゼロインフレカーブ、季節性、ZCIIS | 未対応 | インフレ全般 |
| 37 | Inflation bonds | CPI 債 | 未対応 | インフレ全般 |
| 39 | Default-probability curves | Upfront・Spread の CDS ヘルパー、MidPoint・ISDA、生存確率カーブ | 一部対応 | デフォルト密度（`defaultDensity`）と `DefaultDensityCurve`。Upfront と Spread のヘルパーの曲線、確認用 CDS、ハザード率、生存確率カーブは再現した（`cds.R`。すべて QuantLib 1.42 と一致。Guide の表示とは版の差：確認用 CDS の NPV が Guide は -0.2777（プレミアム 1 日分）、ここと 1.42 は 0。デフォルト確率は最大 0.009 ポイント差） |
| 40 | Bonds and CDS curves | 信用リスクを含む債券評価、スプレッド換算 | 未対応 | リスキー債券エンジン（Upfront ヘルパーは 2026-11-04 に追加） |

章の導入（3、15、30、35、38）、表紙、Feedback には例がない。

## 3. 公式 Examples（v1.42）の対応

サンプルの判定は「再現済み／一部再現／未対応／対象外」とする。現時点ではサンプルを作っていないので、判定は既存機能で再現できる見込みである。

| Example | 内容 | 見込み | 不足 |
|---|---|---|---|
| Bonds | 預金・債券・IBOR スワップのカーブで割引債・固定利付債・IBOR 変動利付債 | 一部再現 | 債券ヘルパー、預金ヘルパー。IBOR は対象外 |
| CDS | Spread ヘルパーでハザード、MidPoint・ISDA、ISDA 規約の金利カーブ | 一部再現 | ISDA 規約の金利カーブ（預金・IBOR スワップ） |
| EquityOption | European・Bermudan・American を多数のエンジンで | 一部再現 | Barone-Adesi–Whaley、Bjerksund–Stensland、有限差分、QD+、JR・Tian などの木、積分エンジン |
| FRA | Euribor の FRA | 対象外 | IBOR |
| FittedBondCurve | 当てはめカーブ 6 方式 | 未対応 | 当てはめカーブ |
| MulticurveBootstrapping | OIS 割引と Euribor 予測 | 一部再現 | OIS 部分のみ。IBOR は対象外 |
| BermudanSwaption | G2、Hull–White、Black–Karasinski の木・有限差分、校正 | 一部再現 | G2、Black–Karasinski、木・有限差分のエンジン（Riemann は状態格子） |
| CallableBonds | Hull–White の木でコール債 | 一部再現 | QuantLib の木と Riemann の状態格子は方式が違う。債券の規約からの変換 |
| Repo | 債券のフォワード | 未対応 | 債券フォワード |
| ConvertibleBonds | 転換社債 | 未対応 | |
| AsianOption | アジア型 | 未対応 | |
| DiscreteHedging | 離散ヘッジのシミュレーション | 未対応 | |
| Replication | バリアの静的複製 | 一部再現 | European とバリアの解析解はある。複製のポートフォリオは R で組める見込み |
| CVAIRS | CVA つき IRS | 未対応 | IBOR、CVA |
| BasketLosses、LatentModel | ポートフォリオ信用の損失 | 未対応 | |
| Gaussian1dModels | Gaussian1d モデル | 未対応 | |
| MarketModels | LMM 全体 | 未対応 | LMM 全体（P11-6 で範囲外とした） |
| GlobalOptimizer、MultidimIntegral | 最適化・多次元積分の数値手法 | 対象外 | 金融商品の例ではない |

## 4. 対応の優先順位（案）

効果（使えるようになる例の数）と作業量で並べた。

1. **R からカーブを作る関数**（フラット、ゼロ金利、割引係数、ゼロスプレッド）。Rust にはあり、公開するだけで済む。Guide 2、9、13、14、16、23、24、33 と Examples の一部の前提になる。**実装済み**。
2. **任意のキャッシュフローの NPV・BPS、`yearFraction`、カレンダーの `adjust`・`advance`、スケジュールの生成**の公開。Guide 5、6、14、22 が使う。いずれも内部にある。**実装済み**。
3. **SimpleThenCompounded** と、**ステップアップ・アモチ**の固定利付債。Guide 7、23、29 が使う。**実装済み**。
4. **CDS の Upfront ヘルパーと生存確率カーブ**。Guide 39、40 が使う。**実装済み**（`bootstrap_hazard_curve(quote_type = "upfront")`、`create_survival_probability_curve`、`compute_hazard_rate`）。Guide 40 のリスキー債券エンジンは含まない。
5. **債券ヘルパーのブートストラップと線形ゼロ補間**。Guide 34、Examples Bonds が使う。
6. **American オプションの近似式**（Barone-Adesi–Whaley、Bjerksund–Stensland）。Examples EquityOption、LagrangeFinance II-4.5 が使う。

後回しにするもの：当てはめカーブ、OIS 変動利付債、lookback・lockout、先物ヘルパー、Heston、インフレ、クロスカレンシー、転換社債、アジア型、G2、CVA。

## 6. 作ったサンプル

R パッケージの `inst/examples/quantlib_guide/` にある。一覧と主要な結果は同じフォルダーの `README.md`、実行方法は各スクリプトの冒頭にある。インストールしたパッケージからは `source(system.file("examples", "quantlib_guide", "<名前>.R", package = "RiemannFinMath"))` で実行できる。テストは `tests/testthat/test-quantlib-guide-examples.R`。

## 5. 今の機能で作れるサンプル

追加実装なしで、比較と R のサンプルにできる例である。

| 題材 | 出典 | 使う関数 |
|---|---|---|
| 固定利付債のキャッシュフロー、利回り、デュレーション、コンベクシティ、評価日の変更 | Guide 7、14、23（利回りで計算する部分） | `create_fixed_rate_bond`、`get_bond_cashflows`、`compute_bond_values`、`compute_yield_from_price`、`compute_duration`、`compute_convexity` |
| SOFR の OIS カーブでの Z スプレッドと i スプレッド | Guide 29（SimpleThenCompounded の利回りを除く） | `bootstrap_ois_curve`、`compute_z_spread_from_price` |
| European オプションの価格と Greeks、バリアの Greeks | Guide 10（Heston を除く）、31 | `compute_european_option_value`、`compute_barrier_option_value` |
| Spread・Upfront ヘルパーのハザードカーブ、生存確率カーブと MidPoint・ISDA の CDS | Guide 39、Examples CDS（ハザード部分） | `bootstrap_hazard_curve`、`create_survival_probability_curve`、`compute_hazard_rate`、`create_credit_default_swap`、`compute_cds_value` |
| カレンダーの休日と営業日 | Guide 5（`isHoliday`、`isBusinessDay`） | `is_business_day` |
