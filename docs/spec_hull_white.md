# Hull-White モデルと SABR の仕様（P9）

実装は `crates/riemann/src/hull_white.rs`、`crates/riemann/src/sabr.rs`、R の入口は `crates/riemann-r/R/rate_models.R` である。

**モデルの価値であることに注意する。** ここでの一致は、実装がモデルどおりであることを示す。パラメーターの妥当性や、市場で成り立つ価格であることは示さない。

## 1. Hull-White

`dr = (theta(t) - a r) dt + sigma dW` を初期カーブに合わせる。時間は年で、カーブの基準日からの値である。

| 計算 | 関数（R） | 手法 |
|---|---|---|
| 割引債オプション | `compute_hull_white_bond_option_value` | QuantLib `HullWhite::discountBondOption` の式を因数分解した形（1.1 節） |
| European スワップション | `compute_hull_white_swaption_value` | Jamshidian の分解 |
| Bermudan スワップション | `compute_hull_white_bermudan_swaption_value` | 状態格子の後退計算 |
| コール債 | `compute_hull_white_callable_bond_value` | 状態格子の後退計算（LagrangeFinance II-8.4） |
| 校正 | `calibrate_hull_white` | 対数変数の Levenberg–Marquardt、自動微分（1.1 節） |

**感応度**：European スワップションの `a` と `sigma` についての微分は自動微分である。臨界金利 `r*` は数値で解いたうえで、記録した値の上で Newton 法を 1 回行い、陰関数定理の微分を伝える。1 階の微分が正確になる。格子の計算（Bermudan、コール債）の自動微分は未対応である。ノード数が多く、確保するメモリーが大きいためである。

**格子の計算**：各ステップは、状態と割引の同時正規分布から求めた正確な条件付き期待値である。積分は数値で行い、次の時点の値を線形に補間する。任意の初期カーブで使える。

## 1.1 校正（P11-3）

`calibrate_hull_white` は、割引債オプションと European スワップションの市場価格に `a` と `sigma` を合わせる。`fix_mean_reversion = TRUE` なら `sigma` だけを合わせる。

### 比較元の条件【ソース確認】

**LagrangeFinance II-6.4**（`part2_quantive_riemann/ch06_irs_calibration.qmd`）

- 市場価格は合成である。フラットなゼロ金利 0.03、`a = 0.08`、`sigma = 0.012` の閉形式で作る。適合点は（満期, 債券満期, 行使価格倍率）= (0.5, 2, 0.99)、(1, 3, 1.00)、(2, 5, 1.01)、未使用点は (3, 7, 1.00) である。
- 目的関数は `sum((10000 * (model - market))^2)` である。
- 方法は `stats::optim` の L-BFGS-B で、初期値 `(0.05, 0.01)`、下限 `(0.001, 0.0001)`、上限 `(1, 0.1)`、`maxit = 1000`、`factr = 1e3` である。勾配は `optim` の数値差分で、`ndeps` は既定の 1e-3 のままである。

**QuantLib v1.42**

| 項目 | 内容 | ソース |
|---|---|---|
| 目的関数 | `values` は各商品の `calibrationError * sqrt(weight)`、最小化するのはその二乗和 | `ql/models/model.cpp` |
| 誤差の定義 | 既定は `RelativePriceError`（`\|market - model\| / market`）。ほかに `PriceError`（`market - model`）と `ImpliedVolError` | `ql/models/calibrationhelper.cpp` |
| 制約 | `Vasicek` の `a` と `sigma` は `PositiveConstraint`。`HullWhite` の `b` と `lambda` は `NullParameter` で動かない | `ql/models/shortrate/onefactormodels/vasicek.cpp`、`ql/math/optimization/constraint.hpp` |
| 制約違反の扱い | 違反または非有限の値なら、全要素 1e10 を返す。射影はしない | `ql/math/optimization/levenbergmarquardt.cpp` `fcn` |
| 方法 | MINPACK `lmdif`（`factor = 100`、`mode = 1`）。ヤコビアンは前進差分で、刻みは `sqrt(max(epsfcn, MACHEP)) * \|x\|` | `levenbergmarquardt.cpp`、`lmdif.cpp` |
| 停止条件 | `ftol = endCriteria.functionEpsilon()`。`xtol`、`gtol`、`epsfcn` は `LevenbergMarquardt` の引数で既定は 1e-8。評価回数の上限は `maxIterations * (n + 1)`。`EndCriteria` の他の値は使わない | `levenbergmarquardt.cpp` |
| 終了の報告 | `info` 1〜4 は `StationaryFunctionValue`、5 は `MaxIterations`、6 は `FunctionEpsilonTooSmall`。7、8 は例外 | `levenbergmarquardt.cpp` |
| パラメーターの固定 | `fixParameters`（`Projection` で除く） | `ql/models/model.cpp` |

### 比較元との違い【設計案ではなく実装済みの選択】

次はどれも Riemann 独自の選択であり、QuantLib や LagrangeFinance の写しではない。

| 項目 | QuantLib | LagrangeFinance | Riemann |
|---|---|---|---|
| 変数 | `a`、`sigma` そのもの | `a`、`sigma` そのもの | **`ln(a)`、`ln(sigma)`**。`a` の正は変数変換で保つ |
| 入力データ | ボラティリティの気配を helper が価格にする | 閉形式で作った合成価格 | 呼び出し側が渡す価格の数値 |
| 制約 | 正（`p > 0`）。違反は罰則 1e10 | 箱型の上下限 | **`a` の下限 1.49e-8（`sqrt(MACHEP)`）**。上限なし。下限で止まると `at_lower_limit` を立て、`sigma` だけ動かす |
| `a = 0` | **不可**。コンストラクターが `invalid value` で拒否し（2026-11-03 実行確認。`a = -0.01` も同じ）、探索でも罰則になる。`a < 1.49e-8` の価格は `a = 0` の極限と同じ | **不可**。下限 0.001 | 校正では**不可**（`ln(a)` は 0 に届かない）。`fix_mean_reversion = TRUE` なら `a = 0` を指定でき、ここは QuantLib より広い |
| 1 回の動き | 信頼領域 | L-BFGS-B の直線探索 | **対数で ±2 まで**。計算できない試行は棄却（罰則は使わない） |
| 目的関数 | 既定は相対価格誤差 | 価格差の 1 万倍の二乗和 | **`sum(weight * (model - market)^2)`**。`weight = 1 / market^2` で QuantLib の既定、`weight = 1e8` で LagrangeFinance と同じ値になる。インプライド・ボラティリティ誤差は未対応 |
| 微分 | 前進差分 | 数値差分 | **自動微分**（`r*` は陰関数定理） |
| 方法 | `lmdif` | L-BFGS-B | **Marquardt の減衰を、列を単位長にそろえた連立方程式に入れる Levenberg–Marquardt**。`calibrate_sabr` と同じ手順に、列のスケーリングと対数の動きの制限を足した |
| 停止 | `ftol`、`xtol`、`gtol` 既定 1e-8 | `factr = 1e3` | **目的関数の相対減少 1e-14、対数の動き 1e-10、余弦 1e-10、減衰を 30 回上げても下がらない、`max_iterations`（既定 200）のいずれか**。理由は `stop_reason` で返す。上限への到達はエラーにしない（QuantLib と同じ）ので、呼び出し側で確かめる |
| 初期値 | モデルの現在値 | `(0.05, 0.01)` | 既定は `(0.05, 0.01)`（LagrangeFinance と同じ） |

**下限と対数の動きの制限を入れた理由**：`a` が 0 に向かうと、`ln(a)` についての列も `a` に比例して消える。スケーリングなしでは連立方程式が特異に見え、制限なしでは `ln(a)` の歩幅が際限なく伸びて試行が棄却され続け、`sigma` まで動かなくなった。実際に、最初の実装はこのため目的関数が QuantLib より悪い点で止まった【実行照合済み】。

**割引債オプションの式も QuantLib と変えた**：QuantLib は `c` の 6 個の指数を足し引きし、`a < sqrt(MACHEP)` で `a = 0` の式に切り替える。Riemann は同じ式を因数分解し、`sigma * sqrt(T * phi(2 a T)) * exp(-a (s - T)) * B(s, S)`、`phi(y) = (1 - exp(-y)) / y` として小さい `y` では級数で計算する。`B` も同じ `phi` を使う。切り替えがなくなり、`a = 0` まで値と微分が連続である。`a = 0` でも評価できる。

### 照合結果【実行照合済み】（2026-11-03）

入力はすべて合成である（`tools/golden/p11_hull_white_calibration.R`、`crates/riemann/tests/golden_hull_white.rs`）。スワップション 4〜8 本の組が 20 組、それぞれ価格誤差、相対誤差、`a` 固定の 3 通りで 60 件である。

| 確認 | 件数 | 最大の差（絶対） |
|---|---|---|
| QuantLib の `a` が 1e-3 を超える校正の `a` | 11 | 3.8e-5 |
| 同じく `sigma` | 11 | 3.2e-6 |
| `a` 固定の `sigma` | 20 | 7.5e-9 |
| QuantLib のパラメーターでの Riemann と QuantLib の価格（`a > 1e-3`、固定を含む） | 31 | 4.5e-9 |
| 同じ比較（`a <= 1e-3`） | 29 | 4.5e-5（下の所見） |


**各自の推定値での目的関数と再評価の一致**：それぞれ自分の価格で評価した値を比べた。

| 区分 | 件数 | `a` の差 | `sigma` の差 | 目的関数の相対差（QuantLib / Riemann - 1） | 商品ごとのモデル価格の最大差 | 判断 |
|---|---|---|---|---|---|---|
| `a` が内部解 | 11 | 3.8e-5 | 3.2e-6 | -2.4e-7〜+1.6e-7 | 1.1e-6 | 一致（最小点は同じ） |
| `a` 固定 | 20 | （固定） | 7.5e-9 | -2.5e-7〜+7.9e-8 | 3.1e-8 | 一致 |
| `a` が 0 側の境界 | 29 | 比べない | 約 2.0e-4 | -2.1e-3〜+2.3e-2 | 1.3e-3 | **一致しない** |

境界の 29 件の不一致は、下の「平均回帰がほぼ 0 になる理由」のとおり、許容 0 の外に最小点があって両者が別の点で止まるためである。

**平均回帰がほぼ 0 になる理由**：29 件で Riemann は下限で止まり、QuantLib は `a` が 9e-6〜3.8e-4 の点で止まった。`a` ごとに `sigma` を合わせ直した目的関数 `J(a)` を調べた。

- **最適点は境界にある**：29 件すべてで `J(a)` は `a = 0` から増える（傾きが正）。`J(1e-4)`、`J(1e-3)`、`J(0.01)` の `J(0)` からの増加は、それぞれ 0.01〜0.45%、0.14〜4.5%、1.3〜49% である。特定しにくくて止まったのではない。
- **負の `a` を求めている**：`a = 0` 付近の 2 次式で外挿した最小点は 28 件で負である（残る 1 件は曲率が負で外挿に意味がない）。外挿による推定であり、負の `a` の価格は計算していない【未確認】。
- **ただし弱く特定されている面もある**：`a` と `sigma` のヤコビアン列の相関の絶対値は、境界の 29 件で 0.85〜0.99、内部解の 11 件で 0.93〜0.997 と高い。内部解でも、推定 `a = 0.0023` の例（ケース 59）は `J(0)` との差が 0.022% しかなく、推定 `a = 0.0766` の例（ケース 22）は `a = 0.125` まで目的関数の増加が 1% 以内である。`a` の値を単独で読むのは危うい。
- **原因はこの入力である**：golden の Black ボラティリティは満期と無関係な乱数（0.25〜0.45）である。正の `a` は遠い行使の変動を下げるので、遠い行使が高い組では `a = 0` 側に寄る。比較文書の例でも、同じ日付と行使価格で、満期とともに下がるボラティリティなら `a = 21.4%` の内部解、上がるボラティリティなら境界である。市場で `a` が 0 になるという意味ではない。
- **下限の影響**：`J(1.49e-8)` は `J(0)` より相対で最大 6.7e-7 大きい。`ln(a)` では 0 に届かないことの差である。厳密に `a = 0` で合わせるなら `a` を固定する。
- **QuantLib 側**：比較文書の例では、QuantLib の目的関数は `J(0)` より 0.56% 大きく、その `a` だけで説明できる増加（1e-4 で 0.12%）を上回る。`sigma` も最小点に届いていない。丸め誤差と罰則のどちらが原因かは切り分けていない【未確認】。

- **目的関数**：どちらのパラメーターも Riemann の価格で評価すると、60 件すべてで Riemann の目的関数は QuantLib の点以下である（相対差の最大 -6.4e-15）。
- **局所最小**：60 件すべてで、自由なパラメーターを ±0.1% 動かしても目的関数は下がらない。
- **自己回復**：自分の価格から `a`、`sigma` を戻せる（`a` 1e-9 以内。スワップションは 3 つの初期値から 1e-8 以内）。
- QuantLib の許容誤差を 1e-12 まで詰めても、Riemann との差は縮まらなかった（`a` で最大 1.2e-4、golden の `a_tight`）。差分の刻みが小さくなり、QuantLib の `r*` の探索精度 1e-8 に近づくためと考えるが、原因の切り分けはしていない【未確認】。

既存の照合の最大差は、式の変更の後も変わらない（European 6.49e-9、Bermudan 1.03e-5、コール債 8.4e-13）。

### 比較文書での扱い

`comparison/chapters/ch11_hull_white.qmd` の Calibration 節に載せた。判定するのは、LagrangeFinance の推定値での価格 4 件、再評価誤差 4 件、目的関数 1 件の計 9 件で、すべて PASS である。推定値そのものは最適化手法が違うので N/C とした。

### 比較元についての所見【実行照合済み】

**LagrangeFinance の推定値は最小点の手前である。** 適合点の目的関数は `(0.08, 0.012)` で 1.5e-24 だが、II-6.4 の結果は `a = 0.07748`、`sigma = 0.01193`、目的関数 0.0348 で止まる。同じ目的関数と上下限で `optim` の制御だけを変えた結果が次である。

| 制御 | `a` | `sigma` | 目的関数 |
|---|---|---|---|
| II-6.4 のまま（`factr = 1e3`、`ndeps` 既定 1e-3） | 0.07747874 | 0.01193290 | 3.48e-2 |
| `factr = 10` | 0.07747879 | 0.01193291 | 3.48e-2 |
| `ndeps = 1e-5` | 0.07999979 | 0.01199999 | 2.65e-10 |
| `ndeps = 1e-6` | 0.08000000 | 0.01200000 | 1.62e-14 |
| `parscale = c(0.1, 0.01)` | 0.07999999 | 0.01200000 | 5.80e-12 |

停止条件を詰めても変わらず、数値勾配の刻みかスケールを変えると解に届く。`sigma` が 0.012 程度なのに刻みが 1e-3 なので、勾配が粗いことが原因である。LagrangeFinance は変更していない。

**QuantLib の割引債オプションは `a` が小さいと丸め誤差に支配される。** 5 年後行使の 5 年スワップション（名目 1、`sigma = 0.01`）を `JamshidianSwaptionEngine` で評価した。

| `a` | 値 | `a` を相対 1e-6 ずつ 8 回動かしたときの値の幅 |
|---|---|---|
| 1e-3 | 0.0369298 | 1.4e-9 |
| 1e-5 | 0.0371017 | 7.7e-6 |
| 1e-6 | 0.0351163 | 2.3e-2 |
| 3e-7 | 0.0017930 | 2.0e-1 |
| 1e-7 | 0.6014708 | 6.4e-1 |
| 1.4e-8（切り替えの下） | 0.0371081 | 0 |

真の値は `a` に対して滑らかで、`a -> 0` で 0.0371081 である。`c` は `a` の 3 乗の大きさになるのに、1 程度の指数の差で計算するためである。`a` がこの領域に入る校正では、QuantLib の目的関数も雑音を含む。

## 2. SABR

Hagan らの展開式で、対数正規と正規の両方を返す（`compute_sabr_volatility`）。フォワードと行使価格は正でなければならない。シフト付きは未実装である。

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

LagrangeFinance II-8 と同じ Gauss–Hermite 法（24 点）は、行使の折れ目があるときに精度が落ちる。Riemann の単体テストでは、1 回行使のスワップションで Jamshidian と 1.3e-4 の差が出た（48 点、状態 1201 点）。台形則（幅 0.02）に変えると 1e-5 以内に入った。`compute_hull_white_bermudan_swaption_value` の既定を台形則にしたのは、このためである。コール債の既定は、LagrangeFinance と同じ値を再現するために Gauss–Hermite 24 点のままとする。方法ごとの差は検証 QMD に示す。LagrangeFinance は変更していない。

## 6. 未対応

- BGM/LMM、HJM、FMM（LagrangeFinance II-7）。
- Sobol 列と Longstaff–Schwartz 法（II-4.6）。
- MBS のプリペイメントと OAS（II-8.8）。
- キャップ・フロア。IBOR の扱いは対象外とした。
