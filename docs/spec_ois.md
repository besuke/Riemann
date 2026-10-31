# OIS カーブの仕様（P3）

P3 では、O/N 預金と OIS の気配から割引カーブを作る。TONAR の対数線形（P3-1）、`MonotonicLogCubic`（P3-3）、€STR・SOFR・SONIA（P3-2）を実装済みである。OIS の指標はこの 4 つに限定する。計算の方式は QuantLib C++ で定める。R バインディングは照合に使うだけで、Rust の設計を R バインディングに合わせることはしない。

記述の区分（【ソース確認】【実行確認】【実行照合済み】【設計案】）は仕様書と同じ考え方である。【実行確認】は R バインディングで動作を確かめた少数の例で、Riemann との数値照合である【実行照合済み】とは区別する。

| 段階 | 内容 | 状態 |
|---|---|---|
| P3-0 | QuantLib のソース調査と仕様の整理 | 2026-10-31 完了（この文書） |
| P3-1 | TONAR：O/N 預金と OIS、対数線形補間、入力レートの再現、QuantLib との照合 | 2026-10-31 照合済み（8 章） |
| P3-2 | €STR・SOFR・SONIA へ拡張（英国 Exchange カレンダーを含む） | 2026-11-01 照合済み（10 章） |
| P3-3 | TONAR の `MonotonicLogCubic`（反復ループ、単調化フィルター） | 2026-10-31 照合済み（9 章） |

## 1. 参照したもの

| 対象 | 版 | 場所 |
|---|---|---|
| QuantLib（C++）、照合の正 | `v1.42`（タグ `94db321`、コミット `41b0e14`、2026-04-13） | https://github.com/lballabio/QuantLib |
| QuantLib（C++）、差の確認用 | `70f240f`（2026-10-29） | 同上 |
| QuantLib R バインディング | 1.42 | 手元にインストール済みのパッケージ |
| QuantLib-SWIG（照合用 R 関数の対応先の確認だけ） | `v1.42`（コミット `924f814`） | https://github.com/lballabio/QuantLib-SWIG 、`SWIG/piecewiseyieldcurve.i`、`SWIG/interpolation.i` |
| GaussQuant | `c08fdb1` | `R/compat_quantive_riemann_GQH.R`（`ir_build_ois_curve_GQH`、`.ir_get_ois_convention_GQH`）、`tests/testthat/test-ois-asset-swap-GQL.R` |

**基準と照合の区別**：仕様の基準は QuantLib C++ `v1.42` のソースである。実行照合に使う R パッケージは Version 1.42 だが、ビルド日（2026-04-06）が `v1.42` のタグのコミットより前で、正確な C++ のコミットは分からない。そこで、照合結果は「正確な C++ コミットが不明な R パッケージ Version 1.42 との照合」と記録し、C++ `v1.42` そのものとの実行照合とは区別する。C++ `v1.42` をビルドして実行した照合はしていない。

読んだ QuantLib のファイル（`v1.42`。パスはリポジトリのルートから）：

| 内容 | ファイル |
|---|---|
| RateHelper | `ql/termstructures/yield/oisratehelper.{hpp,cpp}`、`ql/termstructures/yield/ratehelpers.{hpp,cpp}`（`DepositRateHelper`）、`ql/termstructures/bootstraphelper.hpp` |
| OIS | `ql/instruments/makeois.{hpp,cpp}`、`ql/instruments/overnightindexedswap.cpp`、`ql/instruments/fixedvsfloatingswap.cpp`、`ql/pricingengines/swap/discountingswapengine.cpp` |
| 日次複利 | `ql/cashflows/overnightindexedcoupon.cpp`、`ql/cashflows/overnightindexedcouponpricer.cpp`、`ql/cashflows/coupon.cpp`、`ql/cashflows/floatingratecoupon.hpp`、`ql/cashflows/cashflows.cpp`（`npvbps`） |
| 指標 | `ql/indexes/iborindex.{hpp,cpp}`、`ql/indexes/interestrateindex.{hpp,cpp}`、`ql/indexes/ibor/{tonar.hpp,estr.cpp,sofr.cpp,sonia.cpp}` |
| ブートストラップ | `ql/termstructures/iterativebootstrap.hpp`、`ql/termstructures/yield/bootstraptraits.hpp`、`ql/termstructures/yield/piecewiseyieldcurve.hpp` |
| 解法 | `ql/math/solver1d.hpp`、`ql/math/solvers1d/brent.hpp`、`ql/math/solvers1d/finitedifferencenewtonsafe.hpp` |
| 補間 | `ql/math/interpolations/loginterpolation.hpp`、`ql/math/interpolations/linearinterpolation.hpp`、`ql/math/interpolations/cubicinterpolation.hpp`（要点のみ。7 章） |
| カレンダー | `ql/time/calendars/unitedkingdom.cpp`、`ql/time/schedule.cpp`（`allowsEndOfMonth`） |
| テスト | `test-suite/overnightindexedswap.cpp`、`test-suite/piecewiseyieldcurve.cpp`（読んだだけで、実行はしていない） |

## 2. QuantLib v1.42 の仕様【ソース確認】

### 2.1 4 つの指標

| 指標 | クラス | fixingDays | カレンダー | 日数計算 | `MakeOIS` の既定の受渡日数 |
|---|---|---|---|---|---|
| TONAR | `Tonar` | 0 | `Japan` | Actual/365 Fixed | 2 |
| €STR | `Estr` | 0 | `TARGET` | Actual/360 | 2 |
| SOFR | `Sofr` | 0 | `UnitedStates(SOFR)` | Actual/360 | 2 |
| SONIA | `Sonia` | 0 | `UnitedKingdom(Exchange)` | Actual/365 Fixed | 0 |

既定の受渡日数は `MakeOIS` のもので、`testMakeOISDefaultSettlementDays` が確かめている。`OISRateHelper` は受渡日数を必須の引数として受け取るので、この既定を使わない。

翌日物の指標は共通で、期間 1 日、営業日調整 Following、月末の扱いなし。`maturityDate(v) = advance(v, 1 Days, Following, false)`。

### 2.2 O/N 預金（`DepositRateHelper(rate, index)`）

- 基準日 = `adjust(評価日)`（Following）。開始日 = `valueDate(基準日)`（fixingDays が 0 なので基準日のまま）。満期 = `maturityDate(開始日)`。ピラー = 最終関連日 = 満期。
- 再現する値：`(discount(d1) / discount(d2) - 1) / t`、t は指標の日数計算による d1 から d2 までの年数。
- 日数計算と営業日調整を個別に渡す別のコンストラクターもある。テストはこちらを使い、GaussQuant の `overnight_deposit` は指標を渡す上の形を使う。

### 2.3 OIS（`OISRateHelper` と `MakeOIS`）

`OISRateHelper` の既定値（市場の規約ではない。2.10 節）：`telescopicValueDates = false`、支払ラグ 0、支払の営業日調整 Following、支払頻度 Annual、ピラー `LastRelevantDate`、平均化 Compound、スケジュール Backward、スケジュールの営業日調整 ModifiedFollowing（終了日も同じ）。スプレッド、lookback、lockout は 0 またはなし。受渡日数に既定はなく、必須の引数である。

| 手順 | 規則 |
|---|---|
| スポット日 | `advance(adjust(評価日), 受渡日数 Days)` |
| 開始日 | スポット日 + `forwardStart`（既定 0）を `adjust`（Following） |
| 月末の扱い | 既定で `isEndOfMonth(開始日)`（カレンダー上の月末営業日か） |
| 終了日 | 開始日 + 期間。月末の扱いがあり、期間が 1 か月以上の月・年単位で、開始日が月末営業日なら、`endOfMonth(終了日)` |
| スケジュール | 固定・変動とも同じ設定：開始日から終了日、1 年ごと、指標のカレンダー、ModifiedFollowing、Backward |
| 固定レグの日数計算 | 指標の日数計算 |
| 支払日 | `advance(計算期間の終了日, 支払ラグ Days, 支払の営業日調整)` |
| ピラー | `max(満期, 最後の支払日, 最後のフィキシングの終了日)` |

### 2.4 日次複利のクーポン

- 値の日付（value dates）は、開始日から終了日まで 1 日ごとのスケジュール（指標のカレンダー、Following、Backward）。フィキシング日は、値の日付の最後を除いたもの（fixingDays が 0 の場合）。
- 複利係数：`∏(1 + r_i dt_i)`、`dt_i` は指標の日数計算による隣り合う日付の間の年数。
- 評価日より前のフィキシングは、過去の値（ない場合はエラー）を使う。評価日当日は、値があれば使い、なければ予測する。
- 将来の部分は、各日を計算せず割引係数の比に置き換える（lookback がない場合）：`discount(v_i) / discount(v_n)`。予測フィキシングが `(discount(v_i) / discount(v_{i+1}) - 1) / dt_i` なので、積は比に一致する。
- クーポンの利率：`(複利係数 - 1) / accruedPeriod(終了日)`。額：`rate × accrualPeriod × nominal`。

**割引係数の比で計算できる条件**（`overnightindexedcouponpricer.cpp` の `compute`）。将来の部分 `∏(1 + F_k dt_k)`（k = i〜n−1）が `P(v_i) / P(v_n)` に等しくなるのは、次がすべて成り立つ場合である。`P` は**指標の予測カーブ**である。

| # | 条件 | 成り立たない例 |
|---|---|---|
| 1 | 各 `F_k` が、同じ予測カーブによる `(P(v_k) / P(v_{k+1}) - 1) / τ_k` で求まる | フィキシングの値が与えられている日 |
| 2 | 予測の期間と利息の期間が同じで、`τ_k = dt_k` | lookback（`canApplyTelescopicFormula` が偽）、指標と異なるカレンダーで終了日が指標の休日になる場合 |
| 3 | lockout がない | lockout（その区間は各日を計算する） |
| 4 | スプレッドを日次で複利しない | `compoundSpreadDaily` |
| 5 | 計算の終点が最後の利息日である | 期間の途中までの経過利息（最後の 1 日は別に計算する） |
| 6 | 予測カーブが `v_i` から `v_n` までの範囲を持つ | 基準日が `v_i` より後のカーブ |

条件と、よく混同される点の関係：

- **予測と割引が同じカーブであることは、この比の条件ではない。** 比に使うのは予測カーブだけで、割引カーブは支払額を現在価値にするときに使う。別のカーブでも比で計算できる。
- **過去のフィキシングがないことも、比の条件ではない。** 過去の部分は積で計算し、残りを比で計算する：`複利係数 = ∏_{過去}(1 + r_k dt_k) × P(v_i) / P(v_n)`。評価日当日のフィキシングは、値があれば積に、なければ比に入る。
- **両方が成り立つ場合（P3 の範囲）**：複利係数は `P(s) / P(e)` だけで決まる。さらに、支払ラグが 0 で、`accruedPeriod` と `accrualPeriod` が同じなら、1 クーポンの現在価値は数式の上で `N (P(s) - P(e))` になる。支払ラグがあれば `N (P(s) / P(e) - 1) P(pay)` のままで、このように簡単にならない。
- **`telescopicValueDates` は別のものである。** これは値の日付の列を短く作る最適化で、既定は false である。比の計算は、false でも上の条件を満たせば行われる。
- **Riemann の計算順**：数式で等しくても浮動小数の結果は変わるので、P3 では QuantLib と同じく `(P(s) / P(e) - 1) / accruedPeriod × accrualPeriod × nominal` の順で計算し、各日の積も `N (P(s) - P(e))` も使わない。

### 2.5 OIS が再現する値（`impliedQuote`）

- `npvbps`（`cashflows.cpp`）：基準日以前の支払を除き、`npv = Σ amount × df`、`bps = Σ nominal × accrualPeriod × df` を足す。`d = discount(基準日)` として、`npv /= d`、`bps = 1e-4 × bps / d`。
- `impliedQuote = -(floatingLegNPV + spreadNPV) / (fixedLegBPS / 1e-4)`。名目 1.0、Payer（固定レグ −1、変動レグ +1）。

### 2.6 ブートストラップ（`IterativeBootstrap`、`Discount`）

| 項目 | 規則 |
|---|---|
| 精度 | 既定 `accuracy = 1e-12`（割引係数に対する精度。`PiecewiseYieldCurve`） |
| ヘルパーの整理 | ピラーで並べ替える。ピラー ≤ 基準日のものは捨てる。すべて捨てられたらエラー |
| エラー | ピラーの重複、最終関連日が前のヘルパーを超えない場合 |
| 節点 | `dates[0] =` 基準日（値 1）、以後ピラー。`times[i] = dc.yearFraction(基準日, dates[i])` |
| 解の範囲 | `min = data[i-1] × exp(-1.0 × dt)`、`max = data[i-1] × exp(1.0 × dt)`（`maxRate = 1.0`） |
| 初期値 | `i = 1` は `1 / (1 + 0.05 t)`（`avgRate = 0.05`）。以後は前の節点のゼロレートで平らに延ばした `exp(-r t_i)`。範囲外なら範囲の端から 1/5 内側へ |
| 解法 | 最初の 1 周は Brent。目的関数は `quote - impliedQuote`。評価は最大 100 回。解を挟めなければエラー（既定は再試行なし） |
| ループ | 補間が局所的（対数線形）で、ピラーが最終関連日より前になければ、1 周で終わる |
| 補間の途中経過 | 節点 i の探索中は、節点 0〜i だけで補間する（局所的な補間でも毎回作り直す） |

対数 3 次（`global = true`）はループが必須になる。2 周目からは `FiniteDifferenceNewtonSafe` で前の周の値を初期値にして解き、`min = min(data) / 2` とする。節点の変化の最大が `accuracy` 以下で終わり、最大 100 周。2 周目以降は、すべての節点で補間する。

**節点に残る値**：`IterativeBootstrap` は解法の戻り値を節点に代入しない。節点に残るのは、目的関数に**最後に渡した値**である。`Brent` は収束時に戻す根でもう一度 `f` を呼ぶので、根と一致する。`FiniteDifferenceNewtonSafe` は収束時に `f` を呼ばないので、戻り値ではなく、最後に評価した値が残る。bisect の分岐で `f(xh)` を呼んだ後に収束した場合は、`xh` が残る。

### 2.7 補間

- 対数線形：`LogLinear`。`exp(線形補間(log df))`。範囲外は端の区間を延ばす。P2-3 の `YieldCurve::discount_factors` と同じ（`docs/spec_bonds.md` 4 章で照合済み）。
- 対数 3 次：**Riemann の仕様は QuantLib C++ の `MonotonicLogCubic` とする**。定義は `LogCubic(CubicInterpolation::Spline, monotonic = true, SecondDerivative 0, SecondDerivative 0)`（`loginterpolation.hpp`）。

**照合用 R 関数の対応先【ソース確認】**：SWIG `v1.42` では、`PiecewiseLogCubicDiscount` は SWIG の `LogCubic` を既定の引数で使う（`piecewiseyieldcurve.i`）。その既定は `Spline, true, SecondDerivative 0, SecondDerivative 0` で（`interpolation.i`）、C++ の `MonotonicLogCubic` と同じ設定である。名前は `MonotonicLogCubic` ではないが、計算は同じになる。

**実行確認**：節点を `MonotonicLogCubicDiscountCurve` と `NaturalLogCubicDiscountCurve` に渡し、節点の間の 2 点を比べた。単調でない気配の例で、前者は差 0、後者は相対 1.9e-3 と 7.1e-3 の差だった。ソースと矛盾しない。

### 2.8 QuantLib のテストから分かること

| テスト | 内容 |
|---|---|
| `testBaseBootstrap` ほか（`overnightindexedswap.cpp`） | €STR のカーブ（Discount、LogLinear）を作り、許容誤差 1e-8 で OIS の気配を再現する。支払ラグは 2 を明示 |
| `testBootstrapRegression` | FedFunds、`MonotonicLogCubic`、支払ラグ 2。コメントに「既定の `LastRelevantDate` では失敗する」とあり、`Pillar::MaturityDate` を使う |
| `testBootstrapWithDifferentCalendars` | SOFR で満期が指標の休日になる場合、最終関連日が満期より後になる（2028-04-14 と 2028-04-17） |
| `testMakeOISDefaultSettlementDays` | 既定の受渡日数と、評価日が休日の場合の開始日（3 章） |
| `testLogLinearDiscountConsistency`（`piecewiseyieldcurve.cpp`） | 対数線形の整合性。許容誤差 1e-9 |
| `testLogCubicDiscountConsistency` | v1.42 ではコメントアウトされている |

このように、対数 3 次とピラーの選択には失敗しうる場合がある。P3-3 では、既定の設定で失敗する入力も照合の対象にする。

### 2.9 実行で確かめた日付【実行確認】

評価日 2026-04-29（日本の休日）、R バインディング 1.42：

| 指標 | O/N 預金 | OIS 1 年、受渡日数 2 |
|---|---|---|
| TONAR | 2026-04-30 → 2026-05-01 | 2026-05-07 → 2027-05-07 |
| €STR | 2026-04-29 → 2026-04-30 | 2026-05-04 → 2027-05-04 |
| SOFR | 2026-04-29 → 2026-04-30 | 2026-05-01 → 2027-05-03 |
| SONIA | 2026-04-29 → 2026-04-30 | 2026-05-01 → 2027-05-04 |

### 2.10 QuantLib の既定と市場の規約

QuantLib の既定は市場の規約ではない。下の「市場の規約」は、次の区分で書く。

- **一次**：中央銀行・作業部会・清算機関が公表した文書の本文を読んで確認した。
- **二次**：短資会社のページや解説記事だけで確認した。
- **未確認**：読める資料で確認できなかった（QuantLib の既定や他通貨からの類推はしない）。

| 項目 | QuantLib の既定 | TONAR | €STR | SOFR | SONIA |
|---|---|---|---|---|---|
| 支払頻度 | Annual | 二次：年 1 回または半年 [4] | 一次（EONIA）：年 1 回 [1]。清算は 1M〜12M も可 [2] | 二次：年 1 回 [6] | 未確認（清算では 2018 年 Q4 から年 1 回より多くも可 [3]） |
| 支払ラグ | 0 | 二次：2 営業日 [4][5] | 一次：T+1 [2] | 一次：2 日 [7] | 一次：ラグなし [8] |
| 受渡日数 | 必須の引数（`MakeOIS` の既定は 2 章） | 未確認 | 未確認（[1] は「Spot」とだけ記す） | 未確認 | 未確認 |
| 短期 OIS | 1 年以下は年 1 回の Backward で 1 回払い | 未確認 | 一次（EONIA）：0〜12 か月も Annual [1] | 二次：1 年未満は通常ゼロクーポン [5] | 未確認 |
| 日数計算 | 指標と同じ | 二次：Act/365 Fixed [4] | 一次（EONIA）：ACT/360 [1] | 二次：Act/360 [6] | 一次：ACT/365 Fixed [8] |

資料：

1. ECB, Working group on euro risk-free rates, *On the transfer of EONIA's cash and derivatives markets liquidity to the €STR*（2020-02-19）。表は EONIA のもので、€STR も仕様（time lag など）を保つことを勧めている。https://www.ecb.europa.eu/pub/pdf/other/ecb.wgeurofr_transferofliquidityfromEONIA_scashandderivativesmarketstotheeuroshorttermrate~c62c9819a5.en.pdf
2. LCH, *Product Eligibility EUR*（2019 年 10 月）。清算できる条件であり、取引の標準を決めるものではない。https://www.lch.com/sites/default/files/media/files/Product-Eligibility-EUR.pdf
3. Working Group on Sterling Risk-Free Reference Rates, *Discussion Paper: Conventions for referencing SONIA in new contracts*（2019 年 3 月）第 31 段落。https://www.bankofengland.co.uk/-/media/boe/files/markets/benchmarks/discussion-paper-conventions-for-referencing-sonia-in-new-contracts.pdf
4. 東京短資, TONA Swap Convention（日付なし）。https://www.tokyotanshi.co.jp/en/market_report/market_data/tona/swap.html
5. Clarus, *OIS Swap Nuances*（2015 年、2022 年更新）。https://www.clarusft.com/ois-swap-nuances/
6. Clarus, *SOFR Swap Nuances*（2020 年）。https://www.clarusft.com/sofr-swap-nuances/
7. ARRC, *An Updated User's Guide to SOFR*（2021 年 2 月）：標準の SOFR OIS は "generally use a payment delay to settle 2 days after the end of the interest period"。https://www.newyorkfed.org/medialibrary/Microsites/arrc/files/2021/users-guide-to-sofr2021-update.pdf
8. Working Group on Sterling Risk-Free Reference Rates, *Statement ... Conventions for referencing SONIA in new contracts*（2019 年 8 月）の OIS の列。https://www.bankofengland.co.uk/-/media/boe/files/markets/benchmarks/statement-and-summary-of-responses-to-sonia-conventions-discussion-paper.pdf

読めなかった資料：JSCC の 2 文書（`jpx.co.jp`、HTTP 403）、CME の €STR OIS 清算の文書（タイムアウト）。[1] の T+1・T+2 の議論は支払の決済（最後のフィキシングから決済まで）についてであり、取引の受渡日数ではない。CME の SOFR の文書（CFTC 提出、2020 年 6 月）にある「Payment Offset 2D」は 3 か月払いの個別の例なので、支払頻度の根拠にしない。

**支払ラグは QuantLib の既定と市場で違う**：既定の 0 は SONIA とは一致するが、€STR（T+1）と SOFR（2 日）とは異なる。TONAR の 2 営業日も二次資料からは既定と異なる。

### 2.11 英国のカレンダー

`Sonia` は `UnitedKingdom(Exchange)` を使い、Riemann の既存のカレンダーは `UnitedKingdom(Settlement)` だけである。

- 【ソース確認】v1.42 の `unitedkingdom.cpp` で、`SettlementImpl` と `ExchangeImpl` の `isBusinessDay` の本体は同じで、ともに共通の `isBankHoliday` を使う。
- 【実行確認】R バインディング 1.42 で全日付（109,208 日、1901-01-01〜2199-12-31）を比べ、Settlement と Exchange の差は 0 件だった（Metals との差も 0 件）。
- 既存の Riemann の Settlement は全日付で QuantLib と照合済みである（`docs/spec_dates_calendars.md`）。

したがって、**現時点で営業日の結果は変わらない**。ただし QuantLib では市場ごとに別の定義で、将来の改定で分かれる可能性があり、例外の日も別々に持つ。そのため、Riemann でも `UkMarket::Exchange` を別の値として追加し、規則は共有し、P1 と同じ方法で照合する。

## 3. v1.42 と `70f240f` の差【ソース確認】

Riemann は v1.42 に合わせる。`70f240f` では次が変わっている。

| 項目 | v1.42 | `70f240f` | 結果への影響 |
|---|---|---|---|
| 評価日が休日のときのスポット日 | `advance(adjust(評価日), n Days)` | `advance(評価日, n Days)`（issue #753） | **日付が変わる**。テストの例では、土曜日の T+2 が v1.42 は水曜日、`70f240f` は火曜日 |
| 預金の再現値 | `fixing(fixingDate, true)` | `forecastFixing(fixingDate)` | P3 の範囲では同じはず（評価日以後のフィキシングは予測） |
| 値の日付 | 1 日ごとの `Schedule` | `businessDayList` | 調整済みの開始日と終了日なら同じはず |
| 日次複利の計算 | スパン・スプレッドをその場で計算 | `growthFactor` に整理。範囲チェックを追加 | 過去のフィキシングと daily spread がなければ同じはず |
| `OISRateHelper` | 固定レグの日数計算は指標と同じ | `fixedDayCount` を追加 | 指定しなければ同じ |
| 補間の更新 | 作るときに更新 | `interpolateWithoutUpdate` | 同じはず |
| `Discount` の traits | `transformDirect` など | `globalGuess` を追加 | `IterativeBootstrap` には影響しないはず |

「同じはず」はソースを読んだ判断で、`70f240f` を実行した確認ではない。

## 4. グローバルな状態と Riemann の値【設計案】

| QuantLib のグローバルな状態 | 使う場所 | Riemann |
|---|---|---|
| `Settings::evaluationDate` | スポット日、預金の日付、クーポンの過去と将来の区別 | 引数 `evaluation_date` |
| `Settings::includeReferenceDateEvents` | 基準日の支払を含めるか（既定 false） | 含めない（P2 と同じ扱い） |
| `IndexManager` のフィキシング（指標名で共有） | 過去のフィキシング | P3 では持たない。将来は値として明示的に渡す |
| `Handle`・オブザーバー | カーブの更新 | なし。入力が変われば新しいカーブを作る |

カーブを作る処理は、入力の値から `Result<YieldCurve, Error>` を返す純粋関数にする。途中の節点は関数の中だけの可変状態とし、外に見せない。

```rust
// 形の案。名前は実装時に確定する。
pub struct OvernightIndex { pub calendar: HolidayCalendar, pub day_count: DayCount }

pub enum OisCurveInstrument {
    OvernightDeposit { rate: f64 },
    Ois {
        tenor: Period,
        rate: f64,
        settlement_days: u32,
        payment_frequency: u32,
        payment_lag: u32,
    },
}

pub struct OisCurveSpec {
    pub evaluation_date: Date,
    pub reference_date: Date,
    pub index: OvernightIndex,
    pub curve_day_count: DayCount,
    pub instruments: Vec<OisCurveInstrument>,
    pub interpolation: Interpolation,
    pub accuracy: f64,
}

pub fn build_ois_curve(spec: &OisCurveSpec) -> Result<OisCurve, Error>;
```

- 評価日、基準日、カレンダー、日数計算、受渡日数、支払頻度、支払ラグは既定値を持たず、すべて明示する。QuantLib の既定を市場の規約として扱わないためである。4 通貨の規約は、`tonar()` のように指標の値を返す関数で用意する。返すのは普通の値なので、項目を変えて使える。
- 指標は `OvernightIndex` の値であり、4 通貨の違いはデータだけにする。継承は使わない。
- 再現した気配（`quote_repricing`）をカーブと一緒に返す。これは GaussQuant と同じ情報で、「入力レートの再現」の確認に使う。
- P3 の範囲外（渡されたらエラー）：過去のフィキシング、lookback、lockout、算術平均、スプレッド、forward start、割引用の別カーブ、3 次以外の補間の追加。市場の規約の値は、一次資料で確認できた項目だけ `tonar()` などの値に入れ、未確認の項目は呼び出す側に明示させる。

## 5. 段階と照合の案【設計案】

- **P3-1（TONAR）**：O/N 預金と OIS、対数線形、基準日は評価日。追加するもの：`Brent`、翌日物の指標、OIS の日付の計算、`impliedQuote`、ブートストラップ。
- **照合用の値**：`tools/golden/` の R スクリプトで、QuantLib R バインディング 1.42 の `PiecewiseLogLinearDiscount` から作る。**気配は乱数で作った合成の値だけを使い、市場データは使わない**。GaussQuant の `tests/testthat/fixtures` は確認に使わず、リポジトリにも入れない。
- **照合する条件**：支払ラグは QuantLib の既定の 0 と、市場に近い値（1、2）の両方を入れる。ラグがあるとピラーが支払日に移るので、結果が変わる。
- **確かめる値**：ヘルパーの日付（完全一致）、節点の日付（完全一致）、節点の割引係数（下記）、節点の間の割引係数、再現した気配、QuantLib のエラーとその原因。評価日には休日・週末・月末を含める。
- **許容誤差**：割引係数は、同じ手順の Brent なら反復が揃い、数学ライブラリーの差だけが残るはずである。実測してから決める（仕様書 7.3 と 6 章の 3 番）。
- **P3-2**：€STR・SOFR・SONIA。受渡日数は 2.1 節。SONIA の受渡日数 0 では開始日が評価日になり、当日のフィキシングが予測になる場合を確かめる。
- **P3-3**：対数 3 次。解法は `FiniteDifferenceNewtonSafe` を追加する。

## 6. 既存の仕様との差

| # | 既存の記述 | 調査で分かったこと | 対応案 |
|---|---|---|---|
| 1 | 仕様書 10 章の P3：照合先は GaussQuant と回帰テスト、LagrangeFinance、EONIA ベンチマーク | 先に QuantLib R バインディングと直接照合する | 10 章に QuantLib を追記した。GaussQuant と LagrangeFinance は後の確認に回す。EONIA は `Eonia` クラスがあるだけで、P3-1〜P3-3 の照合には含めない |
| 2 | P1 の英国カレンダーは Settlement だけ | `Sonia` は `UnitedKingdom(Exchange)` を使う。規則の本体は同じで、全日付で差は 0 件（2.11 節） | P3-2 で `UkMarket::Exchange` を別の値として追加し、P1 と同じく全日付で照合する |
| 3 | 仕様書 7.3 の例：「気配の再現誤差 1e-8bp 以内」 | QuantLib の `accuracy = 1e-12` は割引係数の精度で、1 日の預金の金利では 1e-12 × 365 ≈ 3.7e-10（3.7e-6bp）までの差を許す。テストの許容誤差は 1e-8・1e-9（金利） | P3-1 の実測は最大 5.3e-11（5.3e-7bp）で、QuantLib も同じ値だった（8.3 節）。7.3 の例を 1e-9（金利）に直した |
| 4 | GaussQuant：基準日の既定は `settlement`、`overnight_deposit` は `evaluation` が必要 | `settlement` では O/N 預金のピラーが基準日以前になり、捨てられる | Riemann は `reference_date` を必須にする |
| 5 | GaussQuant：指標名は `TONA`、独自の `OvernightIndex` | QuantLib は `Tonar`。規約の値は同じ | Riemann の名前は TONAR に統一する |
| 6 | GaussQuant：SONIA のスポット日の計算は UK Settlement、指標は UK Exchange | 2 番のとおり規則の本体は同じなので、結果は同じはず | Riemann は指標のカレンダーに統一する |
| 7 | GaussQuant：`depo` と `flat_forward` にも対応 | P3 の 10 章には含まれない | P3 の範囲外。必要になったら足す |
| 8 | 10 章の「日次複利のクーポン」 | P3 の範囲では各日の積を計算しない（2.4 節） | 過去のフィキシングを扱う段階（P5 の ASW など）で積の計算を足す |

## 7. 未確認の事項

- **市場の規約**：2.10 節の「二次」と「未確認」の項目。特に、受渡日数は 4 指標とも一次資料で確認できていない。TONAR は支払頻度・支払ラグ・日数計算が二次資料だけで、JSCC の文書は読めなかった。€STR の支払頻度・短期・日数計算は EONIA の表からの確認で、資料も 2019〜2021 年のものなので、現在の規約とは照らしていない。ISDA の定義書は読んでいない。
- **R バインディングのビルド**：インストール済みのパッケージは Version 1.42 だが、ビルド日は 2026-04-06 で、`v1.42` のタグのコミット（2026-04-13）より前である。正確な C++ のコミットは特定できていない。
- **`70f240f` の動作**：C++ をビルドしておらず、3 章はソースとテストを読んだ判断である。
- **QuantLib のテスト**：読んだだけで、実行していない。
- **評価日当日の過去のフィキシング**：P3 では扱わないが、後の段階で渡す形は未定である。

## 8. P3-1 の実装と照合結果（TONAR、対数線形）

### 8.1 実装

- `ois.rs`：`OvernightIndex`、`tonar()`、`OisCurveInstrument`、`OisInterpolation`、`OisCurveSpec`、`build_ois_curve`（`OisCurve`、`QuoteRepricing`）。評価日、基準日、カレンダー、日数計算、受渡日数、支払頻度、支払ラグはすべて引数で渡し、グローバルな状態を使わない。
- `math/solver1d.rs`：`brent`（QuantLib と同じ手順）。`curves.rs`：`YieldCurve::discount_nodes`。
- 計算順は 2.4 節のとおり QuantLib に合わせ、固定レグの支払日も QuantLib と同じく ModifiedFollowing で調整する（`FixedVsFloatingSwap`）。
- この段階の補間は対数線形である。仕様の `MonotonicLogCubic` は 9 章で実装した。

### 8.2 照合の条件

2026-10-31、**正確な C++ コミットが不明な QuantLib R パッケージ Version 1.42 との照合**（C++ `v1.42` そのものを実行した照合ではない）。`tools/golden/p3_tonar_ois.R`、`cargo test --test golden_ois`。

合成の気配（乱数、市場データなし）で 800 通りを選んだ：評価日（2000〜2039 年。休日、週末、月末、2 月 29 日を含む）、基準日（評価日、スポット日、ごくまれに全商品より後）、カーブの日数計算 2 種類、受渡日数 0・1・2、支払ラグ 0・1・2、支払頻度 年 1・2 回、O/N 預金の有無、1 週間〜40 年の OIS を最大 14 本、気配（水準 −0.4%〜5% に乱数の増減を積み上げ、まれに 150%）、入力の順序の入れ替え。

### 8.3 照合結果【実行照合済み】

| 対象 | 件数 | 結果 |
|---|---|---|
| カーブの作成 | 800 通り | 764 通りを作成。36 通りは QuantLib と Riemann の両方がエラー |
| 商品の日付（開始日、満期、ピラー、最終関連日、使用の有無） | 6,394 件 | すべて一致 |
| 節点の日付 | 7,053 個 | すべて一致 |
| 節点の割引係数 | 7,053 個 | 6,733 個が完全一致。320 個は最大で相対 4.1e-15 の差（許容誤差 1e-12 以内） |
| 任意の日付の割引係数 | 7,639 個 | 6,814 個が完全一致。825 個は最大で相対 3.6e-15 の差 |
| 再現した気配（QuantLib との差） | 6,289 個 | 5,925 個が完全一致。364 個は最大で絶対 2.2e-16 の差（相対 5.3e-14） |
| 入力の気配の再現 `\|implied − quote\|` | 6,289 個 | 最大 5.3e-11（5.3e-7bp）。QuantLib も同じ 5.3e-11 |

作成時のエラーの原因は、QuantLib のメッセージと Riemann のメッセージを照合した：`all instruments expired` 21 通り、ピラーの重複 4 通り、`root not bracketed` 11 通り（気配を 150% にした場合など）。任意の日付の割引係数のエラー 1,522 件は、QuantLib のメッセージを記録していないので、日付からの判断である（基準日より前は負の時間、最後の日付より後は外挿なし）。

許容誤差：日付は完全一致、割引係数と再現した気配は相対 1e-12、入力の気配の再現は絶対 1e-9（金利）。

**差の原因（未確認の推定）**：`ln` と `exp` の数学ライブラリーの最下位ビットの違いと考えられるが、個別には確かめていない。作成に失敗した 36 通りの商品の日付は、比べていない。

**再現の精度**：`accuracy = 1e-12` は割引係数の精度なので、短い期間ほど金利の差は大きくなる。実測の最大 5.3e-11 は、以前の 7.3 の例（1e-8bp = 1e-12）を超える。QuantLib も同じ値なので Riemann 固有の問題ではなく、7.3 の例を 1e-9 に直した。

## 9. P3-3 の実装と照合結果（TONAR、`MonotonicLogCubic`）

### 9.1 実装

- `math/interpolation.rs`：`MonotonicCubicSpline`（Spline、Hyman の単調化、両端の 2 階微分 0）。三重対角の解き方（`TridiagonalOperator::solveFor`）と係数の計算順は QuantLib と同じ。v1.42 の `cubicinterpolation.hpp`、`tridiagonaloperator.cpp` を行単位で読んだ。
- `math/solver1d.rs`：`finite_difference_newton_safe`、`close_with`。`brent` は収束時に根で目的関数を呼ぶようにした（2.6 節）。
- `curves.rs`：`LogDiscountInterpolation`、`YieldCurve::interpolated_discounts`、`zero_rate`、`forward_rate`。
- `ois.rs`：`OisInterpolation::MonotonicLogCubic` と反復ループ。解法の戻り値ではなく、目的関数に最後に渡した値を節点に残す（2.6 節）。
- 再試行はしない（QuantLib の既定 `maxAttempts = 1`）。

### 9.2 照合の条件

2026-10-31、**正確な C++ コミットが不明な QuantLib R パッケージ Version 1.42 との照合**（C++ `v1.42` そのものを実行した照合ではない）。`tools/golden/p3_tonar_log_cubic.R`、`cargo test --test golden_ois`。

合成の気配で 500 通りを選んだ。条件は 8.2 節と同じで、気配が上下する場合（15%）と、±8% で交互に振れる解きにくい場合（5%）を加えた。割引係数のほかに、ゼロ金利（連続複利）、1 暦日の単利フォワード、同じ日付を指定したフォワードも比べた。

照合の相手の関数は `PiecewiseLogCubicDiscount` である。C++ の `MonotonicLogCubic` と同じ設定であることは、2.7 節のソース確認に加えて、できた節点を `MonotonicLogCubicDiscountCurve` に渡す比較でも確かめた（4 つの曲線形、各 407 日で差 0。`tools/verify/verify_ois.R` の `ois_method_check`）。

### 9.3 照合結果【実行照合済み】

| 対象 | 件数 | 結果 |
|---|---|---|
| カーブの作成 | 500 通り | 449 通りを作成。51 通りは QuantLib と Riemann の両方がエラー |
| 商品の日付（作成できた場合） | 3,592 件 | すべて一致 |
| 節点の日付 | 3,976 個 | すべて一致 |
| 節点の割引係数 | 3,976 個 | 3,343 個が完全一致。633 個は最大で相対 3.4e-14 の差 |
| 任意の日付の割引係数 | 4,917 個 | 3,355 個が完全一致。1,562 個は最大で相対 3.7e-14 の差 |
| 再現した気配（QuantLib との差） | 3,527 個 | 2,805 個が完全一致。722 個は最大で絶対 1.2e-14 の差 |
| ゼロ金利 | 4,917 個 | 3,486 個が完全一致。1,431 個は最大で絶対 8.9e-16 の差 |
| 1 暦日のフォワード | 4,685 個 | 4,166 個が完全一致。519 個は最大で絶対 1.4e-12 の差 |
| 同じ日付を指定したフォワード | 4,917 個 | 4,363 個が完全一致。554 個は最大で絶対 5.1e-11 の差 |
| 入力の気配の再現 `\|implied − quote\|` | 3,527 個 | 最大 1.6e-10。QuantLib も同じ 1.6e-10 |

作成時のエラー 51 通りは、原因が一致した：`root not bracketed` 28 通り、`convergence not reached`（100 周で収束しない）9 通り、`all instruments expired` 14 通り。`root not bracketed` は、失敗した反復の回数（1〜12 周目）と商品の位置も一致した。任意の日付の割引係数のエラー 913 件は、8.3 節と同じく日付からの判断である。ゼロ金利とフォワードのエラーは、エラーになることの一致だけを確かめ、原因は比べていない。作成に失敗した 51 通りの商品の日付は、比べていない。

許容誤差：日付は完全一致、割引係数は相対 1e-12、金利（再現した気配、ゼロ金利、1 暦日のフォワード）は絶対 1e-10、入力の気配の再現は絶対 1e-9。同じ日付を指定したフォワードは 0.0001 年で割るので、割引係数の相対 1e-12 に対応する絶対 1e-8 とした。

**再現した気配の判定を、相対から絶対に変えた理由**：P3-1 の相対 1e-12 では、ケース 316 の 1 週間の OIS だけが超えた（相対 3.0e-12、絶対 1.2e-14）。このケースの節点の割引係数は 15 個すべて完全に一致していた。差は節点の間の割引係数の比に入った最下位 1 ビット程度（2.2e-16）で、8/365 年で割ると約 1e-14 になる。金利が 0.38% と小さいので、相対では大きく見える。そこで、この比較は仕様書 7.3 節の金利の基準（絶対 1e-10）で判定した。P3-1（対数線形）の判定は変えていない。

**差の原因（未確認の推定）**：`ln` と `exp` の数学ライブラリーの最下位ビットの違いが、割引係数に入ったものと考えられる。3 次補間の差が対数線形の約 10 倍なのは、複数の節点が結果に影響し、反復を重ねるためと考えられる。どちらも個別には確かめていない。

### 9.4 対数線形との比較

`tools/verify/verify_riemann.qmd` で、同じ気配から作った 2 つのカーブを比べる。この文書の 4 つの曲線形では、ゼロ金利の差は最大 8.4bp、1 日フォワードの差は最大 90bp だった。対数線形のフォワードは節点で階段状に飛び、`MonotonicLogCubic` は連続する。一方、気配が上下する曲線形では、節点の間で対数線形より大きく振れる。これらは合成の気配の結果で、市場のカーブでの大きさを示すものではない。

## 10. P3-2 の実装と照合結果（€STR・SOFR・SONIA、`MonotonicLogCubic`）

### 10.1 実装

TONAR の処理をそのまま使い、指標の違いは `OvernightIndex` の値だけで表す。カーブを作る処理に、指標ごとの分岐はない。

| 関数 | カレンダー | 日数計算 | 【ソース確認】v1.42 |
|---|---|---|---|
| `tonar()` | `Calendar::Japan` | Actual/365 Fixed | `ql/indexes/ibor/tonar.hpp` |
| `estr()` | `Calendar::Target` | Actual/360 | `ql/indexes/ibor/estr.cpp` |
| `sofr()` | `Calendar::UnitedStates(UsMarket::Sofr)` | Actual/360 | `ql/indexes/ibor/sofr.cpp` |
| `sonia()` | `Calendar::UnitedKingdom(UkMarket::Exchange)` | Actual/365 Fixed | `ql/indexes/ibor/sonia.cpp` |

受渡日数、支払頻度、支払ラグは指標の値に入れず、これまでどおり呼び出す側が明示する。市場の規約として確認できていない項目があるためである（2.10 節）。英国 Exchange カレンダーは `docs/spec_dates_calendars.md` 3.5 節で実装と照合を済ませた。

### 10.2 照合の条件

2026-11-01、**正確な C++ コミットが不明な QuantLib R パッケージ Version 1.42 との照合**（C++ `v1.42` そのものを実行した照合ではない）。`tools/golden/p3_ois_index_log_cubic.R`、`cargo test --test golden_ois`。補間は `MonotonicLogCubic` で、照合の相手は `PiecewiseLogCubicDiscount` である（2.7 節）。

各指標とも合成の気配で 400 通りを選んだ。条件は 9.2 節と同じで、評価日には各カレンダーの休日と月末を含めた。受渡日数は €STR・SOFR が 0・1・2（2 が多い）、SONIA が 0・1・2（0 が多い）で、いずれも乱数で選んだ比較用の値であり、市場の規約ではない。

### 10.3 照合結果【実行照合済み】

**「QuantLib との照合」と「入力気配の再現」は別の判定である。** QuantLib と一致していても、入力の気配を再現しているとは限らない。`accuracy` は仕様どおり 1e-12 である。

**Riemann と QuantLib の差**

| 対象 | €STR | SOFR | SONIA |
|---|---:|---:|---:|
| カーブの作成（400 通り中） | 351 | 359 | 356 |
| 両方がエラー（原因も一致） | 49 | 41 | 44 |
| 商品の日付（すべて一致） | 2,767 件 | 2,815 件 | 2,927 件 |
| 節点の個数（日付はすべて一致） | 3,069 | 3,126 | 3,258 |
| 節点の割引係数の最大差（絶対） | 1.0e-12 | 7.3e-15 | 7.1e-15 |
| 任意の日付の割引係数の最大差（絶対） | 9.6e-13 | 3.9e-14 | 1.7e-14 |
| 再現した気配の最大差（絶対） | 2.7e-13 | 3.8e-15 | 3.0e-16 |
| ゼロ金利の最大差（絶対） | 3.1e-13 | 2.2e-12 | 7.3e-16 |
| 1 暦日のフォワードの最大差（絶対） | 9.7e-13 | 5.7e-13 | 4.0e-13 |
| 同じ日付を指定したフォワードの最大差（絶対） | 2.7e-11 | 2.7e-11 | 2.0e-11 |
| **QuantLib との照合** | **PASS** | **PASS** | **PASS** |

作成時のエラーの内訳は、`root not bracketed` が €STR 24、SOFR 28、SONIA 29（失敗した反復の回数と商品の位置も一致）、100 周で収束しないものが 6、4、2、全商品の期限切れが 16、7、11、ピラーの重複が 3、2、2 である。任意の日付の割引係数のエラーは 713、747、696 件で、9.3 節と同じく日付からの判断である。ゼロ金利とフォワードのエラーは、エラーになることの一致だけを確かめた。作成に失敗した場合の商品の日付は、比べていない。

**入力気配の再現誤差（基準：金利で 1e-9）**

| 対象 | TONAR | €STR | SOFR | SONIA |
|---|---:|---:|---:|---:|
| Riemann の最大誤差 | 1.6e-10 | 2.6e-5 | 3.8e-3 | 1.6e-10 |
| Riemann の基準未達（商品、カーブ） | 0、0 | 3、2 | 2、1 | 0、0 |
| QuantLib の最大誤差 | 1.6e-10 | 2.6e-5 | 3.8e-3 | 1.6e-10 |
| QuantLib の基準未達（商品、カーブ） | 0、0 | 3、2 | 2、1 | 0、0 |
| **入力気配の再現** | **基準達成** | **基準未達** | **基準未達** | **基準達成** |

基準未達のカーブは次の 3 つで、未達のカーブは Riemann と QuantLib で同じである。テストはこの一覧を固定しており、増減すると失敗する。

| 指標 | ケース | 気配 | Riemann と QuantLib の最大誤差 | QuantLib との照合 | 入力気配の再現 |
|---|---|---|---|---|---|
| €STR | 125 | ±8% で交互に振れる | 1.8e-7 | PASS | 基準未達 |
| €STR | 246 | ±8% で交互に振れる | 2.6e-5 | PASS | 基準未達 |
| SOFR | 199 | ±8% で交互に振れる | 3.8e-3 | PASS | 基準未達 |

3 つとも節点の割引係数は QuantLib と完全に一致し（差 0）、最小の割引係数はほぼ 0 である。`accuracy` は割引係数に対する絶対の精度なので、割引係数がほぼ 0 のカーブでは金利の精度が落ちる。これは QuantLib の方式どおりの動作だが、**このような入力では、Riemann は入力の気配を再現しない**。呼び出す側は `quote_repricing` で再現誤差を確かめる必要がある。

### 10.4 許容誤差の変更の理由と対象件数

変更は `MonotonicLogCubic` の照合用の判定だけである。**ライブラリーの `accuracy` は変えていない。** 対数線形の判定も変えていない。

| 対象 | 変更前 | 変更後 | 変更前の基準を超えた件数 |
|---|---|---|---|
| 節点の割引係数 | 相対 1e-12 | 絶対 2e-12 | €STR 1 個（ケース 84）。他の 3 指標は 0 |
| 任意の日付の割引係数 | 相対 1e-12 | 絶対 2e-12 | €STR 1 個。他の 3 指標は 0 |
| 再現した気配の QuantLib との差 | 相対 1e-12 | 絶対 1e-10 | TONAR 1 個、€STR 2 個、SOFR 1 個、SONIA 0 個 |

- **割引係数**：ケース 84 の差は相対 1.1e-12、絶対 1.0e-12 である。最後の周の変化量がほぼちょうど `accuracy` で、停止の判定が最下位ビットで分かれ、QuantLib と Riemann の反復回数が 1 回違う。原因を確かめるために、一度だけ `accuracy` を 1.001e-12 にして実行すると、QuantLib との差は 6.7e-16 に下がった。**この値は切り分けの実験であり、本番の設定にも照合にも使わない。** 仕様書 7.3 節は、収束条件の範囲内で反復回数が変わることを許している。両者がそれぞれ `accuracy` 以内で止まるので、差の上限を 2e-12 とした。
- **再現した気配**：仕様書 7.3 節の金利の基準に合わせた。TONAR のケース 316 は 9.3 節のとおり確かめた。€STR と SOFR の 3 個は、原因を個別には確かめていない（未確認の推定：短い期間で割るので、最下位ビットの差が相対では大きく見える）。絶対差は最大 2.7e-13 である。
- **入力気配の再現の基準（1e-9）は変えていない。** 未達は上のとおり「基準未達」と記録する。

**差の原因（未確認の推定）**：ケース 84 以外の微小な差は、9.3 節と同じく数学ライブラリーの最下位ビットの違いと考えられるが、個別には確かめていない。

### 10.5 検証レポート

`tools/verify/verify_riemann.qmd` に 3 指標を追加した。TONAR は 4 つ、他の指標は 2 つの曲線形で、対数線形と `MonotonicLogCubic` の両方を QuantLib と比べる。対数線形の 3 指標の照合は、このレポートの範囲だけである（`cargo test` の照合は `MonotonicLogCubic` だけ）。照合用データ全件の結果も表示し、「Riemann と QuantLib の差」「Riemann の再現誤差」「QuantLib の再現誤差」を分け、総合判定でも「QuantLib との照合」と「入力気配の再現」を分ける。

### 10.6 微小な差の原因（P4 で判明）

R パッケージ（`crates/riemann-r`、Rtools の GNU ツールチェーン）で照合用データ全件を計算すると、節点、任意の日付の割引係数、再現した気配の QuantLib R パッケージとの差は、4 指標とも**ちょうど 0** だった（2026-11-01、`tools/verify/verify_riemann.qmd`）。`cargo test` は MSVC でビルドする。QuantLib R パッケージも Rtools でビルドされている。

【実行確認】したがって、8.3 節、9.3 節、10.4 節で未確認の推定としてきた微小な差の主な原因は、MSVC と Rtools の数学ライブラリーの違いと判断できる。ただし、`exp`・`ln` などの関数ごとに個別には確かめていない。GNU ビルドの値は、実行するスレッドの x87 の精度でも変わる（`docs/spec_r.md` 5 章）。ここでの差 0 は、R のメインスレッドで計算する直接呼び出しの結果である。

€STR のケース 84 の反復回数の違いも、GNU ビルドでは起きない。10.4 節の許容誤差は、MSVC で実行する `cargo test` のものとして残す。**未達の 3 カーブは GNU ビルドでも基準未達のままである。**
