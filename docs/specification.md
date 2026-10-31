# Riemann 設計仕様書（草案 v0.8）

Riemann は、Rust で一から作る金融計算ライブラリーである。主な利用言語は **R と TypeScript** とし、同じ中核をほかの言語からも使えるようにする。

主題は、既存の金融ライブラリーを支えてきた二つの仕組みからの脱却と、Rust の性質を活かした作りである。

1. **オブジェクト指向からの脱却**：継承、仮想関数、状態を持つオブジェクト、オブザーバーをやめ、**値（データ）と関数**だけで組み立てる。
2. **テンプレートからの脱却**：利用者が選ぶ金融上の選択肢（補間方式、日数計算、評価手法など）を、型の引数（C++ のテンプレート、型によるメタプログラミング）で表すのをやめ、**実行時の値として渡す**。Rust のジェネリクスを全面的に禁止するものではなく、自動微分など内部の数値計算に必要な型の抽象化は使える（R6）。
3. **Rust の高速性とスレッド安全性を活かす**：ネイティブの速度で計算し、言語が保証するスレッド安全性（所有権と `Send`／`Sync`）の上で並列に評価する。計算の中核を一つにして、**R と TypeScript（Node.js とブラウザー）から、計算ごとに定めた許容誤差の範囲で同じ結果が得られる**ようにする（7.3）。Python などは、同じ決まりで後から足せる形にしておく。

1 と 2 で作った「書き換えない値と、副作用のない関数」は、3 の並列化と多言語対応の土台にもなる。共有するデータを誰も書き換えないので、スレッド間で安全に共有でき、言語の境界もデータの受け渡しだけで済む。ただし、土台があることと、計算が正しく再現できることは別であり、後者はテストで確かめる（6.3）。

本書は実装前の仕様の草案であり、コードはまだない。

## 記述の区分

本書では、根拠の確かさを次の三つで区別する。

| 区分 | 意味 |
|---|---|
| 【ソース確認】 | QuantLib・finmath-lib のソースを読んで確かめた事実、またはソースを読んでの判断。プログラムを実行してはいない |
| 【実行照合済み】 | Riemann の実装を実行し、基準値と照合した結果 |
| 【設計案】 | Riemann としてそうする、という設計。未実装・未検証であり、照合で確かめるまでは目標にとどまる |

**現時点（2026-10-31）で【実行照合済み】の項目は、P1 の日付・休日カレンダー・営業日調整・スケジュール生成・日数計算（[spec_dates_calendars.md](spec_dates_calendars.md) の 2.3、3.4、4.3、5.3、6.3 節）と、P2-1〜P2-4 の固定利付債のキャッシュフロー・経過利子・利回りと価格・割引カーブによる評価・日本式の利回り（[spec_bonds.md](spec_bonds.md) の 2.2 節・3.3 節・4.3 節・5.3 節）だけである。** それ以外の Riemann の動作に関する記述は【設計案】である。


## 1. 何から脱却するのか

参考にした QuantLib（C++）と finmath-lib（Java）は、どちらも優れたライブラリーだが、設計の骨格はオブジェクト指向とテンプレートにある。数は本体のソースから数えた（QuantLib `70f240f`、finmath-lib `9bd054f`）。

| 仕組み | QuantLib | finmath-lib | 問題 |
|---|---|---|---|
| 継承 | `class X : public Y` が 1,384 か所、`virtual` が 961 か所（`ql/` のヘッダー） | `extends` が 446 か所、抽象クラスが 61、インターフェースが 164 | 振る舞いがクラス階層に散らばり、ある計算がどこで行われるかを追いにくい |
| テンプレート | `template <` が 1,715 か所（253 本のヘッダー） | ジェネリクスは補助的 | 選択肢が型に埋め込まれ、実行時やR から選べない。エラー表示が読みにくく、組み合わせごとにコードが生成される |
| 型で選ぶカーブ | `PiecewiseYieldCurve<Traits, Interpolator, Bootstrap>`：割引係数かゼロ金利か、補間方式、解き方を型の引数で決める | `DiscountCurveInterpolation extends CurveInterpolation implements DiscountCurve` | 「対数線形の割引係数」という設定が、データではなく型になっている |
| CRTP・ビジター | `ql/patterns/curiouslyrecurring.hpp`、`ql/patterns/visitor.hpp` | — | 型の仕組みで分岐を表すため、分岐の全体が見えない |
| 状態を持つオブジェクト | `Handle`／`RelinkableHandle` の付け替え、`Observer`／`Observable`、`LazyObject` | 変更した複製を返す（`addCurve`、`getCloneWithModifiedData`） | QuantLib では、値を変えると見えないところで再計算が走る |
| グローバル状態 | `Settings` シングルトンの評価日、`IndexManager` の Fixing | なし（評価時点は引数） | 同じ関数呼び出しでも、評価日の設定によって結果が変わる |
| 工場（Factory） | `MakeOIS`、`MakeSchedule` などのビルダー | Factory クラスが 33 | オブジェクトを組み立てるための仕組みが、計算と別に必要になる |
| 手法ごとの商品クラス | 商品は一つで、エンジンを付け替える | `EuropeanOption` が MC・FD・フーリエ・ツリー・株式の 5 か所にある | 同じ商品が評価手法ごとに別の型になる |

## 2. Riemann の原則

| 番号 | 原則 | 内容 |
|---|---|---|
| R1 | データは値 | 日付、規約、気配、カーブ、商品、結果は `struct` と `enum` の値とする。作ったあと書き換えない |
| R2 | 計算は関数 | すべての計算は「入力をすべて引数で受け取り、結果を返す」関数とする。隠れた入力（グローバル状態、オブジェクトの内部状態）を持たない |
| R3 | 選択肢は値 | 補間方式、日数計算、評価手法などの選択肢は `enum` の値で表し、実行時に選ぶ。型の引数で選ばない |
| R4 | 分岐は `match` | 種類による違いは `enum` と `match` で書く。新しい種類を足したとき、対応していない箇所をコンパイラーが見つける |
| R5 | 継承しない | クラス階層、trait オブジェクトの階層、仮想関数による振る舞いの差し替えを使わない |
| R6 | 選択肢を型にしない | 利用者が選ぶ金融上の選択肢（補間方式、日数計算、カレンダー、評価手法など）を、型の引数や trait の実装の差し替えで表さない。公開 API は値で受け取り、利用者がジェネリクスを意識せずに使えるようにする。一方、内部の数値計算では、自動微分のためのスカラー型の抽象化など、必要な型の抽象化を使ってよい。その場合は、使う理由と範囲をコードの説明に書き、公開 API の型には出さない（入口では `f64` などの具体的な型にする）。マクロで DSL は作らない |
| R7 | 更新は新しい値 | 市場データの変更は「古い値から新しい値を作る関数」で表す。変わっていない部分は `Arc` で共有する。中核では `RefCell`・`Mutex` などの内部可変性を使わない |
| R8 | 状態は引数で渡す | 評価日、Fixing、設定は `ValuationContext` として明示的に渡す |
| R9 | エラーも値 | panic せず `Result` を返す。エラーには、原因になった入力（気配の ID、日付など）を持たせる |
| R10 | 型は意味のために使う | 型は、単位と意味の取り違えを防ぐためだけに使う（`Rate`、`Bp`、`Price`、`Money`、`Date`）。振る舞いの組み替えには使わない |
| R11 | すべて `Send + Sync` | 中核のすべての型を、スレッド間で安全に送受信・共有できる型にする。コンパイル時の確認をテストに置く。これはデータ競合を防ぐ土台であり、計算の正しさと再現性は別にテストで確かめる（6.3） |
| R12 | 中核に `unsafe` を書かない | 中核の crate は `#![forbid(unsafe_code)]` とする。言語の入口でも、各言語の結び付け用 crate（extendr・PyO3・napi-rs・wasm-bindgen）が用意する安全な API だけを使う |
| R13 | 中核は言語に依存しない | 中核は R・TypeScript などの API を知らない。各言語の入口は、データの変換とエラーの変換だけを行う薄い層にする |
| R14 | 速さを測る | 主要な計算に性能測定（ベンチマーク）を付け、照合と同じく各段階で確かめる |
| R15 | WebAssembly でも動く | 中核は `wasm32-unknown-unknown` 向けにもビルドできるようにする。ファイル操作・時刻の取得・OS の機能を中核で使わない。並列化は feature（`parallel`、既定で有効）で切り替え、WebAssembly では無効にできるようにする |
| R16 | 一致は許容誤差で定める | 照合と言語間の一致は、完全一致を一律に求めず、計算ごとに許容誤差を定める（7.3）。ネイティブと WebAssembly は別の基準で扱う |

## 3. 置き換えの対応表

| 既存の仕組み | Riemann での置き換え |
|---|---|
| `Instrument` の派生クラス（QuantLib）、商品インターフェースの実装（finmath） | `enum Instrument { FixedRateBond(..), OvernightIndexedSwap(..), .. }` |
| 商品に付け替える `PricingEngine` | 評価関数の引数として渡す値 `enum Method { Discounting, .. }` |
| `PiecewiseYieldCurve<Traits, Interpolator, Bootstrap>` | 値 `CurveSpec { quantity: Discount, interpolation: LogLinear, solver: Sequential, .. }` を受け取る関数 `bootstrap` |
| `Handle`／`RelinkableHandle` の付け替え | 新しい `MarketSnapshot` を作って、評価関数にもう一度渡す |
| `Observer`／`LazyObject` による自動の再計算 | 呼び出し側が評価し直す。必要なら、入力の内容から作ったキーで結果をメモ化する |
| `Settings` の評価日、`IndexManager` の Fixing | `ValuationContext { valuation_date, fixings, settings }` |
| CRTP、ビジター、仮想関数による分岐 | `match` |
| `MakeOIS`・`MakeSchedule` などのビルダー、Factory クラス | 仕様の値（`ScheduleSpec` など）と、それを受け取る関数。省略した項目は `Default` で埋める |
| 抽象クラスの階層（`AbstractCurve`、`AbstractForwardCurve` など） | 一つの `struct Curve` と、それを読む関数（`discount`、`zero_rate`、`forward_rate`） |
| 手法ごとの商品クラス（finmath の `EuropeanOption` ×5） | 商品は `Instrument::EuropeanOption` 一つ。手法は `Method` で選ぶ |

## 4. データと関数の形（案）

以下は形を示すための案で、実装ではない。

```rust
// 単位と意味を区別する型（R10）
pub struct Date(i32);
pub struct Rate(f64);          // 小数（0.0425 = 4.25%）
pub struct Bp(f64);
pub struct Price(f64);         // 額面100当たり
pub struct Money { pub amount: f64, pub currency: Currency }

// 選択肢は値（R3）
pub enum DayCount { Act360, Act365Fixed, Thirty360BondBasis, ActActIsda }
pub enum Calendar { Japan, Target, UnitedStatesSofr, UnitedKingdomSettlement, Joint(Vec<Calendar>) }
pub enum Interpolation { LogLinearDiscount, LogCubicDiscount, FlatForward }

// 評価コンテキスト（R8）
pub struct ValuationContext { pub valuation_date: Date, pub fixings: Arc<Fixings>, pub settings: Settings }

// カーブは値、構築は関数（R1・R2）
pub struct CurveSpec { pub currency: Currency, pub index: OvernightIndex, pub interpolation: Interpolation, pub reference: Reference }
pub struct Curve { pub spec: CurveSpec, pub reference_date: Date, pub nodes: Vec<(Date, f64)> }
pub fn bootstrap(spec: &CurveSpec, quotes: &[Quote], ctx: &ValuationContext) -> Result<(Curve, Diagnostics), Error>;
pub fn discount(curve: &Curve, date: Date) -> Result<f64, Error>;

// 市場データの更新は新しい値（R7）
pub fn with_curve(market: &MarketSnapshot, id: CurveId, curve: Curve) -> MarketSnapshot;

// 商品と評価（R1・R2・R4）
pub enum Instrument { FixedRateBond(FixedRateBond), OvernightIndexedSwap(Ois), OisAssetSwap(OisAssetSwap) }
pub enum Method { Discounting }
pub fn price(ctx: &ValuationContext, market: &MarketSnapshot, instrument: &Instrument, method: &Method) -> Result<Valuation, Error>;
```

## 5. この設計で得られること

- **読みやすさ**：ある計算は、一つの関数と一つの `match` を読めば分かる。クラス階層をたどる必要がない。
- **R との相性**：選択肢が値なので、R から文字列やリストで渡せる（`interpolation = "log_cubic_discount"` など）。型の組み合わせを R 側で用意する必要がない。
- **再現性**：同じ入力からは常に同じ結果になるように設計する（【設計案】。6.3 のテストで確かめる）。結果（`Valuation`）に、評価日、市場データの識別子、手法、乱数の種、ライブラリーのバージョンを記録する。
- **並列評価**：書き換えない値を `Arc` で共有するので、異なる評価日・シナリオ・商品の計算を安全に同時に走らせられる。並列化は rayon で、独立な評価の単位に限る。集計の順序は固定する。
- **シナリオとグリークス**：「市場データを変える関数」と「評価する関数」の組み合わせで、シナリオ計算と数値差分のグリークスがそのまま書ける。

## 6. 高速性とスレッド安全性

### 6.1 速さのための方針

| 方針 | 内容 |
|---|---|
| 分岐は静的に | 種類の違いは `match` で分岐する。仮想関数の呼び出しやヒープ上のオブジェクトを経由しないので、コンパイラーが最適化しやすい |
| データは連続した配列に | 節点、キャッシュフロー、MC のパスは `Vec` の連続した領域に置く。MC は「パスの配列」ではなく「時点ごとの配列」（列指向）で持ち、ベクトル演算にしやすくする |
| 計算の中で確保しない | 繰り返しの内側でメモリーを確保しない。必要な作業領域は、関数の外から渡すか、最初に一度だけ確保する |
| 余計な複製をしない | 大きな値は `Arc` で共有し、参照で渡す。新しい値を作るときも、変わらない部分は共有したままにする |
| 測ってから最適化する | criterion でベンチマークを取り、GaussQuant（QuantLib）の同じ計算とも時間を比べる。SIMD などの手作業の最適化は、測定で必要と分かってから行う |

### 6.2 スレッド安全な並列評価

- **言語が保証する安全性**：中核の型はすべて `Send + Sync`（R11）で、共有されるデータは書き換えない値（R1・R7）である。データ競合はコンパイルの時点で防がれる。ただし、これで保証されるのはメモリーの安全性とデータ競合がないことだけである。計算の誤り、並列にしたことによる集計順の違い、乱数の割り当ての誤りは防げないので、6.3 のテストで確かめる。
- **並列にする単位**：「商品 × シナリオ × 評価日」のように、互いに独立な評価を rayon で並列にする。カーブの構築や MC のパスなど、内部の並列化は測定で効果を確かめてから行う。
- **結果はスレッド数に依存しない**：
  - 乱数はパスごと・評価単位ごとに種を分ける（カウンター型の乱数、または種の導出規則を固定する）。
  - 並列の集計は、決まった順序の木構造で足し合わせる。
  - こうした決まりで、1 スレッドでも 32 スレッドでも同じ数値になることを目標とする。実際にそうなっているかは 6.3 のテストで確かめる。結果には、使った乱数の種と並列の設定を記録する。
- **言語の側の制約と分ける**：R の API は一つのスレッドからしか呼べない。Python には GIL がある。Node.js の JavaScript は一つのスレッドで動く。そこで、並列化は中核の中だけで行い、言語の入口は計算の前後でデータを変換するだけにする。Python では計算中に GIL を解放し、Node.js では重い計算を非同期のタスクとして実行して、JavaScript のスレッドを止めない。ブラウザーの WebAssembly では、標準では1スレッドで計算する（R15）。

### 6.3 正しさと再現性のテスト

`Send + Sync` と不変データは土台にすぎない。並列化した計算については、次のテストを置く。

| テスト | 内容 |
|---|---|
| 逐次と並列の一致 | 同じ入力を、並列化なし（1スレッド）と並列化あり（2・4・8 スレッドなど）で計算し、結果を比べる。集計順を固定した計算は完全一致、そうでない計算は 7.3 の許容誤差で比べる |
| 繰り返しの一致 | 同じ入力・同じスレッド数で何度も実行し、毎回同じ結果になることを確かめる |
| 分割の一致 | 評価の単位（商品・シナリオ・パス）の分け方や並び順を変えても、各単位の結果が変わらないことを確かめる |
| 乱数の割り当て | 乱数の種の導出が、スレッド数や実行順に依存しないことを確かめる。同じ種から同じ乱数列が出ることも確かめる |
| 性質のテスト | 割引係数が満期に対して正であること、入力気配の再現、価格とスプレッドの逆算の一致など、計算の性質をランダムな入力で確かめる（proptest など） |
| 基準値との照合 | 並列化した計算も、GaussQuant・LagrangeFinance の基準値と 7.3 の許容誤差で照合する |

## 7. 多言語からの利用

### 7.1 構成

```
            ┌──────────────────────────────────────────┐
            │ riemann（中核）                           │
            │ 値と関数、Send + Sync、unsafe なし          │
            └──────────────┬───────────────────────────┘
       ┌─────────────┬─────┴───────┬──────────────────┐
  riemann-r      riemann-node         riemann-wasm          riemann-py
  R（extendr）   TypeScript／Node.js   TypeScript／ブラウザー  Python（PyO3）
  【主】          （napi-rs）【主】     （wasm-bindgen）【主】  （必要になったら）
```

### 7.2 言語の境界での決まり

| 項目 | 決まり |
|---|---|
| 受け渡しの形 | 表（気配、キャッシュフロー、結果）と、仕様の記録（規約、商品）だけを受け渡す。言語の側にオブジェクトの状態を持たせない |
| 選択肢の名前 | `enum` は、全言語で共通の文字列名で受け渡す（例：`"act_365_fixed"`、`"log_cubic_discount"`）。名前の一覧は中核で一か所に定義する |
| 仕様の記録 | 商品や規約は serde で直列化できるようにし、JSON などで言語間・ファイル間をやり取りできるようにする |
| 大きな表 | 当面は各言語の標準の表（R の data.frame、Python の dict と配列、TypeScript のオブジェクトの配列と `Float64Array`）で受け渡す。大量のデータで必要になったら Apache Arrow を検討する |
| エラー | 中核のエラー（原因の入力を含む）を、各言語の普通のエラー（R の condition、Python の例外、TypeScript の `Error`）に変換する |
| 結果の一致 | 同じ入力なら、どの言語から呼んでも、7.3 で計算ごとに定めた許容誤差の範囲で同じ数値になることを目標とし、テストで確かめる。各言語の入口に、同じ基準値を使ったテストを置く |
| 型定義 | TypeScript の型定義（`.d.ts`）は napi-rs と wasm-bindgen で自動生成し、手で書かない。選択肢の名前は文字列リテラルの型（`"act_360"` と `"act_365_fixed"` の合併型など）として出す |
| `unsafe` | 自分では書かない。各言語の結び付け用 crate の安全な API だけを使う（R12） |

### 7.3 数値一致の許容誤差

完全一致を一律に求めない。計算の種類ごとに許容誤差を定め、テストの基準値ファイルに、比べる値と許容誤差を一緒に記録する。下の値は初期案で、実際の値は各段階で、基準値との差の実測を見て決める。

| 計算の種類 | 例 | 基準値（GaussQuant など）との照合 | 言語間・ネイティブ同士 | ネイティブと WebAssembly |
|---|---|---|---|---|
| 整数・日付・選択肢 | 日付列、営業日調整、スケジュール、カレンダーの判定 | 完全一致 | 完全一致 | 完全一致 |
| 四則演算だけの計算 | 年数（日数計算）、固定のキャッシュフロー額 | 相対 1e-15 程度 | 完全一致（同じ計算順） | 完全一致を目標とし、差が出たら相対 1e-15 程度 |
| 指数・対数などを含む計算 | 割引係数、ゼロ金利、債券価格 | 相対 1e-12 程度 | 完全一致（同じプラットフォーム・同じビルド） | 相対 1e-13 程度（数学関数の実装の違いで最下位の桁が変わりうる） |
| 反復解法 | ブートストラップ、利回りの逆算、キャリブレーション | 解法の収束条件から決める（例：気配の再現誤差は金利で 1e-9 以内。TONAR の実測は最大 5.3e-11。`spec_ois.md` 8.3 節） | 同上 | 収束条件の範囲内。反復回数が変わることも許す |
| 確率的な計算 | MC の価格と感応度 | 標準誤差の数倍の範囲 | 同じ種・同じ設定なら完全一致 | 同じ種なら、上の「指数・対数」と同じ基準 |

- **ネイティブと WebAssembly を区別する理由**：`f64` の四則演算は IEEE 754 で同じ結果になるが、`exp`・`ln`・`powf` などの数学関数は、ネイティブではプラットフォームの数学ライブラリー、WebAssembly ではブラウザーや Rust の実装を使うため、最下位の桁が変わりうる。並列化の有無（WebAssembly は標準で1スレッド）による集計順の違いも、6.3 の決まりで抑えたうえで、この表の基準で比べる。
- **基準値（GaussQuant／QuantLib）との違い**：QuantLib とは補間や反復の実装が異なるので、基準値との照合は、言語間の一致よりも緩い基準になることがある。

## 8. 日付・カレンダーと自動微分の詳細仕様

### 8.1 日付・カレンダー・日付規約

詳細は [spec_dates_calendars.md](spec_dates_calendars.md)。**QuantLib の仕様に依拠し**、同じ入力に対して同じ日付・同じ年数割合になることを照合する。

- 順番：日付 → 休日カレンダー → 営業日調整（Business Day Convention）→ スケジュール生成 → 日数計算（Day Count Convention）。
- 日付：QuantLib と同じシリアル値と有効範囲（1901-01-01〜2199-12-31）。既存の crate は使わず自前で作る。
- 休日：QuantLib の規則による判定に、臨時休日などの例外日のデータを組み合わせる。日本の休日は QuantLib 互換の実装であり、祝日法そのものの実装ではない（詳細仕様 3.3）。判定の順序と追加・取り消しの決まりは QuantLib と同じにし、例外日は全体で共有せず、値として明示的に渡す。
- 参照した QuantLib：仕様は `70f240f`（1.45-dev）、照合の基準値は R バインディング 1.42（`v1.42` = `41b0e14`）。両者の `ql/time/` の差分を読んだ範囲では、判定に関わる変更は見当たらなかった（【ソース確認】。二つの版を実行して比べてはいない）。照合の正は 1.42 の実行結果とする。

### 8.2 自動微分

詳細は [spec_autodiff.md](spec_autodiff.md)。**finmath-lib の方式を参考にする**。

- 確率変数（`Rv`）は、パス方向のベクトルと時刻を持つ値。演算は新しい値を返す。
- 計算グラフは、計算ごとの配列（アリーナ）に記録した順に積み、逆伝播は配列を後ろから1回たどる。finmath-lib の全体共有の ID カウンターは使わない。
- 期待値・条件付き期待値・指示関数は、finmath-lib と同じ考え方で逆伝播する。条件付き期待値の回帰は、書き換えない値として演算に持たせる。
- 数値計算のコードは、内部の小さな trait（`Operand`）について総称的に書き、`f64`・`Rv`・記録用の値で共有する（R6 が許す内部の型の抽象化）。P2 から入れる。

## 9. 独自実装と FFI

| 方式 | 採否 | 理由 |
|---|---|---|
| Rust による独自実装 | 採用 | 本書の原則で作るため。既存のライブラリーは、数式・仕様・規約の参考として読む |
| QuantLib を FFI で呼ぶ | 採用しない | 継承・テンプレート・グローバル状態をそのまま持ち込むことになり、本書の目的と反する。スレッド安全性も Rust の型で保証できなくなる |
| 照合の基準値 | 採用 | GaussQuant（QuantLib の R バインディング）と LagrangeFinance で基準値を作り、CSV に保存する。FFI なしで照合できる |

## 10. 段階的な開発案

各段階で、GaussQuant・LagrangeFinance の既存の計算を基準値として照合し、あわせて性能を測る。基準値は R のスクリプト（`tools/golden/`）で CSV に書き出し、Rust のテストで読む。現在動いている GaussQuant・LagrangeFinance・Curie は変更しない。

| 段階 | 作るもの | 照合する既存の計算 |
|---|---|---|
| P1 | 日付、カレンダー（東京・TARGET・米国 SOFR・英国）、日数計算、営業日調整、スケジュール生成 | QuantLib の R バインディングで、同じ規約の日付列と年数を出す。日付は完全一致、年数は 7.3 の基準で照合する。**2026-10-31 に完了**（【実行照合済み】。結果は spec_dates_calendars.md） |
| P2 | 固定利付債のキャッシュフロー、割引、価格と利回り（複利、日本式の単利） | GaussQuant の `fixed_rate_bond_GQL`、`bond_*_YEN_GQL`、LagrangeFinance II-1。**完了**（【実行照合済み】）：P2-1（キャッシュフローと経過利子）、P2-2（利回りと価格、デュレーション、コンベクシティ、BPV）、P2-3（割引カーブによる評価）は 2026-10-31 に QuantLib と、P2-4（日本式の単利・複利利回り）は同日に GaussQuant と照合済み（[spec_bonds.md](spec_bonds.md)） |
| P3 | OIS のブートストラップ（対数線形・対数3次）、O/N 預金、日次複利のクーポン、4通貨の規約 | QuantLib R バインディング 1.42（合成の気配。仕様は [spec_ois.md](spec_ois.md)）、GaussQuant の `ir_build_ois_curve_GQH` と回帰テストの基準値、LagrangeFinance II-3.2・II-3.4、EONIA ベンチマーク |
| P4 | R からの入口（`riemann-r`、extendr）。多数の評価を並列で行う関数も一つ入れ、6.3 の逐次と並列の一致テストを置く | P1〜P3 を R から呼び、testthat で GaussQuant と直接比べる |
| P5 | OIS の ASW と円ベーシス込み ASW | GaussQuant の `ois_asset_swap_GQL` と回帰テストの基準値、LagrangeFinance II-3.7 |
| P6 | TypeScript の Node.js 版の入口（`riemann-node`、napi-rs） | R と同じ基準値で、7.3 の「言語間・ネイティブ同士」の基準を満たすことを確かめる |
| P7 | TypeScript のブラウザー版の入口（`riemann-wasm`、wasm-bindgen） | 同じ基準値で、7.3 の「ネイティブと WebAssembly」の基準を満たすことを確かめる |

開発の順序は、R → TypeScript の Node.js 版 → ブラウザー版とする。まず既存の GaussQuant で数値を照合し、そのあとで利用先を広げる。

P8 以降（Black・Bachelier、MC、Hull-White、CDS など）は、P7 までの結果を見て決める。Python の入口（`riemann-py`：PyO3）は、必要になった時点で同じ決まり（7.2）で足す。

## 11. 各言語の入口（R と TypeScript を主とする）

- 最初の入口は R とする（`riemann-r`、extendr）。GaussQuant・LagrangeFinance と同じ環境で照合できるためである。
- 入口は「データを渡してデータを返す」形にする。気配や規約は data.frame とリストで受け取り、結果は data.frame で返す。選択肢の `enum` は、全言語共通の文字列名で受け取る。
- 並列の計算は中核の中で行い、R の API を呼ぶのは計算の前後だけにする。
- カーブなど大きな結果は、必要になったら R の外部ポインターで持つ。中身は書き換えない値なので、何度使っても安全である。
- TypeScript の入口も、同じ決まり（7.2）で作る。Node.js 版（napi-rs）はネイティブの速度と並列計算をそのまま使い、ブラウザー版（WebAssembly）は同じ計算をブラウザーの中で動かす。
- Python の入口は主な対象ではなく、必要になったら同じ決まりで足す。

### 11.1 命名の方針

| 対象 | 形 | 例 |
|---|---|---|
| 関数・メソッド・変数・モジュール | snake_case | `remaining_years`、`yen_bond` |
| 型・構造体・enum・trait | PascalCase | `YieldCurve`、`DayCount` |
| 定数 | SCREAMING_SNAKE_CASE | `MAX_SERIAL` |

- R から自然に使えるよう、Rust と R で公開関数名・引数名をできるだけ揃える。
- 言語の境界で変換が必要な場合も、不要な改名は避ける。

2026 年 10 月末時点の Rust のコードは、`cargo clippy --all-targets -- -D warnings -D nonstandard_style` が通り、この方針に沿っている。

## 12. 最小構成

```
Riemann/
├── Cargo.toml              # ワークスペース
├── crates/
│   ├── riemann/            # 中核（言語に依存しない、unsafe なし、Send + Sync）
│   ├── riemann-r/          # R の入口（P4）
│   ├── riemann-node/       # TypeScript／Node.js の入口（P6、napi-rs）
│   ├── riemann-wasm/       # TypeScript／ブラウザーの入口（P7、wasm-bindgen）
│   └── riemann-py/         # Python の入口（必要になったら）
├── benches/                # 性能測定（criterion）
├── tools/golden/           # 基準値を作る R スクリプト
├── tests/golden/           # 基準値の CSV（市場データの気配は含めない）
└── docs/
```

最初の1歩は P1 の中核だけにする。外部の市場データが要らず、日付は QuantLib と完全一致で照合でき、後続のすべての土台になる。

## 13. ライセンス

- Riemann は MIT と Apache-2.0 の併用とする（`LICENSE-MIT`、`LICENSE-APACHE`）。
- QuantLib（修正 BSD）と finmath-lib（Apache-2.0）のコードは移植せず、数式・仕様・規約の参考にする。
- ただし、休日の規則は QuantLib のコードを読んで同じ判定を作るので、QuantLib から派生したものとして扱い、該当ファイルに元のファイル・コミット・著作権表示と条件を書く。リポジトリには `THIRD_PARTY_NOTICES` を置く（[spec_dates_calendars.md](spec_dates_calendars.md) の 8 章）。
- やむを得ず移植するときは、ファイルの先頭に元のファイル・コミット・ライセンスを書く。QuantLib は著作権表示と条件を残し、finmath-lib は `NOTICE.txt` の帰属表示を引き継いで変更した旨を書く。
- この整理は法的な助言ではない。

## 14. 決めていただきたいこと

現在、未決定の事項はない。

決定済み（2026-10-30）：
- 開発の順序は R → TypeScript の Node.js 版 → ブラウザー版。
- 日付・カレンダー・日付規約は QuantLib の仕様に依拠する。日付は自前で作る（8.1）。
- 休日は QuantLib の規則に、例外日のデータを組み合わせる（8.1）。
- 自動微分は finmath-lib の方式を参考にし、計算ごとのアリーナに記録する計算グラフで作る（8.2）。

## 参考資料

- [日付・カレンダー・日付規約の仕様](spec_dates_calendars.md)
- [債券の仕様](spec_bonds.md)
- [自動微分の仕様](spec_autodiff.md)
- [QuantLib と finmath-lib の比較](library_comparison.md)：対応商品・モデル・カーブ・グリークス・設計の、本体ソースによる比較（QuantLib `70f240f`、finmath-lib `9bd054f`）。商品の範囲や数式・規約を調べるときに使う。
