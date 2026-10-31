# 債券の仕様（P2、草案）

P2 では、固定利付債のキャッシュフロー、経過利子、価格と利回り、割引を作る。QuantLib の仕様に依拠し、QuantLib R バインディング 1.42 の実行結果と照合する。進め方は、仕様書（[specification.md](specification.md)）の 10 章と同じく、小さく分けて照合しながら広げる。

記述の区分（【ソース確認】【実行照合済み】【設計案】）は仕様書と同じである。

| 段階 | 内容 | 状態 |
|---|---|---|
| P2-1 | 固定利付債のキャッシュフロー（クーポン・償還）、受渡日、額面、経過利子 | 2026-11-01 照合済み（2 章） |
| P2-2 | 利回りと価格（複利・単利・連続、クリーン・ダーティ）、デュレーション、コンベクシティ、BPV | 未着手 |
| P2-3 | 割引カーブによる評価（ゼロカーブ・フラットカーブ） | 未着手 |
| P2-4 | 日本式の単利利回り（GaussQuant の `bond_*_YEN_GQL`、LagrangeFinance II-1） | 未着手 |

## 1. QuantLib の仕様【ソース確認】

参照：QuantLib `70f240f`（照合の正は R バインディング 1.42 の実行結果）。v1.42 からの差分を読んだ範囲では、以下の結果に関わる変更は見当たらなかった（`cashflows.cpp` は `coupon_cast` への置き換えと BPS 計算の書き方の整理、`cashflow.cpp`・`event.cpp`・`bond.cpp` は `std::optional` への置き換えなど）。

| 項目 | QuantLib の仕様 | ソース |
|---|---|---|
| クーポンの額 | `nominal * (compoundFactor - 1.0)`。単利なら `compoundFactor = 1 + r t`、t は日数計算の年数割合 | `cashflows/fixedratecoupon.cpp`、`interestrate.cpp` |
| 参照期間 | 途中の期間は計算期間と同じ。最初の期間が規則どおりでなければ、参照期間の開始を「終了日から 1 期間前」（スケジュールのカレンダー・規約・月末の扱いで計算）にする。最後の期間が規則どおりでなければ、参照期間の終了を「開始日から 1 期間後」にする | `FixedRateLeg::operator Leg` |
| 支払日 | 計算期間の終了日を、支払カレンダー（指定がなければスケジュールのカレンダー）と支払の営業日調整で調整した日 | 同上 |
| 償還 | 額は `(償還率 / 100) * 額面`。日付は最後のクーポンの支払日。日付で安定ソートし、同じ日付ならクーポンの後 | `bond.cpp`（`addRedemptionsToCashflows`、`calculateNotionalsFromCashflows`） |
| 受渡日 | 評価日から受渡日数だけ営業日を進めた日（0 日なら Following で調整）。発行日より前にはしない | `Bond::settlementDate` |
| 額面 | 償還日より前は額面、償還日以後は 0 | `Bond::notional` |
| 発生済みの判定 | 基準日を含めない（既定）なら、支払日 ≤ 基準日で発生済み。グローバル設定 `includeTodaysCashFlows` で変わりうる | `cashflow.cpp`、`event.cpp` |
| 経過利子 | 受渡日に発生済みでない最初のキャッシュフローの支払日について、その日に支払われるクーポンの経過利子を合計し、額面 100 当たりにする。額面が 0 なら 0 | `CashFlows::accruedAmount`、`BondFunctions::accruedAmount` |

## 2. 実装と照合結果（P2-1）

### 2.1 実装

- `cashflows.rs`：`FixedRateCoupon`（支払日、額面、利率、日数計算、計算期間、参照期間）、`CashFlow`（クーポン・償還）、レグを作る `fixed_rate_leg`、経過利子 `accrued_amount`。クーポンの額は QuantLib と同じ計算順（`N * ((1 + r t) - 1)`）で求める。
- `instruments/fixed_rate_bond.rs`：`FixedRateBondSpec` から `FixedRateBond::new` で債券（スケジュールとキャッシュフロー）を作る。`settlement_date(評価日)`、`notional(日付)`、`accrued_amount(受渡日)`。
- QuantLib との違い：
  - 評価日は引数で渡す（グローバル設定を使わない）。
  - 受渡日当日のキャッシュフローを含めるかどうかは引数で渡す（既定は QuantLib の既定と同じく含めない）。
  - 利率と額面は 1 つ。償却、ex-coupon、支払ラグは扱わない。
- 処理は QuantLib から移したものとして扱い、各ファイルの先頭と `THIRD_PARTY_NOTICES` に移植元と著作権表示を書いた。

### 2.2 照合結果【実行照合済み】

2026-11-01、QuantLib R バインディング 1.42、`tools/golden/p2_fixed_rate_bonds.R`、`cargo test --test golden_bonds`。

入力は乱数（種を固定）で 1,500 本を選んだ：開始日 6 種類（月末・2 月末・休日を含む）の前後、満期 1〜30 年、利払いの間隔 3 か月・6 か月・1 年、カレンダー 4 種類、営業日調整 4 種類（スケジュール・終了日・支払）、Backward・Forward、月末の扱いの有無、端数期間（first date・next-to-last date）、クーポン 0〜8%、日数計算 5 種類（Actual/365 Fixed、Actual/360、30/360 BondBasis、Actual/Actual ISMA・ISDA）、償還率 100・101.5、受渡日数 0〜3、額面 100・100 万。

| 対象 | 件数 | 結果 |
|---|---|---|
| 債券の作成 | 1,500 本 | すべて作成でき、QuantLib と同じ |
| キャッシュフロー（種類、支払日、額、額面、利率、計算期間、参照期間） | 40,379 件（クーポン 38,879 件、うち参照期間が計算期間と異なるもの 1,195 件。償還 1,500 件） | すべて一致 |
| 評価日ごとの受渡日・額面・経過利子 | 10,498 件（経過利子が 0 でないもの 4,998 件、満期後で額面 0 のもの 2,719 件） | すべて一致 |
| 数値（額、額面、利率、経過利子） | 139,133 個 | すべて浮動小数の最後の桁まで一致 |
