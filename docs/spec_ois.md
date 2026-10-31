# OIS カーブの仕様（P3）

P3 では、O/N 預金と OIS の気配から割引カーブを作る。この文書は実装前のソース調査と仕様の整理であり、Riemann の OIS の実装はまだない。計算の方式は QuantLib C++ で定める。R バインディングは照合に使うだけで、Rust の設計を R バインディングに合わせることはしない。

記述の区分（【ソース確認】【実行確認】【設計案】）は仕様書と同じ考え方である。ただし【実行確認】は R バインディングで動作を確かめた少数の例で、【実行照合済み】（Riemann との数値照合）ではない。P3 に【実行照合済み】の項目はまだない。

| 段階 | 内容 | 状態 |
|---|---|---|
| P3-0 | QuantLib のソース調査と仕様の整理 | 2026-10-31 完了（この文書） |
| P3-1 | TONAR：O/N 預金と OIS、対数線形補間、入力レートの再現、QuantLib との照合 | 未着手 |
| P3-2 | €STR・SOFR・SONIA へ拡張（英国 Exchange カレンダーを含む） | 未着手 |
| P3-3 | 対数 3 次補間（反復ループ、単調化フィルター） | 未着手 |

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

対数 3 次（`global = true`）はループが必須になる。2 周目からは `FiniteDifferenceNewtonSafe` で前の周の値を初期値にして解き、`min = min(data) / 2` とする。節点の変化の最大が `accuracy` 以下で終わり、最大 100 周。

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
| 3 | 仕様書 7.3 の例：「気配の再現誤差 1e-8bp 以内」 | QuantLib の `accuracy = 1e-12` は割引係数の精度で、1 日の預金の金利では 1e-12 × 365 ≈ 3.7e-10（3.7e-6bp）までの差を許す。テストの許容誤差は 1e-8・1e-9（金利） | 例の値は QuantLib の保証より厳しい。P3-1 の実測のあとで 7.3 の例を見直す（今回は変更していない） |
| 4 | GaussQuant：基準日の既定は `settlement`、`overnight_deposit` は `evaluation` が必要 | `settlement` では O/N 預金のピラーが基準日以前になり、捨てられる | Riemann は `reference_date` を必須にする |
| 5 | GaussQuant：指標名は `TONA`、独自の `OvernightIndex` | QuantLib は `Tonar`。規約の値は同じ | Riemann の名前は TONAR に統一する |
| 6 | GaussQuant：SONIA のスポット日の計算は UK Settlement、指標は UK Exchange | 2 番のとおり規則の本体は同じなので、結果は同じはず | Riemann は指標のカレンダーに統一する |
| 7 | GaussQuant：`depo` と `flat_forward` にも対応 | P3 の 10 章には含まれない | P3 の範囲外。必要になったら足す |
| 8 | 10 章の「日次複利のクーポン」 | P3 の範囲では各日の積を計算しない（2.4 節） | 過去のフィキシングを扱う段階（P5 の ASW など）で積の計算を足す |

## 7. 未確認の事項

- **市場の規約**：2.10 節の「二次」と「未確認」の項目。特に、受渡日数は 4 指標とも一次資料で確認できていない。TONAR は支払頻度・支払ラグ・日数計算が二次資料だけで、JSCC の文書は読めなかった。€STR の支払頻度・短期・日数計算は EONIA の表からの確認で、資料も 2019〜2021 年のものなので、現在の規約とは照らしていない。ISDA の定義書は読んでいない。
- **R バインディングのビルド**：インストール済みのパッケージは Version 1.42 だが、ビルド日は 2026-04-06 で、`v1.42` のタグのコミット（2026-04-13）より前である。正確な C++ のコミットは特定できていない。
- **3 次補間の細部**：`cubicinterpolation.hpp` は係数の組み立ての要点だけを読んだ。三重対角の解き方と Hyman フィルターは、P3-3 で行単位で確認する。
- **`70f240f` の動作**：C++ をビルドしておらず、3 章はソースとテストを読んだ判断である。
- **QuantLib のテスト**：読んだだけで、実行していない。
- **再現の精度**：Rust で Brent の反復が QuantLib と揃うかは、実装して照合するまで分からない。
- **評価日当日の過去のフィキシング**：P3 では扱わないが、後の段階で渡す形は未定である。
