# CDS の仕様（P8）

実装は `crates/riemann/src/cds.rs`、R の入口は `crates/riemann-r/R/cds.R` である。計算は QuantLib v1.42 に従う。LagrangeFinance II-5 と同じ構成である。

## 1. 対象

| 計算 | 関数（R） | QuantLib での対応 |
|---|---|---|
| CDS の契約 | `credit_default_swap` | `CreditDefaultSwap` |
| 評価（MidPoint、ISDA） | `cds_value` | `MidPointCdsEngine`、`IsdaCdsEngine` |
| 生存確率 | `hazard_curve`、`survival_probability` | `FlatHazardRate`、`InterpolatedHazardRateCurve<BackwardFlat>` |
| ハザードのブートストラップ | `bootstrap_hazard_curve` | `PiecewiseFlatHazardRate`、`SpreadCdsHelper` |
| 標準 CDS の満期 | `cds_maturity` | `cdsMaturity` |
| スプレッドと回収率の感応度 | `cds_quote_sensitivity` | なし（自動微分） |

スケジュールの規則 `cds` と `cds2015` を、この段階で実装した（`time/schedule.rs`）。

請求は額面で、デフォルト時に経過クーポンと請求を支払う。ISDA エンジンの設定は QuantLib の既定（Taylor 展開、半日のバイアス、クーポン期間内は区分的なフォワード）である。両方のカーブの基準日が評価日で、Actual/365 (Fixed) でなければならない。

**感応度**：NPV をハザードについて自動微分し、解いたカーブでの陰関数定理でスプレッドについての微分に変換する。回収率の感応度は、ハザードカーブを固定した値である。

## 2. 照合結果【実行照合済み】（2026-11-01）

基準値は QuantLib R パッケージ Version 1.42 で、入力はすべて合成である（`tools/golden/p8_cds.R`）。呼び方は LagrangeFinance II-5 と同じである。

| 対象 | 件数 | 最大の差 |
|---|---|---|
| NPV、クーポンレグ、デフォルトレグ、公正スプレッド、公正アップフロント（MidPoint、ISDA、`Forward`・`Backward`・`CDS2015`、買い手・売り手） | 300 | 2.4e-14（`\|a - b\| / (1 + \|b\|)`） |
| ブートストラップした生存確率（6 節点、MidPoint、ISDA） | 40 カーブ、240 節点 | 1.1e-16（絶対） |

感応度は、カーブを作り直した中心差分と比べた（Rust のテスト：相対 1e-5 以内。結果は検証 QMD）。

市場価格との照合はしていない。ハザードカーブの形や回収率の妥当性は、ここでは確かめていない。

## 3. 基準値を作るときの注意

QuantLib R パッケージの `hazardRates()` はスタックオーバーフローで失敗した（`PiecewiseFlatHazardRate` に対して）。そこで、各節点の生存確率を基準値にしている。

## 3.1 QuantLib の `MakeCreditDefaultSwap`【ソース確認・実行照合済み】

LagrangeFinance II-5.6 は、GaussQuant を通してこのビルダーで標準 CDS を作る。ビルダーと素のコンストラクターは、次の設定が違う（`ql/instruments/makecds.hpp`、`makecds.cpp`、v1.42）。

| 設定 | コンストラクター | ビルダー | R の引数 |
|---|---|---|---|
| 最終期間の日数計算 | Actual/360 | `Actual360(true)`：最終日を数える | `last_period_day_count = "actual_360_include_last_day"` |
| 保護の開始日 | スケジュールの最初の日付 | CDS 規則では取引日 | `protection_start` |
| 取引日 | 保護の開始日から決める | 指定の日付（経過利子の rebate と決済日を決める） | `trade_date` |
| 決済日数 | 3 | 3 | `cash_settlement_days` |

**比較資料の FAIL 2 行の原因の確定**：coupon leg の差 267.70 と 268.03 は、最終期間に 1 日多い利息である。`Actual360(true)` が直接の原因で、ソースで確かめた。あわせて、取引日と保護の開始日を渡していなかったので、rebate が 0 になり fair upfront が出なかった。中核の `CdsSpec` にはこれらの項目が初めからあり、R の入口に出していなかっただけである。計算の不具合ではない。

**照合**（2026-11-02、QuantLib R パッケージ 1.42、`MakeCreditDefaultSwap` を直接呼ぶ合成入力 200 件、`tools/golden/p8_cds_builder.R`、`crates/riemann/tests/golden_cds.rs`）：NPV、coupon leg、default leg、rebate、fair spread、fair upfront の差は想定元本あたり最大 1.9e-16 である。

## 4. 未対応

- CDS オプション、インデックス、`UpfrontCdsHelper` の気配。
- 請求の変更（額面以外）、デフォルト時以外の支払い。
- ISDA エンジンの数値設定の切り替え。
- 金利カーブの気配についての感応度。ノードの微分は計算の中で持っているので、必要になれば足せる。
