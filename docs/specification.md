# Riemann 設計仕様書（草案 v0.1）

Riemann は、QuantLib と finmath-lib を参考に、Rust で一から作る金融計算ライブラリーである。
R からも呼べるようにする。本書は実装前の設計仕様の草案であり、コードはまだない。

- 比較の根拠：[library_comparison.md](library_comparison.md)（QuantLib `70f240f`、finmath-lib `9bd054f`）
- 現在動いている GaussQuant・LagrangeFinance・Curie は変更しない。照合の基準（オラクル）としてだけ使う。

## 1. 基本方針

1. **関数型で作る**：C++／Java の継承とオブザーバーによるオブジェクト指向はやめる。
   - データ（日付、規約、市場データ、商品）は、作ったあと書き換えない値とする。
   - 種類の違いは継承ではなく、`enum`（代数的データ型）と `match` で表す。
   - 計算は、入力をすべて引数で受け取って結果を返す、副作用のない関数とする。
2. **グローバル状態を持たない**：評価日、Fixing、設定は、すべて評価コンテキストとして明示的に渡す。
3. **独自実装**：QuantLib を FFI で呼ばない。QuantLib は、GaussQuant（R バインディング）経由で照合の基準値を作るためにだけ使う。
4. **小さく作って照合する**：GaussQuant・LagrangeFinance で答えが分かっている計算から順に作り、数値の一致を確かめながら広げる。全商品を一度に移植しない。
5. **R から使える**：計算の中核は R に依存させず、R 用の薄い入口を別の crate にする。

## 2. 設計項目ごとの採用案

### 2.1 引き継ぎたい設計

| 機能・設計項目 | QuantLib | finmath-lib | Rust での採用案と理由 |
|---|---|---|---|
| 日付 | 独自の `Date`（シリアル値） | `java.time.LocalDate` と、年単位の浮動小数の時刻 | 独自の `Date`（日のシリアル値を持つ newtype）。日付の計算は整数で行い、年数（`YearFraction`）は日数計算の結果としてだけ作る。浮動小数の時刻を日付の代わりに使わない |
| カレンダー | `Calendar` の派生クラス56種。休日を規則のコードで持つ | TARGET・ロンドン・NY・週末・指定休日のみ。東京はない | `enum Calendar { Japan, Target, UnitedStatesSofr, UnitedKingdomSettlement, Joint(..), Custom(..) }`。休日は規則の関数と、追加・削除の日付表の組み合わせで表す。東京を最初から持つ（finmath にはない） |
| 日数計算 | `DayCounter` の派生クラス13種 | `DayCountConvention_*` を多数 | `enum DayCount { Act360, Act365Fixed, Thirty360BondBasis, ActActIsda, .. }` と純粋関数 `year_fraction(dc, start, end)` |
| スケジュール | `Schedule`、`MakeSchedule`（前進・後退、月末、各種規約） | `ScheduleGenerator` と、仕様を持つ `SchedulePrototype` | スケジュールの仕様 `ScheduleSpec`（値）から日付列を作る純粋関数 `generate(&ScheduleSpec) -> Result<Schedule>`。finmath と同じく、仕様と生成結果を分ける |
| 市場データ・カーブ・モデル・商品・エンジンの分離 | `Instrument` と `PricingEngine` を分け、エンジンを付け替える | 商品が `getValue(evaluationTime, model)` で自分を評価する。商品の記述から実装を作る `ProductFactory` もある | 商品は記述だけの値（`enum Instrument`）、市場データは別の値（`MarketSnapshot`）、評価手法も値（`enum Method`）とし、評価は関数 `price(ctx, market, instrument, method)` にする。QuantLib の「手法を差し替えられる」と finmath の「記述と実装を分ける」を、どちらも状態なしで実現できる |
| キャッシュフローとレグ | `Leg = vector<shared_ptr<CashFlow>>`。クーポンとプライサーを分ける | `SwapLeg` がスケジュールと金額の配列を持つ | `enum CashFlow { Fixed{..}, OvernightCompounded{..}, Notional{..} }` の `Vec` をレグとする。レグを作る関数と、評価する関数を分ける。OIS の日次複利（lookback・lockout・observation shift）は QuantLib の `OvernightIndexedCoupon` を仕様の参考にする |
| カーブ | `PiecewiseYieldCurve` の逐次ブートストラップ。`Handle` で付け替え | `CalibratedCurves` で複数カーブを同時に解く。カーブを名前で参照 | `enum Curve` は節点と補間方式を持つ値とする。構築は `bootstrap(&CurveSpec, &Quotes, &ctx) -> Result<(Curve, Diagnostics)>`。最初は逐次ブートストラップで作り、通貨ベーシスなど相互依存がある段階で同時解法を加える。カーブは名前（`CurveId`）で参照する（finmath 方式） |
| キャリブレーション | `CalibrationHelper` と `CalibratedModel`、最適化手法 | 最適化手法の工場、AD を使う LM | 「パラメーター → 誤差ベクトル」の純粋関数と、汎用のソルバーに分ける。結果には残差と反復回数を必ず付ける |
| 乱数 | MT19937、Sobol、Halton、xoshiro256** など | MT、Sobol、Halton など | 乱数の生成器は状態を値として渡し、種（seed）を必ず明示させる。並列でも同じ結果になるよう、カウンター型（Philox など）か、パスごとに種を分ける方式にする |
| グリークス | エンジンの結果としての解析グリークス。数値差分は `SimpleQuote` を動かす | 解析式、変更した複製による差分、pathwise・尤度比、AAD | 数値差分は「シナリオ関数で新しい市場スナップショットを作り、評価し直す」形にする。解析グリークスは手法ごとに返す。MC 感応度は後の段階で入れる |
| 自動微分 | 中核にはない。実数型を `QL_REAL` で差し替えられる | 前進型・後退型の AD を MC の確率変数に組み込む | 数値計算の関数を、実数の型について総称的（ジェネリック）に書く（`T: Real`）。最初は `f64` だけで使い、次に前進型の双対数で差し替えられるようにする。後退型（AAD）は MC の段階で検討する |

### 2.2 Rust で見直したい設計

| 機能・設計項目 | QuantLib | finmath-lib | Rust での採用案と理由 |
|---|---|---|---|
| 継承中心の構造 | `Instrument`・`PricingEngine`・`TermStructure` の派生クラス | インターフェースの実装と、手法ごとに別の商品クラス | 閉じた種類は `enum` と `match`（新しい種類を足したとき、対応漏れをコンパイラーが見つける）。外から拡張させたい部分だけ trait にする（補間方式、ソルバーなど）。継承はしない |
| 評価日などのグローバル状態 | `Settings` シングルトンの `evaluationDate`、`IndexManager` の Fixing | ない（評価時点は引数） | `ValuationContext { valuation_date, fixings, settings }` を、すべての評価関数に引数で渡す。同じ入力からは常に同じ結果になり、異なる評価日の計算を同時に走らせられる |
| 市場データの更新と再評価 | オブザーバーが変更を伝え、`LazyObject` が再計算する | 変更した複製を作る（`addCurve`、`getCloneWithModifiedData`） | 更新は「古いスナップショット → 新しいスナップショット」の関数にする（finmath 方式）。変更のないカーブは `Arc` で共有する。キャッシュが必要になったら、入力の内容から作るキーで外付けのメモ化を行い、オブザーバーは使わない |
| 不変データの共有と並列評価 | `QL_ENABLE_SESSIONS` でシングルトンをスレッドごとに持つ | Java の並列ストリーム | `MarketSnapshot` を `Arc` で共有し、`Send + Sync` にする。並列化は rayon で「商品 × シナリオ」の独立な評価に限る。並列の集計は順序を固定して、結果が実行ごとに揺れないようにする |
| 型と単位 | `Real`・`Rate`・`Time` は `double` の別名で、区別がない | `double` | newtype で区別する：`Rate`（小数）、`Bp`、`Price`（額面100当たり）、`Money { amount, currency }`、`DiscountFactor`、`YearFraction`、`Date`。`Rate` と `Bp` の変換は明示的な関数だけで行う。異なる通貨の金額は、そのままでは足し算できないようにする |
| エラー処理 | 例外（`QL_REQUIRE`、`QL_FAIL`） | 例外 | ライブラリーでは panic しない。`Result<T, RiemannError>` を返し、エラーには「どの入力（気配の ID、日付など）が原因か」を持たせる |
| 計算結果の再現性 | 評価日などの状態と、乱数の種に依存する | 乱数の種に依存する | 結果（`Valuation`）に、使った評価日、市場データの識別子、手法、乱数の種、ライブラリーのバージョンを記録する。浮動小数の集計順を固定する |

## 3. 主な型と関数の形（案）

以下は形を示すための案で、実装ではない。

```rust
// 日付と規約（すべて値）
pub struct Date(i32);
pub enum Calendar { Japan, Target, UnitedStatesSofr, UnitedKingdomSettlement, Joint(Vec<Calendar>) }
pub enum DayCount { Act360, Act365Fixed, Thirty360BondBasis, ActActIsda }
pub fn year_fraction(dc: DayCount, start: Date, end: Date) -> YearFraction;
pub fn adjust(cal: &Calendar, date: Date, bdc: BusinessDayConvention) -> Date;

// 評価コンテキスト（グローバル状態の代わり）
pub struct ValuationContext { pub valuation_date: Date, pub fixings: Arc<Fixings>, pub settings: Settings }

// 市場データのスナップショット
pub struct MarketSnapshot { pub id: SnapshotId, curves: Arc<BTreeMap<CurveId, Arc<Curve>>>, quotes: Arc<Quotes> }
pub fn with_curve(m: &MarketSnapshot, id: CurveId, c: Curve) -> MarketSnapshot; // 新しい値を返す

// カーブの構築
pub fn bootstrap(spec: &CurveSpec, quotes: &Quotes, ctx: &ValuationContext) -> Result<(Curve, Diagnostics), RiemannError>;

// 商品と評価
pub enum Instrument { FixedRateBond(FixedRateBond), OvernightIndexedSwap(Ois), OisAssetSwap(OisAssetSwap), /* .. */ }
pub enum Method { Discounting, /* 後の段階で Analytic(..), MonteCarlo(..) */ }
pub fn price(ctx: &ValuationContext, m: &MarketSnapshot, i: &Instrument, method: &Method) -> Result<Valuation, RiemannError>;
```

## 4. 独自実装と FFI の区別

| 方式 | 内容 | 採否 |
|---|---|---|
| Rust による独自実装 | 仕様と数式から Rust で書く。QuantLib・finmath-lib のソースは設計と仕様の参考として読む | **採用**（Riemann の本体） |
| 既存 C++ を FFI で呼ぶ | `cxx` などで QuantLib を Rust から呼ぶ | **採用しない**。QuantLib のグローバル状態とオブザーバーがそのまま入り、関数型の設計と合わない。C++ の中継コードとビルドの負担も大きい |
| 照合用の基準値 | GaussQuant（QuantLib の R バインディング）と LagrangeFinance で基準値を作り、CSV に保存する | **採用**。FFI なしで、QuantLib の結果と照合できる |

## 5. ライセンスと帰属表示

| 元のコード | ライセンス | 移植するときの義務 |
|---|---|---|
| QuantLib | 修正 BSD（`LICENSE.TXT`） | 著作権表示・条件・免責をソースと配布物に残す。QuantLib Group や貢献者の名前を、宣伝や推奨に使わない |
| finmath-lib | Apache License 2.0（`LICENSE.txt`、`NOTICE.txt`） | ライセンス文を同梱し、`NOTICE.txt` の帰属表示を引き継ぎ、変更したファイルに変更した旨を書く |

- **方針**：コードは移植せず、数式・仕様・設計の考え方を参考にして独自に書く。それでも README に、参考にしたライブラリーとして両方の名前を挙げる。
- やむを得ず移植するときは、そのファイルの先頭に元のファイル・コミット・ライセンスを書き、上の義務を守る。
- Riemann 自体のライセンスは、MIT と Apache-2.0 の併用とする（2026-10-30 決定。`LICENSE-MIT`、`LICENSE-APACHE`）。
- 上の整理は法的な助言ではない。

## 6. 段階的な開発案

各段階で、GaussQuant・LagrangeFinance の既存の計算を基準値として照合する。基準値は R のスクリプト（`tools/golden/`）で CSV に書き出し、Rust のテストで読む。

| 段階 | 作るもの | 照合する既存の計算 |
|---|---|---|
| P0 | 仕様書、比較の文書、ワークスペースの骨組み | — |
| P1 | 日付、カレンダー（東京・TARGET・米国 SOFR・英国）、日数計算、営業日調整、スケジュール生成 | QuantLib の R バインディング（GaussQuant 経由）で、同じ規約の日付列と年数を CSV に出す。完全一致を求める |
| P2 | 固定利付債のキャッシュフロー、割引、価格と利回り（複利、日本式の単利） | GaussQuant の `fixed_rate_bond_GQL`、`bond_*_YEN_GQL`、LagrangeFinance II-1 の計算 |
| P3 | OIS のブートストラップ（対数線形・対数3次）、O/N 預金、日次複利のクーポン、4通貨の規約 | GaussQuant の `ir_build_ois_curve_GQH` と回帰テストの基準値（合成気配）、LagrangeFinance II-3.2（TONA）・II-3.4（SOFR）、EONIA ベンチマーク |
| P4 | R からの入口（`riemann-r`）。P1〜P3 の関数を R から呼び、testthat で GaussQuant と直接比べる | 上と同じ |
| P5 | OIS の ASW と円ベーシス込み ASW | GaussQuant の `ois_asset_swap_GQL` と回帰テストの基準値、LagrangeFinance II-3.7 |

P6 以降（Black・Bachelier と自動微分のグリークス、MC、Hull-White、CDS など）は、P5 までの結果を見てから決める。

## 7. R からの入口

- `riemann-r` crate を extendr で作り、R パッケージから呼ぶ。
- 入口は「データを渡してデータを返す」形にする。気配や規約は data.frame とリストで受け取り、結果は data.frame で返す。R の側に状態を持たせない。
- カーブなど大きな結果は、必要になったら R の外部ポインターで持つ。中身は書き換えない値なので、R から複数回使っても安全である。
- Rust の中核（`riemann`）は R に依存させない。

## 8. 最小構成

```
Riemann/
├── Cargo.toml              # ワークスペース
├── crates/
│   ├── riemann/            # 中核ライブラリー（R に依存しない）
│   │   └── src/            # time/、market/、curves/、instruments/、pricing/ を段階ごとに足す
│   └── riemann-r/          # R からの入口（P4 で追加）
├── tools/golden/           # 基準値を作る R スクリプト（GaussQuant・LagrangeFinance を使う）
├── tests/golden/           # 基準値の CSV（市場データの気配は含めない）
└── docs/                   # 本書、比較の文書
```

最初の1歩は P1 の「日付・カレンダー・日数計算・スケジュール」だけにする。ここは外部の市場データが要らず、QuantLib と完全一致で照合でき、すべての後続の土台になる。

## 9. 決めていただきたいこと

決定済み（2026-10-30）：ライセンスは MIT と Apache-2.0 の併用、置き場所は GitHub。

1. 日付の型を自前にするか、既存の crate（`time` や `chrono`）を内部で使うか
2. 休日データの持ち方（QuantLib と同じく規則をコードで書くか、日付表のデータとして持つか）
3. 自動微分を入れる時期（P2 から関数を総称的に書いておくか、P6 で入れるか）
