# QuantLib と finmath-lib の比較（本体ソース）

Rust による金融計算ライブラリー Riemann の設計調査として、二つのライブラリーの本体ソースを比較した。
Python／R のバインディングや教材の掲載範囲は対象外である。

## 比較対象

| 項目 | QuantLib | finmath-lib |
|---|---|---|
| リポジトリ | [lballabio/QuantLib](https://github.com/lballabio/QuantLib) | [finmath/finmath-lib](https://github.com/finmath/finmath-lib) |
| コミット | `70f240f29ddb84927ff654502f1d75b3de24ba43`（2026-10-29） | `9bd054ff694026760e46371ef61b96a4dbd7b8b0`（2026-08-29） |
| バージョン | 1.45-dev（`ql/version.hpp`） | 6.1.10-SNAPSHOT（`pom.xml`） |
| 言語 | C++ | Java |
| ライセンス | 修正 BSD（`LICENSE.TXT`） | Apache License 2.0（`LICENSE.txt`、`NOTICE.txt`） |
| 調べた範囲 | `ql/` 以下（ヘッダー 1,455 本）、`Examples/` | `src/main/java/net/finmath/` 以下（999 ファイル） |

**調べ方**
- 両リポジトリを上記コミットで取得し、クラス・インターフェースの宣言とファイル名を検索した。コードは実行していない。
- テストコード（finmath-lib の `src/test/`、QuantLib の `test-suite/`）は、商品の有無の判定には使っていない。
- **公式ドキュメント（QuantLib の Doxygen リファレンス、finmath-lib の Javadoc サイト）は今回は未確認**である。根拠はソースと、ソース中のドキュメントコメントだけである。

**凡例**
- **標準**：その商品・機能を表す標準のクラスがある。
- **汎用**：専用のクラスはないが、汎用の部品を組み合わせて作れる。
- **サンプルのみ**：公式の `Examples/` などにしかない。
- **未確認**：今回の調査では判断できなかった。
- **なし**：表の最後に示す検索範囲で見つからなかった。

## 1. 対応商品

| 機能・商品 | QuantLib | finmath-lib | 主な違い |
|---|---|---|---|
| 固定利付債 | 標準：[`FixedRateBond`][ql-frb] | 標準：[`marketdata/model/bond/Bond`][fm-bond]（解析評価）、MC 用の `montecarlo/interestrate/products/Bond` | finmath の `Bond` は、割引カーブ・生存確率カーブ・ベーシスを名前で指定する |
| 変動利付債（FRN） | 標準：[`FloatingRateBond`][ql-frn]、`AmortizingFloatingRateBond` | 汎用：`Bond` にフォワードカーブ名を渡す（`forwardCurveName`） | finmath には FRN 専用のクラスがない |
| ゼロクーポン債 | 標準：[`ZeroCouponBond`][ql-zcb] | 汎用：`Bond` でクーポンなし | |
| 債券フォワード・国債先物 | 標準：[`BondForward`][ql-bondfwd]。先物の受渡適格銘柄・CTD のクラスはなし | なし | どちらも Cheapest to Deliver の専用機能はない |
| 転換社債（CB） | 標準：[`ConvertibleFixedCouponBond` など][ql-cb]、[`BinomialConvertibleEngine`][ql-cbeng] | なし | QuantLib だけにある |
| コーラブル債 | 標準：[`CallableFixedRateBond`][ql-callable]（`experimental`） | 未確認：専用クラスはない（クラス宣言 `CallableBond` を検索して0件）。バミューダン行使の部品（`BermudanSwaption`）で組めるかは確かめていない | QuantLib のものは `experimental` に置かれている |
| OIS | 標準：[`OvernightIndexedSwap`][ql-ois]、[`MakeOIS`][ql-makeois]、[`OvernightIndexedCoupon`][ql-oiscpn]（lookback・lockout・observation shift） | 汎用：[`Swap`][fm-swap]／[`SwapLeg`][fm-swapleg] と [`ForwardCurveFromDiscountCurve`][fm-fwdfromdisc] | finmath には日次複利の OIS クーポンも RFR 指標もない。割引カーブから期間金利を求める形になる |
| RFR 指標（€STR・SOFR・TONAR・SONIA） | 標準：[`Estr`][ql-estr]、[`Sofr`][ql-sofr]、[`Tonar`][ql-tonar]、[`Sonia`][ql-sonia] | なし | 名前の一致が5件あったが、いずれも `ADI` などの部分一致だった |
| 通貨スワップ（CCS） | 標準：[`CrossCurrencySwap`][ql-ccs]、[`ConstNotionalCrossCurrencySwap`][ql-cnccs] | 汎用：MC のハイブリッドモデル [`CrossCurrencyLIBORMarketModelFromModels`][fm-xccylmm] | finmath には解析評価の CCS 商品がない |
| FX フォワード | 標準：[`FxForward`][ql-fxfwd] | なし | |
| CDS | 標準：[`CreditDefaultSwap`][ql-cds]、[`IsdaCdsEngine`][ql-isda]、`MidPointCdsEngine` | 標準：[`marketdata/model/cds/CDS`][fm-cds]（固定・変動フィー、アップフロント） | ハザードのブートストラップは QuantLib に標準（[`PiecewiseDefaultCurve`][ql-pdc]）。finmath は未確認 |
| キャップ・フロア | 標準：[`CapFloor`][ql-capfloor]、オプションレットの抽出 | 標準：[`marketdata/products/Cap`][fm-cap]、MC の [`Caplet`][fm-caplet]、`CapletVolatilities` | |
| スワップション | 標準：[`Swaption`][ql-swaption]（Black・Bachelier・Jamshidian・ツリー・Gaussian1d など多数のエンジン） | 標準：MC の [`Swaption`][fm-swaption]、`SwaptionSimple`、`SwaptionAnalyticApproximation`、FD 版 | |
| バミューダン・スワップション | 標準：`Swaption` に [`BermudanExercise`][ql-exercise] | 標準：[`BermudanSwaption`][fm-bermswpt]（MC と回帰） | QuantLib は行使条件を商品とは別のオブジェクトで渡す |
| CMS | 標準：[`CmsCoupon`][ql-cms]、CMS スプレッドのクーポン | 標準：[`CMSOption`][fm-cmsopt]（MC） | |
| MBS（期限前償還） | なし | なし | どちらにもない |
| 物価連動 | 標準：[`CPIBond`][ql-cpibond]、ゼロクーポン・YoY のインフレスワップ | なし（季節性カーブ [`SeasonalCurve`][fm-seasonal] はある） | |
| 株式ヨーロピアン | 標準：[`VanillaOption`][ql-vanilla] と各エンジン | 標準：MC・FD・フーリエ・ツリーそれぞれに `EuropeanOption`（[MC 版][fm-euro]） | finmath は評価手法ごとに別の商品クラスを持つ |
| アメリカン・バミューダン | 標準：`VanillaOption` に `AmericanExercise`／`BermudanExercise` | 標準：FD の [`AmericanOption`][fm-fdamerican]、MC の [`BermudanOption`][fm-bermopt] | |
| バリア | 標準：[`BarrierOption`][ql-barrier]、`DoubleBarrierOption` | 標準：FD の [`BarrierOption`][fm-fdbarrier] など（Black-Scholes・Heston・SABR・CEV の境界条件） | |
| アジアン | 標準：[`AsianOption`][ql-asian] | 標準：MC の [`AsianOption`][fm-asian]、FD・ツリー版 | |
| バスケット | 標準：[`BasketOption`][ql-basket] | 標準：MC の [`BasketOption`][fm-basket]、FD 版 | |
| FX オプション・クオント | 標準：`VanillaOption` に Garman-Kohlhagen 過程、[`QuantoVanillaOption`][ql-quanto] | 未確認：FX 専用のクラスはない（クラス宣言 `Quanto\|FXOption` を検索して0件）。`BlackScholesModelWithCurves` で外貨金利を表せるかは確かめていない | |
| バリアンス・スワップ | 標準：[`VarianceSwap`][ql-varswap] | なし | |
| XVA | 標準：[`CounterpartyAdjSwapEngine`][ql-cva]。CVA の例は [`Examples/CVAIRS`][ql-ex] | 汎用：MC の [`ExposureEstimator`][fm-exposure] | どちらも CVA 一式の枠組みはない |

## 2. モデルと評価手法

| 機能 | QuantLib | finmath-lib | 主な違い |
|---|---|---|---|
| 解析式 | 標準：`AnalyticEuropeanEngine` など多数、[`AnalyticHestonEngine`][ql-heston-eng] | 標準：[`AnalyticFormulas`][fm-formulas]、フーリエ法（Heston・Bates・Merton・VG） | finmath は解析式を static 関数として持つ |
| ツリー | 標準：[`ql/methods/lattices`][ql-lattices]（株式と短期金利） | 標準：[`tree`][fm-tree]（CRR・Jarrow-Rudd・Boyle 三項）。株式のみ | |
| PDE（差分法） | 標準：[`ql/methods/finitedifferences`][ql-fd]（Fdm の枠組み、多次元、Heston・HW・G2） | 標準：[`finitedifference`][fm-fd]（θ法、ADI、射影 SOR） | |
| モンテカルロ | 標準：[`ql/methods/montecarlo`][ql-mc]、LMM（[`ql/models/marketmodels`][ql-mm]） | 標準：`montecarlo` の中核（[`RandomVariable`][fm-rv] はパス方向のベクトル）、LMM・HW・ハイブリッド | finmath の MC は、確率変数をベクトルとして演算する設計である |
| 早期行使 | 標準：[`LongstaffSchwartzPathPricer`][ql-lsm]、ツリー・FD | 標準：[`MonteCarloConditionalExpectationRegression`][fm-regression]、FD の [`ProjectedTridiagonalSOR`][fm-psor] | |
| キャリブレーション | 標準：[`CalibrationHelper`][ql-calhelper]、[`CalibratedModel`][ql-calmodel]、[`LevenbergMarquardt`][ql-lm]、カーブは [`PiecewiseYieldCurve`][ql-pyc] によるブートストラップ | 標準：カーブは [`CalibratedCurves`][fm-calcurves]（複数カーブの同時ソルバー）、[`LevenbergMarquardt`][fm-lm]・CMA-ES、フーリエ法の [`CalibratedModel`][fm-fcal]、AD を使う LM | QuantLib のカーブは逐次のブートストラップ、finmath はソルバーで同時に解く |
| 金利モデル | 標準：Hull-White（[`HullWhite`][ql-hw]）、[`Gsr`][ql-gsr]、G2、LMM | 標準：[`HullWhiteModel`][fm-hw]、[`LIBORMarketModelFromCovarianceModel`][fm-lmm] | IBOR の LMM の部品も、汎用の計算手法として比較に含めた |
| 確率ボラティリティ | 標準：[`HestonModel`][ql-heston]、SABR（[`sabr.hpp`][ql-sabr]） | 標準：MC の [`HestonModel`][fm-heston]、フーリエ法の Heston、SABR の FD 境界 | |

## 3. カーブ

| 機能 | QuantLib | finmath-lib | 主な違い |
|---|---|---|---|
| OIS カーブ | 標準：[`OISRateHelper`][ql-oishelper] と `PiecewiseYieldCurve` | 汎用：[`DiscountCurveInterpolation`][fm-discinterp] と `CalibratedCurves` に OIS 型のスワップを指定する | |
| マルチカーブ | 標準：予測用と割引用の `Handle` を別々に渡す | 標準：[`AnalyticModel`][fm-model] の中でカーブを名前で参照する（`SwapLeg` の `forwardCurveName`／`discountCurveName`） | finmath はカーブの参照が名前で明示される |
| 通貨ベーシス | 標準：`FxSwapRateHelper`、[`ConstNotional` と `MtM` の CCBS ヘルパー][ql-ccbs] | 汎用：MC のハイブリッドモデルだけ。解析評価のベーシス・ヘルパーはない | |
| 担保 | 汎用：割引カーブの選び方で表す。ヘルパーに `collateralCurve` 引数がある | 汎用：[`DiscountCurve`][fm-disc] の説明に「通貨と担保ごとの割引カーブ」とあり、カーブ名で区別する | どちらにも CSA オブジェクトや担保選択（Cheapest to Deposit）の機能はない |

## 4. グリークス

| 機能 | QuantLib | finmath-lib | 主な違い |
|---|---|---|---|
| 解析グリークス | 標準：エンジンの結果として [`OneAssetOption::delta()` など][ql-oneasset] | 標準：`AnalyticFormulas` の delta・vega など | |
| 数値差分 | 汎用：`SimpleQuote` を動かすと、オブザーバー経由で再計算される | 汎用：`AnalyticModel.addCurve` や `getCloneWithModifiedData` で変更した複製を作り、評価し直す | QuantLib は状態を書き換える、finmath は複製を作る |
| 自動微分 | なし（`ql/` で `AAD`・`adjoint`・`xad` を検索して0件）。ただし実数型が [`QL_REAL`][ql-types] で差し替えられる | 標準：後退型の [`RandomVariableDifferentiableAAD`][fm-aad]、前進型の [`RandomVariableDifferentiableAD`][fm-ad] | QuantLib を XAD で使う外部プロジェクトは今回の調査範囲外（未確認） |
| モンテカルロ感応度 | 標準：LMM の pathwise グリークス（[`ql/models/marketmodels/pathwisegreeks`][ql-pathwise]） | 標準：pathwise の [`EuropeanOptionDeltaPathwise`][fm-pathwise]、尤度比の `EuropeanOptionDeltaLikelihood`、AAD | |

## 5. 設計

| 項目 | QuantLib | finmath-lib | 主な違い |
|---|---|---|---|
| 商品・モデル・エンジンの分離 | 商品は [`Instrument`][ql-instrument]、評価手法は [`PricingEngine`][ql-engine]。エンジンを差し替えられる | 商品は [`Product.getValue(evaluationTime, model)`][fm-product]。商品の記述（`DescribedProduct`）からモデル別の実装を作る [`ProductFactory`][fm-productfactory] もある | QuantLib はエンジンを外から付け替え、finmath は商品が自分を評価する |
| 評価日の扱い | グローバルなシングルトン：[`Settings`][ql-settings] の `evaluationDate`、Fixing は [`IndexManager`][ql-indexmanager] | グローバル状態はない。評価時点は `getValue` の引数で、カーブは基準日（`getReferenceDate`）を持つ | Rust 版で特に見直したい点である |
| 再計算 | [`Observer`/`Observable`][ql-observable]、[`LazyObject`][ql-lazy]、[`Handle`][ql-handle] の付け替え | 変更しない値（immutable）の複製を作り直す | |
| キャッシュフロー | [`Leg = vector<shared_ptr<CashFlow>>`][ql-cashflow]。クーポンとプライサーを分ける | `SwapLeg` がスケジュールと金額の配列を持つ | |
| 日付・カレンダー | 標準：[`Schedule`][ql-schedule]、カレンダー56種（[`Japan`][ql-japan] など）、日数計算13種 | 標準：[`ScheduleGenerator`][fm-schedule]、カレンダーは TARGET・ロンドン・NY・週末・指定休日のみ、日数計算は多数 | finmath には東京のカレンダーがない |
| 乱数 | 標準：[`MersenneTwister`][ql-mt]、[`Sobol`][ql-sobol]、Halton、xoshiro256**、Burley 2020 Sobol | 標準：[`MersenneTwister`][fm-mt]、[`SobolSequence`][fm-sobol]、Halton、Van der Corput | |
| 並列処理 | 本体は並列化していない。`QL_ENABLE_SESSIONS` でシングルトンをスレッドごとに持つ（[`userconfig.hpp`][ql-userconfig]） | Java の並列ストリームを `Portfolio`・オイラー法・共分散モデルで使う。[`concurrency/FutureWrapper`][fm-future] | |
| 拡張の方法 | 継承：`Instrument`・`PricingEngine`・`TermStructure` のサブクラスを作る | インターフェース（`Product`・`Model`・`Curve`）を実装する | |

## 「なし」と判定した検索範囲

| 項目 | QuantLib の検索 | finmath-lib の検索 |
|---|---|---|
| CB | — | `src/main/java` 全体で `convertible`（大文字小文字を区別しない）0件 |
| MBS | `ql/` 全体で `mortgage\|prepay` 0件 | `src/main/java` 全体で `mortgage\|prepay` 0件 |
| OIS・RFR 指標 | — | クラス宣言 `[A-Za-z]*OIS[A-Za-z]*\|Overnight[A-Za-z]*\|Sofr\|Estr\|Sonia\|Tona` 0件。本文の一致はコメント中の「overnight deposit」などのみ |
| FX フォワード、インフレ、バリアンス・スワップ、国債先物、FRN 専用クラス | — | クラス宣言で `FXForward\|FxForward`、`Inflation\|CPI`、`VarianceSwap`、`BondFuture\|BondForward`、`FloatingRateBond\|FloatingRateNote` を検索して0件 |
| 自動微分 | `ql/` 全体で `\bAAD\b\|adjoint\|\bxad\b` 0件 | — |
| 担保選択 | `ql/` 全体で `cheapest` 0件 | `src/main/java` 全体で `cheapest` 0件 |

[ql-frb]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/instruments/bonds/fixedratebond.hpp
[ql-frn]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/instruments/bonds/floatingratebond.hpp
[ql-zcb]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/instruments/bonds/zerocouponbond.hpp
[ql-bondfwd]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/instruments/bondforward.hpp
[ql-cb]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/instruments/bonds/convertiblebonds.hpp
[ql-cbeng]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/pricingengines/bond/binomialconvertibleengine.hpp
[ql-callable]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/experimental/callablebonds/callablebond.hpp
[ql-ois]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/instruments/overnightindexedswap.hpp
[ql-makeois]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/instruments/makeois.hpp
[ql-oiscpn]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/cashflows/overnightindexedcoupon.hpp
[ql-estr]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/indexes/ibor/estr.hpp
[ql-sofr]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/indexes/ibor/sofr.hpp
[ql-tonar]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/indexes/ibor/tonar.hpp
[ql-sonia]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/indexes/ibor/sonia.hpp
[ql-ccs]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/instruments/crosscurrencyswap.hpp
[ql-cnccs]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/instruments/constnotionalcrosscurrencyswap.hpp
[ql-ccbs]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/experimental/termstructures/crosscurrencyratehelpers.hpp
[ql-fxfwd]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/instruments/fxforward.hpp
[ql-cds]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/instruments/creditdefaultswap.hpp
[ql-isda]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/pricingengines/credit/isdacdsengine.hpp
[ql-pdc]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/termstructures/credit/piecewisedefaultcurve.hpp
[ql-capfloor]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/instruments/capfloor.hpp
[ql-swaption]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/instruments/swaption.hpp
[ql-exercise]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/exercise.hpp
[ql-cms]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/cashflows/cmscoupon.hpp
[ql-cpibond]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/instruments/bonds/cpibond.hpp
[ql-vanilla]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/instruments/vanillaoption.hpp
[ql-barrier]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/instruments/barrieroption.hpp
[ql-asian]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/instruments/asianoption.hpp
[ql-basket]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/instruments/basketoption.hpp
[ql-quanto]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/instruments/quantovanillaoption.hpp
[ql-varswap]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/instruments/varianceswap.hpp
[ql-cva]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/pricingengines/swap/cvaswapengine.hpp
[ql-ex]: https://github.com/lballabio/QuantLib/tree/70f240f29ddb84927ff654502f1d75b3de24ba43/Examples
[ql-heston-eng]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/pricingengines/vanilla/analytichestonengine.hpp
[ql-lattices]: https://github.com/lballabio/QuantLib/tree/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/methods/lattices
[ql-fd]: https://github.com/lballabio/QuantLib/tree/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/methods/finitedifferences
[ql-mc]: https://github.com/lballabio/QuantLib/tree/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/methods/montecarlo
[ql-mm]: https://github.com/lballabio/QuantLib/tree/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/models/marketmodels
[ql-lsm]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/methods/montecarlo/longstaffschwartzpathpricer.hpp
[ql-calhelper]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/models/calibrationhelper.hpp
[ql-calmodel]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/models/model.hpp
[ql-lm]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/math/optimization/levenbergmarquardt.hpp
[ql-pyc]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/termstructures/yield/piecewiseyieldcurve.hpp
[ql-hw]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/models/shortrate/onefactormodels/hullwhite.hpp
[ql-gsr]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/models/shortrate/onefactormodels/gsr.hpp
[ql-heston]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/models/equity/hestonmodel.hpp
[ql-sabr]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/termstructures/volatility/sabr.hpp
[ql-oishelper]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/termstructures/yield/oisratehelper.hpp
[ql-oneasset]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/instruments/oneassetoption.hpp
[ql-types]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/types.hpp
[ql-pathwise]: https://github.com/lballabio/QuantLib/tree/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/models/marketmodels/pathwisegreeks
[ql-instrument]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/instrument.hpp
[ql-engine]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/pricingengine.hpp
[ql-settings]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/settings.hpp
[ql-indexmanager]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/indexes/indexmanager.hpp
[ql-observable]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/patterns/observable.hpp
[ql-lazy]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/patterns/lazyobject.hpp
[ql-handle]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/handle.hpp
[ql-cashflow]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/cashflow.hpp
[ql-schedule]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/time/schedule.hpp
[ql-japan]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/time/calendars/japan.hpp
[ql-mt]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/math/randomnumbers/mt19937uniformrng.hpp
[ql-sobol]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/math/randomnumbers/sobolrsg.hpp
[ql-userconfig]: https://github.com/lballabio/QuantLib/blob/70f240f29ddb84927ff654502f1d75b3de24ba43/ql/userconfig.hpp
[fm-bond]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/marketdata/model/bond/Bond.java
[fm-swap]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/marketdata/products/Swap.java
[fm-swapleg]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/marketdata/products/SwapLeg.java
[fm-fwdfromdisc]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/marketdata/model/curves/ForwardCurveFromDiscountCurve.java
[fm-xccylmm]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/montecarlo/hybridassetinterestrate/CrossCurrencyLIBORMarketModelFromModels.java
[fm-cds]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/marketdata/model/cds/CDS.java
[fm-cap]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/marketdata/products/Cap.java
[fm-caplet]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/montecarlo/interestrate/products/Caplet.java
[fm-swaption]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/montecarlo/interestrate/products/Swaption.java
[fm-bermswpt]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/montecarlo/interestrate/products/BermudanSwaption.java
[fm-cmsopt]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/montecarlo/interestrate/products/CMSOption.java
[fm-seasonal]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/marketdata/model/curves/SeasonalCurve.java
[fm-euro]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/montecarlo/assetderivativevaluation/products/EuropeanOption.java
[fm-fdamerican]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/finitedifference/assetderivativevaluation/products/AmericanOption.java
[fm-bermopt]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/montecarlo/assetderivativevaluation/products/BermudanOption.java
[fm-fdbarrier]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/finitedifference/assetderivativevaluation/products/BarrierOption.java
[fm-asian]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/montecarlo/assetderivativevaluation/products/AsianOption.java
[fm-basket]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/montecarlo/assetderivativevaluation/products/BasketOption.java
[fm-exposure]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/montecarlo/interestrate/products/components/ExposureEstimator.java
[fm-formulas]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/functions/AnalyticFormulas.java
[fm-tree]: https://github.com/finmath/finmath-lib/tree/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/tree
[fm-fd]: https://github.com/finmath/finmath-lib/tree/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/finitedifference
[fm-rv]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/stochastic/RandomVariable.java
[fm-regression]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/montecarlo/conditionalexpectation/MonteCarloConditionalExpectationRegression.java
[fm-psor]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/finitedifference/solvers/ProjectedTridiagonalSOR.java
[fm-calcurves]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/marketdata/calibration/CalibratedCurves.java
[fm-lm]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/optimizer/LevenbergMarquardt.java
[fm-fcal]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/fouriermethod/calibration/CalibratedModel.java
[fm-hw]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/montecarlo/interestrate/models/HullWhiteModel.java
[fm-lmm]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/montecarlo/interestrate/models/LIBORMarketModelFromCovarianceModel.java
[fm-heston]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/montecarlo/assetderivativevaluation/models/HestonModel.java
[fm-discinterp]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/marketdata/model/curves/DiscountCurveInterpolation.java
[fm-model]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/marketdata/model/AnalyticModel.java
[fm-disc]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/marketdata/model/curves/DiscountCurve.java
[fm-aad]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/montecarlo/automaticdifferentiation/backward/RandomVariableDifferentiableAAD.java
[fm-ad]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/montecarlo/automaticdifferentiation/forward/RandomVariableDifferentiableAD.java
[fm-pathwise]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/montecarlo/assetderivativevaluation/products/EuropeanOptionDeltaPathwise.java
[fm-product]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/modelling/Product.java
[fm-productfactory]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/modelling/ProductFactory.java
[fm-schedule]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/time/ScheduleGenerator.java
[fm-mt]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/randomnumbers/MersenneTwister.java
[fm-sobol]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/randomnumbers/SobolSequence.java
[fm-future]: https://github.com/finmath/finmath-lib/blob/9bd054ff694026760e46371ef61b96a4dbd7b8b0/src/main/java/net/finmath/concurrency/FutureWrapper.java
