# LMM の構成要素の仕様（P11-6）

実装は `crates/riemann/src/lmm.rs`（コレスキー分解は `crates/riemann/src/math/linear.rs`）、R の入口は `crates/riemann-r/R/lmm.R`、比較文書は `comparison/chapters/ch10_sabr.qmd` の「Caplets and correlated shocks」節である。

**範囲**：ユーザーの指示により、当面は LagrangeFinance II-7.4 の Caplet と相関の例を再現する範囲とする。LMM 全体（共通測度でのドリフト、複数のフォワード金利の共同経路、Swaption や相関への校正）は実装しない。本格的なモデルへの拡張は別途判断する。**2026-11-04 の指示で範囲を広げた**：後決め金利の FMM と、LMM の共同経路・校正は [spec_rates_fx_extension.md](spec_rates_fx_extension.md) で扱う。

## 1. LagrangeFinance II-7.4 の条件【ソース確認】

- **II-7.4.2 Caplet**：フォワード 0.025、行使価格 0.0275、満期 2 年、ボラティリティ 0.24、利息期間 0.5 年、割引係数 `exp(−0.02 × 2.5)`、想定元本 1。Black 公式 `N Δ P(0, T+Δ) [F N(d1) − K N(d2)]` と、当該フォワード測度での周辺分布 `L(T) = L(0) exp(−v²T/2 + v√T Z)` から作った 5 万本のモンテカルロ価格を比べる（`set.seed(618)`、`rnorm`）。標準誤差は `sd / √n`。総ボラティリティが 0 のときは本源的価値。
- **II-7.4.3 相関**：目標の相関行列（0.8、0.5、0.7）の上三角のコレスキー因子 `chol(C)` で、独立な正規乱数 3 万行 × 3 列に相関を付け（`set.seed(619)`、`matrix(rnorm(90000), ncol = 3)` で列方向に詰める）、標本相関を確認する。各金利の測度や共通測度のドリフト、共同経路は扱わない。

## 2. Riemann の実装

| 関数（R） | 内容 |
|---|---|
| `compute_caplet_value` | Black または Bachelier の公式による Caplet 価格。総ボラティリティ 0 は本源的価値 |
| `compute_caplet_from_normals` | 与えた標準正規乱数による、周辺分布のモンテカルロ価格と標準誤差 |
| `compute_correlated_normals` | 独立な正規乱数の各行 `z` を `L z`（`L Lᵀ = C`）にする。R の `Z %*% chol(C)` と同じ |
| `generate_correlated_normals` | このライブラリの乱数で相関付きの正規乱数を作る |

**乱数は入力**：計算の関数は乱数を配列で受け取る。そのため R の乱数を渡せば LagrangeFinance と同じ値になり、言語によらず同じ乱数から同じ値が出る。このライブラリの乱数による生成は別の関数にした。

**入口の検査**：相関行列は、対角が 1、対称、成分が [−1, 1]、正定値（コレスキー分解の軸が `n × EPSILON × 最大の対角` を超える）であることを一度だけ確かめる。Black のときはフォワードと行使価格が正であることも確かめる。

## 3. 照合結果【実行照合済み】（2026-11-04）

| 対象 | 比べた相手 | 結果 |
|---|---|---|
| Caplet（Black 公式） | QuantLib `blackFormula` | 差 0（REF） |
| Caplet（Black 公式） | LagrangeFinance II-7.4.2 | PASS、差 0 |
| Caplet（5 万本、同じ乱数） | LagrangeFinance II-7.4.2 | PASS、差 1.3e-18 |
| 標準誤差 | LagrangeFinance II-7.4.2 | PASS、差 4.2e-20 |
| 相関付きショック（3 万行） | R の `Z %*% chol(C)` | 差 0（REF） |
| 標本相関 9 個 | LagrangeFinance II-7.4.3 | PASS、差 0 |
| 標本相関（このライブラリの乱数） | LagrangeFinance II-7.4.3 | N/C（乱数が違う。A-B は 0.8030 と 0.7993） |

比較文書では、Caplet の価格を想定元本の bp（×1e4）で示した。価格が約 0.00115 で、許容誤差 1e-4 では判定にならないためである。

## 4. 未対応

- LMM 全体：スポット測度や終端測度でのドリフト、複数のフォワード金利の共同経路、Swaption の評価、ボラティリティと相関の校正。
- Shifted Lognormal 型、HJM、FMM（後決め金利）。
- マルチカーブ（OIS 割引と予測フォワードの分離）での Caplet。
