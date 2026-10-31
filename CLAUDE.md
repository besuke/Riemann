# Riemann

Rust の金融計算ライブラリー。R（extendr）→ TypeScript の Node 版（napi-rs）→ ブラウザー版（wasm-bindgen）の順に呼べるようにする。ユーザーとのやり取りは日本語。

仕様の本体は `docs/` にある。ここには、次の会話で作業を判断するための決定事項・進捗・手順だけを書く。

| 文書 | 内容 |
|---|---|
| `docs/specification.md` | 全体の設計（原則 R1〜R16、10 章の開発段階、7.3 の許容誤差） |
| `docs/spec_dates_calendars.md` | P1 の仕様と照合結果 |
| `docs/spec_bonds.md` | P2 の仕様と照合結果（2 章 P2-1、3 章 P2-2、4 章 P2-3、5 章 P2-4） |
| `docs/spec_ois.md` | P3 の OIS カーブの仕様（QuantLib のソース調査、差、未確認事項） |
| `docs/spec_autodiff.md` | 自動微分の設計（finmath-lib の方式） |
| `docs/library_comparison.md` | QuantLib と finmath-lib の比較（参考） |

## 決定事項

- **設計の主題**：オブジェクト指向とテンプレートからの脱却。値と純粋関数、enum と match。継承・オブザーバー・グローバル状態（評価日など）は使わない。利用者が選ぶものは実行時の値。総称型は禁止しないが、内部（自動微分など）に留めて公開 API に出さない。`#![forbid(unsafe_code)]`。
- **対応言語**：R と TypeScript が主。C ABI は不要。Python は必要になってから。言語間の一致は計算ごとの許容誤差で確かめ、ネイティブと WebAssembly は分けて扱う。
- **日付・カレンダー・規約**：QuantLib に従う。日本のカレンダーは「QuantLib 互換の実装」であり、祝日法の説明とは区別する。
- **照合の正**：QuantLib R バインディング 1.42。仕様を読むソースは `70f240f`（1.45-dev）だが、計算が変わっている部分は v1.42（`41b0e146`）に合わせる。
- **P3 の基準**：QuantLib C++ `v1.42`。R パッケージとの照合は「正確な C++ コミットが不明な Version 1.42 との照合」と記録し、C++ `v1.42` そのものとの実行照合とは区別する（`docs/spec_ois.md` 1 章）。
- **日本式債券計算の規則**：`docs/spec_bonds.md` 5.1 節の規則を採用し、GaussQuant の `R/bond_yen_GQL.R`（コミット `4ff081c`）と照合する。文書では計算規則を直接説明し、VBA を根拠にしない（「VBA 互換」などと書かない）。二次情報の「市場慣行」だけを根拠に別の規則へ変えない。2 月 29 日の応当日が平年になる場合は 2 月 28 日とし、その期間を 1 年とする（例：2024-02-29 から 2025-02-28 は残存 1 年）。この規則は日本式債券計算だけのもので、QuantLib 互換の日付・日数計算には適用しない。
- **自動微分**：finmath-lib の方式（アリーナのグラフ、グローバルな ID カウンターなし）。`Operand` trait は AD-1 の段階で入れる。
- **ライセンス**：MIT と Apache-2.0 の併用。QuantLib から移したコードは、著作権表示をソースから写し（記憶で書かない）、ファイルの先頭と `THIRD_PARTY_NOTICES` に載せる。

## 命名の方針

- 関数・メソッド・変数・モジュールは snake_case。
- 型・構造体・enum・trait は PascalCase。
- 定数は SCREAMING_SNAKE_CASE。
- R から自然に使えるよう、Rust と R で公開関数名・引数名をできるだけ揃える。
- 言語の境界で変換が必要な場合も、不要な改名は避ける。

詳細は `docs/specification.md` 11.1 節。

## 作業のきまり

- 文書の記述は【ソース確認】【実行照合済み】【設計案】を分ける。確かめていないことを断定しない。原因が推定なら「未確認の推定」と書く。
- 照合日は実行ログの実際の日付を書く。見た目を揃えるために変えない。
- エラーの一致は、件数だけでなく原因も確かめる。
- GaussQuant・LagrangeFinance・Curie は、Riemann の作業では変更しない。
- 市場の気配値（Bloomberg のデータ）は表や出力に出さない。
- コミットと push は、ユーザーに頼まれたときだけ行う。
- 段階ごとの手順：照合用 R スクリプト → Rust の実装 → 照合テスト → 仕様書と `THIRD_PARTY_NOTICES` の更新 → 全テストと clippy → 報告。

## 進捗（2026-10-31 時点）

| 段階 | 内容 | 状態 |
|---|---|---|
| P1 | 日付、カレンダー、営業日調整、スケジュール、日数計算 | 完了 |
| P2-1 | 固定利付債のキャッシュフロー、経過利子 | 完了 |
| P2-2 | 利回りと価格、デュレーション、コンベクシティ、BPV | 完了 |
| P2-3 | 割引カーブによる評価（フラット、ゼロ、割引係数、ゼロスプレッド） | 完了 |
| P2-4 | 日本式の単利・複利利回りとリスク指標（`yen_bond.rs`） | 完了 |
| P3 | OIS のブートストラップ | **TONARの対数線形まで完了（`docs/spec_ois.md` 8 章）。仕様で採用した `MonotonicLogCubic` は未実装。次は P3-3（`MonotonicLogCubic` を TONAR で照合）で、P3-2（€STR・SOFR・SONIA）はその確認後に行う。計算方式は QuantLib C++ で定め、R バインディングは照合だけに使う。QuantLib の既定を市場の規約として扱わない** |
| P4 | R の入口 | 未着手 |
| P5 | OIS の ASW と円ベーシス込み ASW | 未着手 |
| P6・P7 | TypeScript の Node 版、ブラウザー版 | 未着手 |

件数と結果は各仕様書の照合結果の節にある。P3 以降の内容は `docs/specification.md` の 10 章。

## 未解決の点

- 許容誤差内の微小な差：P2-2 は R と Rust の数学ライブラリーの差を確かめた。P2-3・P2-4 の差も同じ原因と考えているが、未確認の推定。
- カーブ（P2-3）について、v1.42 と `70f240f` の差は確認していない。
- `cargo fmt --check` は既存のコードで差が出る（rustfmt の設定なし、行が長め）。新しいファイルと変更した箇所だけ整形し、無関係なファイルをまとめて整形しない。

## 環境（Windows）

- テスト：`cargo test --release`。clippy：`cargo clippy --all-targets -- -D warnings`。
- 新しいテストの実行ファイルで LNK1104 が一時的に出ることがある。もう一度実行すれば通る。
- R は PATH にないので、フルパスで実行する：`"C:/Program Files/R/R-4.6.0/bin/Rscript.exe"`。R の中のパスは `C:/...` の形で書く（`/c/...` は使えない）。
- 照合用の値は Riemann のルートから `tools/golden/*.R` を実行し、`tests/golden/` に書き出す。仕様が変わらなければ作り直さない。
- 検証レポート：`tools/verify/verify_riemann.qmd`（`quarto render`）と `verify_bonds.R`。R の入口（P4）ができるまで、`crates/riemann/examples/verify_bonds.rs` と CSV で受け渡す。OIS は P3 の実装後に `MonotonicLogCubic` で追加する。
- `Rscript -e` は引用符で失敗しやすいので、スクリプトファイルにする。パッケージのインストールは `devtools::install` ではなく `R CMD INSTALL`（pak の DLL がアプリ制御で止められる）。
- `rustfmt` にファイルを渡すと `mod` 先まで整形する。テストのファイルでは `tests/common/mod.rs` まで変わるので、`--config skip_children=true` を付けて確かめる。
- CRLF のファイルは MSYS の sed で CR が消える。`tests/golden` の CSV は R が CRLF で書き、git で LF になる（`.gitattributes`）。
- QuantLib のソース（v1.42 の確認用）は以前の会話の一時フォルダーにあり、新しい会話では使えない。必要なら一時フォルダーに clone し直す。
