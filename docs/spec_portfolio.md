# P12 制約付きポートフォリオ最適化とリスク寄与

- 作成：2026-11-14（ユーザーの指示「RIEMANにも ガウスのように ポートフォリオアナライザーの機能をつけるべき」に対する設計）。実装と照合：2026-11-15（結果は 6 章）
- 対象：LagrangeFinance I-8「制約付きポートフォリオ最適化」（`part1_empirical/ch08_constrained_portfolio_optimization.qmd`、コミット `b9ddc2a`）と、I-9 以降で共通に使うリスク尺度（ボラティリティ、VaR、ES、リスク寄与）
- 比べる相手：GaussQuant（コミット `c08fdb1`、版 2026.6.18.1）の `portfolio_spec_GQL()`・`portfolio_constraint_GQL()`・`portfolio_optimize_GQL()`、quadprog、Rglpk、閉じた式、最適性条件（KKT）。原稿 I-8.8 は `portfolio_frontier_GQL()` の名前を挙げるが、GaussQuant にこの関数はなく、原稿も `target_return` の繰り返しで Frontier を作る
- 位置づけ：第 I 部のうち、正解を解析解・最適性条件・独立な計算で確かめられる部分を Riemann に入れる（`CLAUDE.md` の決定事項、2026-11-14 に変更）。回帰・ファクターモデル（I-4〜I-5）とイベントスタディ（I-6）、機械学習（I-7）は R の統計関数で足りるので対象外のまま

## 1. 原稿と GaussQuant の計算【ソース確認】

### 1.1 原稿 I-8 の条件

- データ：`data/derived/portfolios/asset_return_history.parquet`（公開リポジトリにある匿名化した月次リターン）。最初のリバランス年度の時価総額上位 20 銘柄、資産名は `firm_ID`、時点は `month_ID`
- 制約：ウエイトの合計 = 1、各ウエイト 0〜0.20（`weight_sum` と `box`）
- 方法：最小分散、平均分散（`risk_aversion`）、最大リターン、最大シャープ、最小 ES（95%）、リスク・バジェット（均等）、目標リターン、目標リスク、Efficient Frontier（`portfolio_frontier_GQL()`）

### 1.2 GaussQuant の実装（`R/portfolio_*_GQ[LH].R`、GaussQuant の現在のコミット）

| 項目 | 実装 |
|---|---|
| 平均と共分散 | 欠損のない月だけ使い、`colMeans` と `stats::cov`（分母 T−1） |
| 制約 | `weight_sum` は最小値 = 最大値 = 1 だけ。`box` は最後に登録したもの。ほかの種類（group、turnover など）は登録できるが、最適化は拒否する |
| 最小分散・平均分散・目標リターン | `ROI` + `quadprog`（厳密な2次計画）。平均分散の目的は `μ'w − λ w'Σw`（`Q = 2λΣ`）。原稿 I-8.4.2 の式は `μ'w − (γ/2) w'Σw` なので、原稿の `risk_aversion = 5` は γ = 10 にあたる |
| 最大リターン | `ROI` + `glpk`（線形計画） |
| 最大シャープ・最小 ES・リスク・バジェット・目標リスク | `stats::optim` の Nelder–Mead。ウエイトを箱と合計に射影した値で目的関数を評価する。**厳密な最適解ではない**（収束判定は Nelder–Mead のもの、目標リスクは罰則項） |
| ES | 損失 = −リターン。VaR = `quantile(type = 7)`、ES = VaR 以上の損失の平均 |
| リスク寄与 | 分散に対する割合 w_i (Σw)_i / w'Σw（合計 1） |

## 2. Riemann に入れるもの【設計案】

### 2.1 R の公開関数（命名は `CLAUDE.md` の方針に従う。`optimize_` は認めた動詞にないので `compute_` にする）

| 関数 | 内容 |
|---|---|
| `compute_return_moments(returns)` | リターン行列（T×N、列名が資産名）から平均・共分散（分母 T−1）・観測数。欠損のある行は除き、除いた行数を返す |
| `create_portfolio_constraints(assets, weight_sum = 1, lower = 0, upper = 1)` | 制約の値（リスト）。最初は合計と箱だけ。ほかの種類は受け付けずにエラーにする |
| `compute_optimal_portfolio(returns, constraints, method, ...)` | `method` は GaussQuant と同じ名前（`minimum_variance`、`mean_variance`、`maximum_return`、`maximum_sharpe`、`minimum_ES`、`risk_budget`、`target_return`、`target_risk`）。ウエイト、期待リターン、ボラティリティ、ES、リスク寄与、ソルバーの状態（反復回数、KKT の残差、有効な制約） |
| `compute_efficient_frontier(returns, constraints, targets)` | 目標リターンの列ごとの最小分散 |
| `compute_portfolio_risk(weights, returns, confidence_level, method)` | ボラティリティ、VaR・ES（正規分布とヒストリカル）、各尺度のリスク寄与（自動微分によるオイラー配分）。I-9 の市場VaRでも使う |

long 形式の tibble（`date`・`asset`・`return`）から行列への変換は R の側で行う（GaussQuant と同じ列名を既定にする）。

### 2.2 計算方式（Rust の中核）

| 方法 | 方式 | 厳密さ |
|---|---|---|
| 最小分散・平均分散・目標リターン | 凸2次計画の有効制約法（等式1本と箱）。N ≤ 数百を想定した密行列 | 厳密（KKT の残差で確かめる） |
| 最大リターン | 合計と箱だけなら、平均の大きい順に上限まで詰める閉じた解（同順位は資産の順） | 厳密 |
| 最大シャープ | 変数変換 y = κw で「y'Σy の最小化、(μ − r_f)'y = 1、Σy = κ、κ·lower ≤ y ≤ κ·upper、κ ≥ 0」の2次計画にする | 厳密（超過リターンが正の資産があるとき） |
| 目標リスク | ボラティリティ ≤ 目標の下でリターン最大。平均分散のリスク回避度を二分法で動かして Frontier 上の点を求める | 許容誤差内 |
| 最小 ES | Rockafellar–Uryasev の線形計画（変数は N + T + 1）。線形計画のソルバーを自前で書くか crate を使うかは未決定（5 章） | 厳密（RU の ES の定義で） |
| リスク・バジェット | 箱制約なしの均等・指定寄与は、凸な目的 ½w'Σw − Σ b_i log w_i のニュートン法で解き、合計 1 に正規化する。箱が効く場合の扱いは未決定（5 章） | 箱が効かなければ厳密 |
| リスク寄与 | ボラティリティ・正規 VaR・正規 ES は σ(w) の勾配（自動微分）× w。ヒストリカル ES の寄与は、テールの月の各資産の損失の平均（ES の w に関する方向微分） | 自動微分と閉じた式の両方を返して照合する |

### 2.3 ES の定義の違い

GaussQuant の ES（`quantile(type = 7)` 以上の損失の平均）と、Rockafellar–Uryasev の ES（VaR を超える部分を按分する CVaR）は、標本が有限のとき一致しない。`compute_portfolio_risk()` は両方を返し、最適化は RU の定義で行う。照合では、同じウエイトでの GaussQuant の ES を別に計算して並べ、定義の差として記録する。

## 3. 照合の計画【設計案】

| 比べるもの | 相手 | 判定 |
|---|---|---|
| 制約が合計だけの最小分散・接点ポートフォリオ・Frontier | 閉じた式（Σ⁻¹1 / 1'Σ⁻¹1、Σ⁻¹(μ − r_f) の正規化、二基金定理） | 1e-12 程度 |
| 箱つきの全方法 | KKT 条件（勾配、有効な制約、乗数の符号） | 残差 1e-10 程度 |
| 最小分散・平均分散・最大リターン・目標リターン・Frontier | GaussQuant（quadprog、glpk）、原稿 I-8 と同じ 20 銘柄・上限 20% | ウエイト 1e-8 程度 |
| 最大シャープ・最小 ES・リスク・バジェット・目標リスク | GaussQuant（Nelder–Mead） | ウエイトは判定しない。Riemann の目的関数の値が GaussQuant の値以上に良いこと（同じ定義で計算） |
| 最小 ES | R で独立に組んだ RU の線形計画（`ROI.plugin.glpk`） | 目的関数の値 1e-10 程度 |
| リスク寄与 | 閉じた式と中心差分 | 相対 1e-8 程度 |

照合スクリプトは `tools/golden/p12_portfolio.R`、参照値は `tests/golden/portfolio.csv`。Rust のテストが入力を読めるように、原稿の 20 銘柄・59 か月のリターンも参照値のファイルに入れた（LagrangeFinance の公開リポジトリにある匿名化したデータ。市場の気配値ではない）。

## 4. 後続（2026-11-14 のユーザーの方針）

P12 の後、LagrangeFinance 第 I 部の次の章に相当する計算を同じ手順で入れる。

| 段階 | 原稿 | 内容 |
|---|---|---|
| P13 | I-9 市場VaR | 分散共分散法・ヒストリカル法・モンテカルロ法の VaR と ES、リスク寄与、バックテスト。FRTB の ES（原稿 I-9.1、I-9.11 は背景の説明で、標準的方式の資本計算は原稿も扱わない） |
| P14 | I-10 信用VaR | One-Factor Gaussian、条件付き PD、相関デフォルトのモンテカルロ、EL・UL、信用 ES、ASRF。DRC（FRTB のデフォルト・リスク・チャージ）との関係 |
| P15 | I-11 トランチング | Attachment・Detachment、トランシェ損失、期待トランシェ損失、Premium Leg と Loss Leg、MBS の Sequential・PAC・Z（P11-5 の MBS を使う） |

各段階の範囲は、着手時に原稿を読んで決める。

## 5. 決めたこと（2026-11-14 のユーザーの承認「おすすめでいこう」）

1. 線形計画のソルバー（最小 ES）は自前の単体法（`math/simplex.rs`）。依存は増やさない
2. リスク・バジェットで箱制約が効く場合はエラーにする（制約が効かないときだけ解く）
3. 期待リターンは履歴の平均だけ（GaussQuant と同じ）。外から渡す引数は後で必要になったら足す
4. 共分散は標本共分散（分母 T − 1）だけ

## 6. 実装と照合の結果【実行照合済み】

- 照合日：2026-11-15。R 4.6.0、quadprog 1.5.8、Rglpk 0.6.5.1、GaussQuant 2026.6.18.1、RiemannFinMath 0.3.0（このコミットのソースから導入）
- 再実行：`Rscript tools/golden/p12_portfolio.R`（参照値）、`cargo test --release --test golden_portfolio`、`Rscript tools/experiments/p12_portfolio_diffs.R`（下の表の差）

### 6.1 実装

| ファイル | 内容 |
|---|---|
| `crates/riemann/src/math/quadratic.rs` | 狭義凸2次計画の双対有効制約法（Goldfarb–Idnani 1983 の論文から書いた。quadprog と同じ方式で、コードは写していない）。`J = L^-T Q` と `R` を Givens 回転で更新する。停留条件の残差 `max |Gx + a − Σ u_i n_i|` を返す |
| `crates/riemann/src/math/simplex.rs` | 標準形の線形計画の2段階単体法（密なタブロー、Bland の規則）。最後に基底の連立方程式を元の行列で解き直す（タブローの丸めの蓄積を除く） |
| `crates/riemann/src/portfolio.rs` | 平均・共分散、制約、8 つの方法、Frontier、リスク尺度とオイラー寄与（自動微分） |
| `crates/riemann-r/R/portfolio.R` | `create_portfolio_constraints()`、`compute_return_moments()`、`compute_optimal_portfolio()`、`compute_efficient_frontier()`、`compute_portfolio_risk()`。リターンは行列、列ごとの data frame、long 形式（`date`・`asset`・`return`、GaussQuant と同じ列名）のどれでもよい |

### 6.2 参照値との差

5 ケース（原稿 I-8 の 20 銘柄・上限 20%、合成データで制約なし 3 ケース・箱制約あり 2 ケース）、39 の最適化、Frontier 90 点。

| 比べたもの | 最大差 | テストの許容誤差 |
|---|---|---|
| ウエイト：quadprog・Rglpk・閉じた式・R のニュートン法との差（最小 ES 以外の全方法） | 1.7e-12（合成データ 5 の平均分散。共分散の条件数 2.4e6） | 1e-10 |
| 最小 ES の値：Rglpk の線形計画の最適値との差 | 8.3e-17 | 1e-12 |
| Frontier の各点のウエイト | 1e-10 未満 | 1e-10 |
| リスク尺度（ボラティリティ、正規 VaR・ES、ヒストリカル VaR・ES）：R の閉じた式との相対差 | 1e-13 未満 | 相対 1e-13 |
| リスク寄与（自動微分）：閉じた式との差 | 4.2e-16 | 1e-14 |
| 2次計画の停留条件の残差 | 4.5e-13 | 1e-12 |
| 原稿 I-8 で GaussQuant が厳密に解く 4 方法（最小分散、平均分散 γ = 10、最大リターン、目標リターン）のウエイト | 3.7e-16 | 1e-10 |

**許容誤差を 0 にしなかった理由**：独立な計算（別の解法、R の線形代数）との浮動小数点の差。ウエイトの差は共分散の条件数とともに大きくなる。

**最小 ES のウエイトは判定しない**：線形計画の最適解はウエイトについて一意とは限らないので、最適値と、そのウエイトで計算し直した ES を比べる。

**比べなかった値**：最小 ES のウエイトでの `tail_mean_es`（VaR 以上の損失の平均、GaussQuant の定義）。最小 ES の解では多くの月の損失が VaR に並ぶため、VaR 以上に入る月の数が丸めの差で変わる（合成データ 5 で R と 0.0149 と 0.0148）。

### 6.3 原稿 I-8 で GaussQuant が Nelder–Mead で解く 4 方法

同じ入力（59 か月、20 銘柄、ウエイト 0〜0.20、合計 1）で、GaussQuant 自身の目的関数で比べた。

| 方法 | RiemannFinMath | GaussQuant |
|---|---|---|
| 最大シャープ（r_f = 0）のシャープレシオ | 0.491019 | 0.415700 |
| 最小 ES（95%）：Rockafellar–Uryasev の ES | 0.054963 | 0.068649（GaussQuant のウエイトで計算） |
| 同じウエイトでの、VaR 以上の損失の平均（GaussQuant の定義） | 0.054524 | 0.068515 |
| 均等リスク・バジェット：寄与の割合の 1/20 からの最大のずれ | 2.8e-17 | 7.0e-3 |
| 目標リスク（ボラティリティ 0.047583 以下）でのリターン | 0.022576（ボラティリティ 0.047583） | 0.018443（ボラティリティ 0.047555） |

GaussQuant の最大シャープは、同じ入力の平均分散ポートフォリオ（シャープ 0.490）より低い。Nelder–Mead が 20 次元で最適解まで進まないためで、GaussQuant の実装の性質であり、原稿の説明（制約付き最適化の考え方）とは別の話である。

### 6.4 途中で直したこと

1. **単体法の誤った「非有界」**：第 1 段で、丸めだけで −1e-11 になった被約費用の列を選び、正の要素がない（7e-12）ので非有界と判定していた。被約費用の許容誤差をコスト行の大きさに比例させ、丸めの大きさの負の被約費用で軸要素のない列は飛ばすようにした
2. **単体法の解の精度**：合成データ 5 で、タブローの丸めが蓄積してウエイトの合計が 1 からずれた。最終的な基底で元の行列の連立方程式を解き直すようにした
3. **正規分布の分位点**：QuantLib の逆関数の近似（相対誤差 1e-9 程度）で、正規 VaR が R の `qnorm` と 6.6e-11 違った。分布関数でのニュートン法を 2 回加えた
4. **リスク・バジェットの収束**：最適解の近くで目的関数の差が丸めより小さくなり、直線探索がステップを縮め続けて上限の 200 回まで反復していた（割合のずれ 3e-10）。ニュートン減少量が小さいときは正値だけを確かめて全ステップを取り、減少量で止めるようにした（7〜8 回、ずれ 1e-16）

## 7. 未対応

- 箱制約が効くリスク・バジェット、グループ・回転率・取引コスト・ファクター制約（GaussQuant も登録だけで最適化は拒否する）
- 期待リターンを外から渡すこと、共分散の縮小推定
- 大きな問題（数百銘柄を超える場合）向けの疎な解法。今の2次計画と単体法は密行列
