# 自動微分の仕様（草案）

自動微分は **finmath-lib の方式を参考にする**。特に、確率変数（`RandomVariable`）、AAD の計算グラフと逆伝播、モンテカルロの条件付き期待値の扱いを調べ、Riemann の関数型の設計にどう取り込むかを示す。

参照したソース：finmath-lib `9bd054ff694026760e46371ef61b96a4dbd7b8b0`（2026-08-29、6.1.10-SNAPSHOT）。以下のパスは `src/main/java/net/finmath/` からの相対パスである。

## 記述の区分

本書では、根拠の確かさを次の三つで区別する。

| 区分 | 意味 |
|---|---|
| 【ソース確認】 | QuantLib・finmath-lib のソースを読んで確かめた事実、またはソースを読んでの判断。プログラムを実行してはいない |
| 【実行照合済み】 | Riemann の実装を実行し、基準値と照合した結果 |
| 【設計案】 | Riemann としてそうする、という設計。未実装・未検証であり、照合で確かめるまでは目標にとどまる |

**現時点（2026-10-31）で【実行照合済み】の項目は、P1 の日付・休日カレンダー・営業日調整・スケジュール生成・日数計算（[spec_dates_calendars.md](spec_dates_calendars.md) の 2.3、3.4、4.3、5.3、6.3 節）と、P2-1〜P2-4 の固定利付債のキャッシュフロー・経過利子・利回りと価格・割引カーブによる評価・日本式の利回り（[spec_bonds.md](spec_bonds.md) の 2.2 節・3.3 節・4.3 節・5.3 節）だけである。** それ以外の Riemann の動作に関する記述は【設計案】である。

## 1. finmath-lib の方式【ソース確認】

### 1.1 確率変数 `RandomVariable`

| 項目 | 内容 | 根拠 |
|---|---|---|
| 中身 | 「パスごとの実現値の配列」と「フィルトレーションの時刻」の組。値が確定している場合は配列を持たず、1つの値（スカラー）として持つ | `montecarlo/RandomVariableFromDoubleArray.java`（`time`、`realizations`、非確率的な場合の値） |
| 演算 | 足し算・掛け算・`exp`・`log`・`floor`・`cap`・`choose`（指示関数）などは、すべて新しい確率変数を返す。元の値は書き換えない | `stochastic/RandomVariable.java` |
| 期待値など | `getAverage`（重み付きも）、分散、標準誤差、`getConditionalExpectation(estimator)` | 同上 |

MC の価格計算は、パスごとのループではなく、この「パス方向のベクトル」どうしの演算として書かれる。

### 1.2 AAD の計算グラフ（`montecarlo/automaticdifferentiation/backward/RandomVariableDifferentiableAAD.java`）

- 微分可能な確率変数は、値（`RandomVariable`）と計算グラフのノード（`OperatorTreeNode`）を持つ。
- ノードは、ID、演算の種類（`OperatorType`）、引数のノード、偏微分の計算に必要な引数の値、演算のパラメーターを持つ。
- 演算の種類は列挙型で、`ADD`・`MULT`・`DIV`・`EXP`・`LOG`・`CAP`・`FLOOR`・`CHOOSE`・`DISCOUNT`・`ACCRUE`・`POW`・`AVERAGE`・`VARIANCE`・`STDERROR`・`CONDITIONAL_EXPECTATION` など 33 種類ある。
- ID は、プログラム全体で共有するカウンター（`static AtomicLong indexOfNextRandomVariable`）で、作られた順に振られる。後から作られたノードほど ID が大きい。

### 1.3 逆伝播（`getGradient`）

1. 出力のノードに随伴（自分自身に対する微分 = 1）を置く。
2. 未処理のノードを ID の順に並べた表（`TreeMap`）から、ID の最も大きいノードを取り出す。ID は作られた順なので、これで依存関係の逆順に処理できる。
3. ノードの随伴に、演算ごとの偏微分（`getPartialDerivative`、演算の種類で分岐）を掛け、引数のノードの随伴に足し込む（`addProduct`）。随伴はパスごとのベクトルである。
4. 設定により、途中のノードの随伴を捨てて、葉（入力）の随伴だけを残す（`isGradientRetainsLeafNodesOnly`）。メモリーを節約するためである。

### 1.4 確率的な演算の特別な扱い

finmath-lib は、確率的な AAD（Fries の論文 SSRN 2995695、3000822、3282667）を実装している。

| 演算 | 逆伝播での扱い | 意味 |
|---|---|---|
| `AVERAGE`（期待値） | 随伴をパス方向に平均する | 期待値の演算の随伴は期待値 |
| `CONDITIONAL_EXPECTATION` | 随伴に、順方向と**同じ**条件付き期待値の推定量（`ConditionalExpectationEstimator`）を当てる | 条件付き期待値は射影なので、随伴も同じ射影になる。バミューダン型の行使判断などで、回帰による推定をそのまま微分に使える |
| `CHOOSE`（指示関数） | 設定により、ディラックのデルタ関数を近似する（`DiracDeltaApproximationMethod`：`DISCRETE_DELTA`、`REGRESSION_ON_DENSITY` など） | 不連続な支払い（デジタル、バリア、行使の境界）の微分を、0 にせず近似する |

### 1.5 モンテカルロの条件付き期待値

- インターフェース `stochastic/ConditionalExpectationEstimator` は、ある時点の情報 Z を固定した `E( · | Z )` を表す。
- 実装の `montecarlo/conditionalexpectation/MonteCarloConditionalExpectationRegression.java` は、基底関数（`RegressionBasisFunctions`）への回帰で推定する。推定用と予測用で別のパスの基底関数を使うこともできる。正規方程式の行列 XᵀX を特異値分解（Apache Commons Math の `SingularValueDecomposition`）して解く。
- バミューダン・オプション（`montecarlo/assetderivativevaluation/products/BermudanOption.java`）は、この推定量で継続価値を求め、行使判断に使う。

## 2. Riemann に取り込むうえでの見直し点（左の2列は【ソース確認】、右の列は【設計案】）

| finmath-lib の作り | 問題 | Riemann での見直し |
|---|---|---|
| ノードの ID を全体共有のカウンターで振る | グローバル状態（R8 に反する）。並列に記録すると、ID の並びがスレッドの実行順で変わる | ID は、一つの計算グラフの中での位置（配列の添え字）とする。グラフは計算ごとに持ち、全体で共有しない |
| ノードどうしを参照でつなぎ、逆伝播のときに `TreeMap` で並べ直す | 並べ直しの手間がかかる | ノードを記録した順に配列（アリーナ）へ積む。配列の順がそのまま依存関係の順なので、逆伝播は配列を後ろから1回たどるだけで済む |
| 回帰の解法をキャッシュし、ロックで守る（`solverLock`） | 内部可変性（R7 に反する） | 回帰は「基底関数と値から、係数を返す関数」とし、当てはめた結果（係数）を書き換えない値として持つ |
| 演算の種類を列挙型にし、偏微分を `switch` で分岐する | — | そのまま取り入れる（R4 の `enum` と `match` に合う） |
| 確率変数はパス方向のベクトルで、演算は新しい値を返す | — | そのまま取り入れる（R1 の値と合う） |
| 確定値はスカラーとして持つ | — | そのまま取り入れる（無駄な配列を作らない） |

## 3. Riemann の設計案【設計案】

### 3.1 確率変数

```rust
pub struct Rv { pub filtration_time: f64, pub repr: RvRepr }
pub enum RvRepr { Deterministic(f64), Paths(Arc<[f64]>) }   // 書き換えない値
pub fn add(a: &Rv, b: &Rv) -> Rv;                            // 演算は新しい値を返す
pub fn expectation(a: &Rv) -> f64;
```

### 3.2 計算グラフ

```rust
pub struct NodeId(u32);                        // グラフの中での位置
pub enum Op { Add, Mult, Div, Exp, Log, Floor, Cap, Choose(DiracDelta), Average,
              ConditionalExpectation(Arc<Regression>), /* .. */ }
pub struct Node { pub op: Op, pub args: [Option<NodeId>; 3], pub arg_values: Vec<Rv> }
pub struct Tape { nodes: Vec<Node> }           // 記録した順に積む。記録後は書き換えない

// 記録：可変なのは記録中の局所の作業領域だけで、結果は書き換えない Tape になる
pub fn record<F>(inputs: &[Rv], f: F) -> (Tape, Vec<NodeId>, NodeId) where F: FnOnce(&mut Recorder, &[Var]) -> Var;

// 逆伝播：副作用のない関数。配列を後ろから1回たどる
pub fn gradient(tape: &Tape, output: NodeId, wrt: &[NodeId]) -> Vec<Rv>;
```

- 記録の途中だけ、局所の作業領域（`Recorder`）を `&mut` で書き換える。これは共有されないので、R7 の「内部可変性を使わない」には反しない。記録が終われば、`Tape` は書き換えない値で、`Send + Sync` になる（R11）。
- 並列化は、計算グラフの単位（シナリオ、評価日、商品）で行う。それぞれが自分の `Tape` を持つので、ID の並びは実行順に依存しない。これにより結果が再現する設計とするが、未実装であり、実際に再現するかは仕様書 5.2 のテストで確かめる。
- パス方向のベクトル演算は、`Rv` の中で並列化してよい（決まった大きさの区画に分けて計算し、集計の順序を固定する）。

### 3.3 確率的な演算

| 演算 | Riemann での扱い |
|---|---|
| 期待値 | 随伴をパス方向に平均する（finmath と同じ） |
| 条件付き期待値 | `Regression`（基底関数の作り方と、当てはめた係数）を書き換えない値として演算に持たせる。順方向と逆伝播の両方で、同じ `Regression` を当てる |
| 指示関数 | ディラックのデルタ関数の近似方法を値（`enum DiracDelta { Ignore, Discrete { width_per_stddev }, RegressionOnDensity { .. } }`）で選ぶ。利用者が選ぶ設定であり、型で選ばない（R6） |

回帰の解法は、特異値分解または QR 分解のうち一つに決め、計算の順序を固定する。同じ入力から常に同じ係数が出るようにする。

### 3.4 数値計算のコードを共有する方法

同じ計算（割引、補間、MC の支払いなど）を、微分なしの高速な計算と、計算グラフを記録する計算の両方で使えるようにする。

- 内部に小さな trait（`Operand`：足し算・掛け算・`exp` などの演算）を置き、`f64`・`Rv`・記録用の `Var` で実装する。数値計算のコードは、この trait について総称的に書く。
- これは仕様書 R6 が許す「内部の型の抽象化」に当たる。公開 API には出さず、利用者は `Method`（例：`Method::MonteCarlo { greeks: Greeks::Aad }`）などの値で、感応度を求めるかどうかを選ぶ。

### 3.5 確かめ方（いずれも未実施）

| 確かめる内容 | 方法 |
|---|---|
| 偏微分の正しさ | 演算ごとに、数値差分と比べる（性質のテスト） |
| 確定的な計算の感応度 | 債券・カーブのバケット DV01 を、数値差分と LagrangeFinance III-6（金利リスクと自動微分）の結果で照合する |
| MC の感応度 | ヨーロピアン・オプションのデルタ・ベガを、解析解と LagrangeFinance III-5（オプションのグリークスと自動微分）で照合する |
| 条件付き期待値を含む感応度 | バミューダン・オプションのデルタを、数値差分（同じ乱数）と比べる |
| 再現性 | 同じ入力・同じ種で、スレッド数を変えても同じ勾配になること（仕様書 5.2） |

## 4. 導入の段階【設計案】

**現状（2026-11-01）**：自動微分は未実装である。下の 3.4 の trait は P2 の時点で入れる予定だったが、入れていない。債券、カーブ、OIS の計算は `f64` で書かれている。P5 で、この直しが最初の作業になる。

**torch の置き換えに必要なもの**【ソース確認】：LagrangeFinance の第 III 部は、R の `torch` パッケージで次を使っている。

| 使い方 | 場所 | 対応 |
|---|---|---|
| 逆モードの微分（`backward`） | III-4〜III-7 | P5 |
| 2 階微分（`create_graph = TRUE`） | III-5 | P5 で設計を確かめる。上の設計案は 1 階だけなので、追加の検討が要る |
| 多数のパスの一括計算（III-5 で 20 万パス、III-7 で 2 万パス） | III-5、III-7 | P6 のベクトル単位の計算グラフ |
| 微分できないペイオフの平滑化 | III-5 | P6 |
| GPU（本文の説明） | III-4、III-7 | 代替しない。CPU の並列処理で十分かを実測する |

torch はコードで精度を指定していないので、単精度で動いている可能性がある（未確認）。ビットの一致は求めず、解析解、数値差分、GaussQuant の結果を基準にする。

段階の番号は仕様書 7 章と共通である。

| 段階 | 内容 | 前提 |
|---|---|---|
| P5 | 確定値（`f64`）の計算グラフと逆伝播。債券価格とカーブのバケット感応度 | 仕様書の P2・P3（債券・OIS カーブ） |
| P6 | `Rv`（パス方向のベクトル）の計算グラフ。MC のヨーロピアン・オプション | MC の基盤（P6 で作る） |
| P9 | 条件付き期待値（回帰）と指示関数の近似。バミューダン・オプション | P6 |

3.4 の trait は P5 の最初に入れる。これより後の計算を増やすほど、書き直しが大きくなるためである。
