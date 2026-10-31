# R の入口の仕様（P4）

R から Riemann を直接呼ぶ入口である。パッケージ名は `riemann`、場所は `crates/riemann-r`。金融の計算はすべて Rust の `riemann` crate にあり、R は日付の変換と結果の表の組み立てだけを行う。

記述の区分（【実行確認】【実行照合済み】【設計案】）は仕様書と同じ考え方である。

## 1. 構成

| 場所 | 内容 |
|---|---|
| `crates/riemann/src/names.rs` | 選択肢の文字列名（全言語で共通。snake_case） |
| `crates/riemann/src/batch.rs` | 並列バッチ（R に依存しない） |
| `crates/riemann-r/src/rust/src/lib.rs` | extendr の境界。変換だけを行う |
| `crates/riemann-r/R/` | 薄い R の関数（`common.R`、`bonds.R`、`ois.R`、`calendars.R`、`batch.R`） |
| `crates/riemann-r/inst/examples/quickstart.R` | Positron の R コンソールから実行するサンプル |
| `crates/riemann-r/tests/testthat/` | R の入口のテスト |

既存の R 連携コードはなかったので新しく作った。`rextendr` は入っていないので、R の関数は手で書き、`.Call(wrap__関数名, ...)` で呼ぶ。Rust の crate は、ルートのワークスペースから外している（`R CMD INSTALL` が `windows-gnu` 向けに単独でビルドするため）。

## 2. 導入

Windows では次が必要である：Rtools 4.5、`rustup target add x86_64-pc-windows-gnu`。

```
R CMD INSTALL crates/riemann-r
```

- `Makevars` は毎回 `cargo` を呼び、再ビルドの判断を `cargo` に任せる。依存を書かないと、一度ビルドした後は Rust のソースを変えても古いライブラリーのままになる。実際にこの不具合が起きたので直した。
- ディレクトリーから導入する。Rust の crate は相対パスで中核に依存するので、tar.gz にして他の場所で導入することは、まだできない。

## 3. 公開関数

名前は Rust の関数と同じにする。引数も、できるだけ同じにする。規約に既定値は持たせない（QuantLib の既定は市場の規約ではないため）。既定値があるのは、解法の設定、仕様で採用した補間、`digits` などの計算の設定だけである。

| 分類 | R の関数 |
|---|---|
| 債券 | `fixed_rate_bond`、`settlement_date`、`bond_values`、`clean_price`、`dirty_price`、`accrued_amount`、`duration`、`convexity`、`basis_point_value`、`yield_value_basis_point`、`yield_from_price`、`value_on_curve` |
| 日本式の債券計算 | `remaining_years`、`price_from_simple_rate`、`simple_rate_from_price`、`price_from_compound_yield`、`compound_yield_from_price`、`risk_from_simple_rate`、`risk_from_compound_yield` |
| OIS カーブ | `ois_instruments`、`build_ois_curve`（`tonar`・`estr`・`sofr`・`sonia`）、`with_extrapolation`、`discount`、`zero_rate`、`forward_rate`、`quote_repricing`、`discount_nodes`、`curve_values` |
| カレンダー | `is_business_day`、`holidays` |
| 並列バッチ | `bond_values_batch`、`curve_values_batch` |

- **日付**：R の `Date` または ISO 文字列（`YYYY-MM-DD`）を受け取り、`Date` で返す。それ以外と欠損はエラーにする。
- **結果**：複数の項目を返す関数は tibble で返す。
- **真偽値**：公開 API では logical だけを受け取り、logical で返す。NA と logical 以外の入力はエラーにし、FALSE に読み替えない。結果の NA は「失敗」の意味である（`is_business_day(..., on_error = "na")`）。バッチの列を整数にするのは R と Rust の境界の中だけで、extendr に `bool` のベクトルの変換がないためである。Rust 側でも 0 と 1 以外は失敗として返す。
- **値**：債券とカーブは Rust の値を外部ポインターで持つ。中身は書き換えない。`with_extrapolation` は新しい値を返す。

## 4. エラーと panic

| 種類 | 扱い |
|---|---|
| 想定内のエラー（呼び出し全体）：不明な名前、作れない債券、収束しないカーブ | Rust が `failure` を持つリストを値として返し、`checked()` が R のエラーにする |
| 想定内のエラー（行ごと）：範囲外の日付など | 行に NA と `error` のメッセージを入れる。`on_error = "stop"`（既定）で止まり、`"na"` で NA を残す |
| 予期しない panic（Riemann の不具合） | process の panic hook を変えないので、ファイルと行が stderr に出る。extendr が R のエラーにし、R は動き続ける |

extendr の `Err` は内部で panic を経由し、R のコンソールに `panicked` と表示する。そこで想定内のエラーには `Err` を使わない。hook は変えない（他のパッケージにも影響し、不具合の診断もできなくなるため）。手書きの `unsafe` はない。

【実行確認】不明なカレンダーでは `panicked` の表示なしで R のエラーになる。確認用の `panic_for_test` では `src\lib.rs` の行とメッセージが表示され、R のエラーになり、その後も計算を続けられる（`tests/testthat/test-direct.R`）。

`bond_values` の `duration_macaulay` は、複利の利回りでないときは NA で、行のエラーにしない。Macaulay デュレーションは複利でしか定義されないからである。`duration(..., "macaulay")` を直接呼ぶと、エラーになる。

## 5. 並列バッチ

`riemann::batch` は rayon で計算する。入力は R のメインスレッドで Rust の値に変換し、スレッドはその値だけで計算し、結果は全スレッドの終了後に R のメインスレッドで変換する。中核の crate は extendr に依存しないので、計算中のスレッドから R の API は呼べない（コンパイルの時点で保証される）。結果の順序は、入力の順序のとおりである。`threads = 0` は論理プロセッサーごとに 1 スレッドである。`threads = 1` でも、呼び出したスレッドではなくワーカースレッドで計算する。1 つの単位の失敗は、他の単位に影響しない。

【実行照合済み】仕様書 6.3 節のテスト（`crates/riemann/tests/parallel_batch.rs`）：スレッド数 1・2・4・8・既定での完全一致、繰り返しの一致、順序の反転と分割の一致、失敗の分離、後の項目が先に終わる場合の出力順、指定したスレッド数のプールで実行されること。R 側でも、並べ替えた入力と順序のばらばらな日付で、出力順が入力順のままであることを確かめる（`tests/testthat/test-direct.R`）。比較はデバッグ文字列で行い、浮動小数のすべてのビットとエラーを含む。検証レポートでも、照合用データ全件で完全な一致を確かめる。

**スレッドと浮動小数点の状態**【実行確認】：R のメインスレッドの x87 制御ワードは `0x037F`（拡張精度、64 ビット）、新しいスレッドは `0x027F`（倍精度、53 ビット）である（2026-11-01、`fnstcw` で実測。SSE の MXCSR は同じ `0x1F80`）。GNU ビルドの数学関数はこの影響を受けるので、`threads = 1` だけメインスレッドで計算していたときは、スレッド数で最下位ビットが変わった（2・4・8・既定は互いに一致し、1 だけが 1 ビット違う行があった）。x87 の設定を変えるには手書きの `unsafe` が要るので変えない。代わりに、バッチは常にワーカースレッドで計算する。

したがって、**バッチと直接呼び出しは最下位の桁で違うことがある**。検証レポートに差を表示する。照合用データの 15,128 個の割引係数で、8,378 個が完全に一致し、最大差は絶対 9.6e-13、相対 1.03e-12 だった（2026-11-01）。絶対差は `docs/spec_ois.md` 10.4 節の 2e-12 以内である。QuantLib R パッケージとの差がちょうど 0 なのは、同じメインスレッドで計算する直接呼び出しの場合である。

同じレポートの 1,515 カーブ（19,673 行）で、スレッド数 1・2・4・8 の結果と入力順は完全に一致した。所要時間は参考値で、10.3 秒、5.9 秒、4.2 秒、4.4 秒だった。

## 6. 照合結果【実行照合済み】

2026-11-01。`tools/verify/verify_riemann.qmd`。比較の相手は、正確な C++ コミットが不明な QuantLib R パッケージ Version 1.42 である（カレンダーは C++ v1.42 そのもの）。

**直接呼び出しと CSV 経由の比較**：

| 対象 | 比べた数 | 完全一致 | 片側だけのエラー | 最大差（絶対） |
|---|---:|---:|---:|---:|
| 債券 | 1,260 | 1,232 | 0 | 2.2e-11 |
| OIS カーブ | 33,480 | 27,810 | 0 | 4.4e-12 |
| カレンダー（69 市場） | 7,535,352 日 | 差 0 | — | — |

債券の差は BPV が最大（値が大きい）で、クリーン価格は最大 1.1e-12 である。

**ビルドによる差**：R パッケージは Rtools の GNU ツールチェーン、CSV 経由と `cargo test` は MSVC でビルドする。GNU ビルドでは、OIS の照合用データ全件（TONAR・€STR・SOFR・SONIA の 1,700 通りのうち、作成できた 1,515 カーブ）で、節点、任意の日付の割引係数、再現した気配の QuantLib R パッケージとの差が**ちょうど 0** だった。債券（180 シナリオ）も、QuantLib 側を和で求めるダーティ価格を除いて完全に一致した。同じ入力で MSVC では最下位ビットの差が出るので、これまで「未確認の推定」としてきた微小な差の主な原因は、数学ライブラリーの違いと判断できる（`docs/spec_ois.md` 10.6 節）。ただし、関数ごとに個別には確かめていない。5 章のとおり、x87 の精度もこの差に関わる。

**入力気配の再現は別の判定である**：GNU ビルドでも、€STR の 2 カーブ（3 商品）と SOFR の 1 カーブ（2 商品）は基準未達のままである（`docs/spec_ois.md` 10.3 節）。

## 7. 未対応

- R の入口に `JointCalendar` と `BespokeCalendar` がない。検証レポートのこれらは CSV 経由のままである。
- 例外日（`with_holiday`）、`first_date`・`next_to_last_date` のバッチでの指定、`HolidayCalendar` の単独の値は、R の入口にない。
- ヘルプページ（`man/`）がない。関数の説明は `R/` のコメントだけである。
- 仕様書 10 章の「testthat で GaussQuant と直接比べる」は未実施である。
- `R CMD check` は実行していない。Windows 以外では、ビルドも確かめていない（`Makevars` は未実行）。
