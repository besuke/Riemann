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

## 4. 未対応

- CDS オプション、インデックス、`UpfrontCdsHelper` の気配。
- 請求の変更（額面以外）、デフォルト時以外の支払い。
- ISDA エンジンの数値設定の切り替え。
- 金利カーブの気配についての感応度。ノードの微分は計算の中で持っているので、必要になれば足せる。
