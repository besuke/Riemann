# Hull-White モデルと SABR の仕様（P9）

実装は `crates/riemann/src/hull_white.rs`、`crates/riemann/src/sabr.rs`、R の入口は `crates/riemann-r/R/rate_models.R` である。

**モデルの価値であることに注意する。** ここでの一致は、実装がモデルどおりであることを示す。パラメーターの妥当性や、市場で成り立つ価格であることは示さない。

## 1. Hull-White

`dr = (theta(t) - a r) dt + sigma dW` を初期カーブに合わせる。時間は年で、カーブの基準日からの値である。

| 計算 | 関数（R） | 手法 |
|---|---|---|
| 割引債オプション | `hull_white_bond_option` | QuantLib `HullWhite::discountBondOption` |
| European スワップション | `hull_white_swaption` | Jamshidian の分解 |
| Bermudan スワップション | `hull_white_bermudan_swaption` | 状態格子の後退計算 |
| コール債 | `hull_white_callable_bond` | 状態格子の後退計算（LagrangeFinance II-8.4） |

**感応度**：European スワップションの `a` と `sigma` についての微分は自動微分である。臨界金利 `r*` は数値で解いたうえで、記録した値の上で Newton 法を 1 回行い、陰関数定理の微分を伝える。1 階の微分が正確になる。格子の計算（Bermudan、コール債）の自動微分は未対応である。ノード数が多く、確保するメモリーが大きいためである。

**格子の計算**：各ステップは、状態と割引の同時正規分布から求めた正確な条件付き期待値である。積分は数値で行い、次の時点の値を線形に補間する。任意の初期カーブで使える。

## 2. SABR

Hagan らの展開式で、対数正規と正規の両方を返す（`sabr_volatility`）。フォワードと行使価格は正でなければならない。シフト付きは未実装である。

`calibrate_sabr` は、`beta` を固定して `alpha`、`nu`、`rho` を合わせる。Levenberg–Marquardt 法で、ヤコビアンは自動微分である。内部の変数は `(ln alpha, ln nu, atanh rho)` なので、値が範囲を外れない。

## 3. 照合結果【実行照合済み】（2026-11-01）

入力はすべて合成である（`tools/golden/p9_hull_white_sabr.R`）。カーブは対数線形の割引係数で、QuantLib と同じ節点を渡す。

| 対象 | 比べた相手 | 件数 | 最大の差（絶対） |
|---|---|---|---|
| European スワップション | QuantLib `JamshidianSwaptionEngine` | 200 | 6.5e-9（下の説明） |
| Bermudan スワップション | QuantLib `TreeSwaptionEngine`（1000 段） | 20 | 1.0e-5 |
| コール債、満期一括債 | LagrangeFinance II-8 `compute_callable_bond_LRL` | 20 | 8.4e-13 |
| SABR（対数正規） | QuantLib `sabrVolatility` | 400 | 2.3e-14（相対） |

割引債オプションは、Jamshidian の分解を通じて確かめた。単独の QuantLib との比較はしていない。正規の SABR は QuantLib R パッケージから直接呼べないので、自己校正と連続性だけを確かめた。

**モデルの性質のテスト**：プット・コール・パリティ（差 1e-12 以内）、Jamshidian と 1 回行使の格子の一致（1e-5 以内）、Bermudan ≥ European、コールなしのコール債 = 満期一括債（1e-10 以内）を確かめた。SABR は自分の生成したスマイルからパラメーターを戻せることを確かめた。

## 4. 差の原因

**European スワップションの 6.5e-9**：QuantLib は `r*` を精度 1e-8 で探し、そのまま使う。Riemann は 1e-12 で探して Newton 法でさらに詰める。QuantLib の探索を同じ手順で再現すると（精度 1e-8、Newton なし）、差は 5.4e-15 に下がった【実行照合済み】。差の原因は QuantLib の探索の精度であり、Riemann の値のほうが解に近い。

**Bermudan の 1.0e-5**：QuantLib の木自身が、500 段から 1000 段で最大 1.6e-5 変わる。この比較は木の誤差の範囲での一致である。

**基準値の作り方の注意**：QuantLib の `Annual` スケジュールは暦の 1 年刻みなので、365 日刻みではない。最初にこれを取り違えて差が出た。基準値には実際の支払日を書き出している。

## 5. LagrangeFinance に関する所見

LagrangeFinance II-8 と同じ Gauss–Hermite 法（24 点）は、行使の折れ目があるときに精度が落ちる。Riemann の単体テストでは、1 回行使のスワップションで Jamshidian と 1.3e-4 の差が出た（48 点、状態 1201 点）。台形則（幅 0.02）に変えると 1e-5 以内に入った。`hull_white_bermudan_swaption` の既定を台形則にしたのは、このためである。コール債の既定は、LagrangeFinance と同じ値を再現するために Gauss–Hermite 24 点のままとする。方法ごとの差は検証 QMD に示す。LagrangeFinance は変更していない。

## 6. 未対応

- BGM/LMM、HJM、FMM（LagrangeFinance II-7）。
- Hull-White のキャリブレーション（II-6.4）。
- Sobol 列と Longstaff–Schwartz 法（II-4.6）。
- MBS のプリペイメントと OAS（II-8.8）。
- キャップ・フロア。IBOR の扱いは対象外とした。
