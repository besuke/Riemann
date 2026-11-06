# RiemannFinMath（リポジトリ名・crate 名は Riemann）

Rust の金融計算ライブラリー。利用の主体は R（extendr）で、TypeScript の Node 版（napi-rs）とブラウザー版（wasm-bindgen）は開発目標である。ユーザーとのやり取りは日本語。

仕様の本体は `docs/` にある。ここには、次の会話で作業を判断するための決定事項・進捗・手順だけを書く。

| 文書 | 内容 |
|---|---|
| `docs/specification.md` | 全体の設計（原則 R1〜R16、7 章の開発段階、5.1 の許容誤差） |
| `docs/spec_dates_calendars.md` | P1 の仕様と照合結果 |
| `docs/spec_bonds.md` | P2 の仕様と照合結果（2 章 P2-1、3 章 P2-2、4 章 P2-3、5 章 P2-4） |
| `docs/spec_ois.md` | P3 の OIS カーブの仕様（QuantLib のソース調査、差、未確認事項） |
| `docs/spec_r.md` | P4 の R の入口（構成、公開関数、エラーと panic、並列バッチ、照合結果、未対応） |
| `docs/spec_autodiff.md` | 自動微分の設計（finmath-lib の方式）と P5 の結果（5 章） |
| `docs/spec_options.md` | P6 のオプション（QuantLib との差の原因は 5 章） |
| `docs/spec_asset_swaps.md` | P7 のアセットスワップと国債先物 |
| `docs/spec_hull_white.md` | P9 の Hull-White と SABR（差の原因は 4 章、未対応は 6 章） |
| `docs/spec_cds.md` | P8 の CDS |
| `docs/spec_prdc.md` | P10 の PRDC（簡略モデルの範囲は 1 章、未対応は 4 章） |
| `docs/spec_ois_swap.md` | P11-2 の OIS スワップ本体（QuantLib の仕様は 2 章、旧版について気づいた点は 5 章） |
| `docs/spec_longstaff_schwartz.md` | P11-4 の Sobol 列と Longstaff–Schwartz（比較条件と seed は 1・2 章、関数の形の比較は 7 章） |
| `docs/spec_mbs.md` | P11-5 のパススルー MBS（LagrangeFinance の条件は 1 章、違いは 2 章） |
| `docs/spec_lmm.md` | P11-6 の LMM の構成要素（Caplet と相関ショック。範囲は冒頭、LagrangeFinance の条件は 1 章） |
| `docs/spec_rates_fx_extension.md` | 金利・通貨機能の拡張（通貨ベーシスの期間構造、担保 CTD、FMM、LMM）：現状の分類、検証基準、後続課題 |
| `docs/quantlib_guide_inventory.md` | QuantLib Guide（全 39 ページ）と公式 Examples（v1.42）の棚卸し：対応表、不足機能、優先順位、今の機能で作れるサンプル |
| `docs/design_distributed.md` | 分散計算基盤（Azure・Slurm・R と Rust の並列の区別）、開発方針（入口は厳格に、計算コアは単純に）、GPU の構想 |
| `docs/library_comparison.md` | QuantLib と finmath-lib の比較（参考） |

## 決定事項

- **設計の主題**：オブジェクト指向とテンプレートからの脱却。値と純粋関数、enum と match。継承・オブザーバー・グローバル状態（評価日など）は使わない。利用者が選ぶものは実行時の値。総称型は禁止しないが、内部（自動微分など）に留めて公開 API に出さない。`#![forbid(unsafe_code)]`。
- **対応言語**：R と TypeScript が主。C ABI は不要。Python は必要になってから。言語間の一致は計算ごとの許容誤差で確かめ、ネイティブと WebAssembly は分けて扱う。
- **日付・カレンダー・規約**：QuantLib に従う。日本のカレンダーは「QuantLib 互換の実装」であり、祝日法の説明とは区別する。
- **照合の正**：QuantLib R バインディング 1.42。仕様を読むソースは `70f240f`（1.45-dev）だが、計算が変わっている部分は v1.42（`41b0e146`）に合わせる。
- **P3 の基準**：QuantLib C++ `v1.42`。R パッケージとの照合は「正確な C++ コミットが不明な Version 1.42 との照合」と記録し、C++ `v1.42` そのものとの実行照合とは区別する（`docs/spec_ois.md` 1 章）。
- **日本式債券計算の規則**：`docs/spec_bonds.md` 5.1 節の規則を採用し、GaussQuant の `R/bond_yen_GQL.R`（コミット `4ff081c`）と照合する。文書では計算規則を直接説明し、VBA を根拠にしない（「VBA 互換」などと書かない）。二次情報の「市場慣行」だけを根拠に別の規則へ変えない。2 月 29 日の応当日が平年になる場合は 2 月 28 日とし、その期間を 1 年とする（例：2024-02-29 から 2025-02-28 は残存 1 年）。この規則は日本式債券計算だけのもので、QuantLib 互換の日付・日数計算には適用しない。
- **新興国カレンダーの用途**：当面は債券の受渡日と利払日の営業日判定に使う。スワップの規約や金利カーブには広げない。OIS は TONAR・€STR・SOFR・SONIA の 4 指標に限定する。
- **カレンダーの照合の正**：QuantLib C++ v1.42 をビルドした値（`tools/golden/p1_all_calendars.sh`）。R パッケージは突き合わせに使う（`Germany::Euwax` と `Indonesia::IDX` は R から作れない）。
- **開発の目的**：自分の研究と分析の基盤。LagrangeFinance の第 II 部と第 III 部に相当する計算を備える。**利用の主体は R**。既存の LagrangeFinance は保持して変更しない。このリポジトリーの QMD は、動作確認用の `tools/verify/verify_riemann.qmd` と、QuantLib との比較資料 `comparison/` である。自動微分は torch ではなく自前で行う。第 I 部に相当する計算は対象外。対象は、市場価格の再現、解析解、無裁定の関係で検証できる計算（仕様書 1.2 節）。
- **正式名称**：RiemannFinMath（R パッケージ名も同じ）。リポジトリ名、crate 名（`riemann`、`riemann-r`）、API 名は変えない。CRAN への公開申請はしていない。
- **著作権とライセンス**：著作権者は Eiichi Horigome、ライセンスは MIT と Apache-2.0 の併用。QuantLib 由来の部分は元の著作権者のまま。配布は社内のみの想定。リポジトリは現在公開で、非公開化は保留。
- **自動微分**：finmath-lib の方式（アリーナのグラフ、グローバルな ID カウンターなし）。`Operand` trait は P5 で入れた。2 階微分は逆伝播の記録、気配の微分は解いたカーブでの陰関数定理。
- **ライセンス**：MIT と Apache-2.0 の併用。QuantLib から移したコードは、著作権表示をソースから写し（記憶で書かない）、ファイルの先頭と `THIRD_PARTY_NOTICES` に載せる。

## 命名の方針

- **R と Rust の公開トップレベル関数は動詞から始める**（2026-11-03 に確定）。基本は `create_`（作成）、`generate_`（規則から生成）、`get_`（保存済みの値を取得）、`compute_`（新たに算出）。意味が明確な `bootstrap_`（実際にブートストラップする場合）、`calibrate_`、`is_`、`has_`、`with_`（元の値を変えず新しい値を返す場合）も認める。`make_`、`calculate_`、`value_` と名詞だけの名前は使わない。Rust のメソッドは慣用名のまま。比較先のライブラリーの名前は変えない。旧名と新名の対応は `docs/naming_migration.md`。
- 関数・メソッド・変数・モジュールは snake_case。
- 型・構造体・enum・trait は PascalCase。
- 定数は SCREAMING_SNAKE_CASE。
- R から自然に使えるよう、Rust と R で公開関数名・引数名をできるだけ揃える。
- 言語の境界で変換が必要な場合も、不要な改名は避ける。

詳細は `docs/specification.md` 4.4 節。

## 作業のきまり

- 文書の記述は【ソース確認】【実行照合済み】【設計案】を分ける。確かめていないことを断定しない。原因が推定なら「未確認の推定」と書く。
- 照合日は実行ログの実際の日付を書く。見た目を揃えるために変えない。
- エラーの一致は、件数だけでなく原因も確かめる。
- GaussQuant・LagrangeFinance・Curie は、Riemann の作業では変更しない。
- 市場の気配値（Bloomberg のデータ）は表や出力に出さない。
- コミットと push は、ユーザーに頼まれたときだけ行う。P6 以降の拡充は、段階ごとにテストと clippy が通ったら確認なしで commit と push をしてよい（2026-11-01 のユーザーの指示）。
- 段階ごとの手順：照合用 R スクリプト → Rust の実装 → 照合テスト → 仕様書と `THIRD_PARTY_NOTICES` の更新 → 全テストと clippy → 報告。

## 進捗（2026-11-02 時点）

| 段階 | 内容 | 状態 |
|---|---|---|
| P1 | 日付、カレンダー、営業日調整、スケジュール、日数計算 | 完了。カレンダーは QuantLib C++ v1.42 の全カレンダー・全市場区分（41 の国・地域、69 市場）を全日付で照合済み（`docs/spec_dates_calendars.md` 3.5 節）。一致は QuantLib とのもので、実際の市場休日は未検証 |
| P2-1 | 固定利付債のキャッシュフロー、経過利子 | 完了 |
| P2-2 | 利回りと価格、デュレーション、コンベクシティ、BPV | 完了 |
| P2-3 | 割引カーブによる評価（フラット、ゼロ、割引係数、ゼロスプレッド） | 完了 |
| P2-4 | 日本式の単利・複利利回りとリスク指標（`yen_bond.rs`） | 完了 |
| P3 | OIS のブートストラップ | **完了。TONAR・€STR・SOFR・SONIA の `MonotonicLogCubic` を照合済み（`docs/spec_ois.md` 8〜10 章）。指標はこの 4 つに限定する。±8% で振れる入力の 3 カーブは、QuantLib と一致するが入力気配の再現基準は未達（10.3 節）。GaussQuant・LagrangeFinance との照合（仕様書 7 章）は未実施。計算方式は QuantLib C++ で定め、R バインディングは照合だけに使う。QuantLib の既定を市場の規約として扱わない** |
| P4 | R の入口（R パッケージ `RiemannFinMath`） | **直接呼び出し・並列評価・Positron での利用確認は完了**（`crates/riemann-r`、extendr。`docs/spec_r.md`）。**配布整備は作業中**：版 0.2.0（`c6218ae`）で Windows のソースパッケージ、`R CMD check`（NOTE 1）、バイナリ zip、別ライブラリーでの確認を実施（`dist/MANIFEST.txt`）。**0.2.0 の quickstart は、利用者の Positron でも正常動作を確認済み（利用者の報告）**。Windows 以外、配布先の公開は未了（同 8 章）。GaussQuant との比較は未実施 |
| P5 | 自動微分の基礎：確定値の計算グラフ、2 階微分、債券の利回り・カーブ・気配の感応度 | **完了**（`docs/spec_autodiff.md` 5 章）。R は `yield_sensitivity`、`curve_node_sensitivity`、`quote_sensitivity` |
| P6 | 株のオプション：ヨーロピアン、アメリカン（二項木）、バリア、Black、Bachelier、MC。自動微分のグリークス | **完了**（`docs/spec_options.md`）。QuantLib の二項木の丸めの不具合を 5 章に記録 |
| P7 | 債券：ASW、円ベーシス込み ASW、国債先物と CTD | **完了**（`docs/spec_asset_swaps.md`） |
| P9 | Hull-White、コール債、SABR、Bermudan | **主要部分が完了**（`docs/spec_hull_white.md`）。LMM、HW 校正、Longstaff–Schwartz、MBS は未対応 |
| P8 | CDS（MidPoint、ISDA、ハザードのブートストラップ、`cds`・`cds2015` 規則） | **完了**（`docs/spec_cds.md`） |
| P10 | PRDC：解析解、MC、自動微分 | **LagrangeFinance III-7 の簡略モデルと、確率的な金利の 3 因子モデル（解析式と MC、コール、感応度、2026-11-07）まで完了**（`docs/spec_prdc.md` 5〜7 章）。コール付きの感応度、ターゲット償還は未対応 |
| L1〜L3 | TypeScript の Node 版、ブラウザー版、Excel | 未着手。目標に必須ではない。P5 の後に WebAssembly で移植性を確認 |

件数と結果は各仕様書の照合結果の節にある。段階の内容は `docs/specification.md` の 7 章。

## 次の作業：不足機能の実装（2026-11-02 のユーザーの指示）

`comparison/chapters/appendix_c_gaps.qmd` の不足を、下の順に実装する（2026-11-03 のユーザーの指示）。各項目は、LagrangeFinance の入力・規約・計算方法と QuantLib C++ の該当実装を読んでから着手し、Rust 中核、R 公開 API、ヘルプと使用例、比較資料への追加までを一組にする。項目ごとに確認してコミットと push をする。回帰テストの許容誤差は変えない。配布ファイルの更新は、機能追加が一区切りついた後にまとめて行う。

| 段階 | 内容 | 状態 |
|---|---|---|
| P11-1 | Z スプレッド、債券キャッシュフロー表 | **完了**（`docs/spec_bonds.md` 4.4 節）。QuantLib v1.42 のソースを読み、探索を同じ手順に直した。境界ケース 25 件も一致 |
| P11-2 | OIS スワップ本体（レグ、NPV、日次複利、Fixing。LagrangeFinance II-3.2〜II-3.4） | **完了**（`docs/spec_ois_swap.md`）。QuantLib と 300 件照合。旧版について気づいた点は同 5 章 |
| P11-3 | Hull-White の校正（II-6.4） | **完了**（`docs/spec_hull_white.md` 1.1 節）。QuantLib の校正 60 件と照合。`ln(a)`、`ln(sigma)` などの独自の選択と、比較元との違いを同節に明記 |
| P11-4 | Longstaff–Schwartz、Sobol 列（II-4.6） | **完了**（`docs/spec_longstaff_schwartz.md`）。Sobol・パス・価格を QuantLib と照合、LagrangeFinance II-4.6.2 を再現。基底・パス共有・パス数の比較は `tools/experiments/p11_ls_function_shape.R` |
| P11-5 | MBS のプリペイメントと OAS（II-8.8） | **完了**（`docs/spec_mbs.md`）。LagrangeFinance のシナリオ表と 20 経路を再現。静的 OAS は比較元の `uniroot` の精度で最大 0.26 bp の差（N/C） |
| P11-6 | LMM。**当面は LagrangeFinance の Caplet と相関の例（II-7.4）を再現する範囲**（ユーザーの指示）。本格的なモデルへの拡張は別途判断 | **完了**（`docs/spec_lmm.md`）。同じ R の乱数を渡し、Caplet と標本相関を差 0〜1.3e-18 で再現。LMM 全体は未実装 |

**配布版 0.3.0（2026-11-04）**：`dist/` に tar.gz と zip を作った（`docs/spec_r.md` 8.6 節、`dist/MANIFEST.txt`）。zip は別ライブラリー・Rust なしの PATH・リポジトリ外で smoke test 34 項目が成功。**tar.gz の `R CMD check` とソース導入は未完了**：新しいフォルダーでのビルドスクリプトを Smart App Control が 2 回ブロックした。zip は tar.gz からではなくリポジトリのパッケージから作った。設定は変えておらず、回避もしていない。

## QuantLib Guide と公式 Examples（2026-11-04）

`docs/quantlib_guide_inventory.md` 4 章の優先順位 1〜6 を実装した（カーブの作成関数、日付・スケジュール・キャッシュフローの NPV、SimpleThenCompounded とステップアップ・アモチ債、CDS の Upfront と生存確率カーブ、債券価格のブートストラップと区分線形スプレッド、American の近似式）。サンプルは `crates/riemann-r/inst/examples/quantlib_guide/`（Guide）と `quantlib_examples/`（公式 Examples）。Guide の表示との差は、QuantLib 1.42 と一致していれば「版の差」として記録する。R のコードは tidyverse の書き方（`purrr::map_*()`、`walk()`）にする（2026-11-04 のユーザーの指示）。残り：4 章の「後回し」の項目、公式 Examples のサンプルの追加、比較資料の全体の再描画（付録 A の新しい関数）。

## QuantLib との比較資料（`comparison/`、2026-11-01 完了）

LagrangeFinance の入力と計算例で、RiemannFinMath（Rust）を基準と比べる英語の資料。翻訳した教材ではない（当初の英語教材の案はユーザーの指示でこの形に変わった）。

- **基準（ユーザーの指示）**：カレンダーは本家 QuantLib C++（v1.42 をビルドした `tests/golden/calendar_all_*.csv`）、それ以外は LagrangeFinance の計算結果（`comparison/baseline/lagrange_values.csv`）。判定はこの基準に対する行だけ。QuantLib R パッケージなどを直接呼ぶ行は参考（REF）で、判定しない。
- **基準値の作り直し**：`comparison/baseline/README.md`。LagrangeFinance が変わったら作り直す。今の値はコミット `05e1932` から。

- **出力**：`quarto render --to html`（`comparison/` で実行）で `comparison/_book/index.html`。`_book` は git に入れない。丸める前の結果は `comparison/results/*.csv` で git に入れる。
- **基準**：絶対誤差 1e-4、表示は小数 4 桁、判定は丸める前、単位を明記。この資料だけの基準で、ライブラリーの回帰テストの許容誤差は変えない。
- **きまり**：文書に日本語を入れない。各項目に入力、両者の呼び出し、結果、差、許容誤差、判定を載せ、ラッパーの差と中核の差を分ける。同じ計算でないものは N/C にして判定しない。QuantLib に相当がないものは基準（GaussQuant など）を明記する。
- **結果**（2026-11-02、`6a5d9d7` のコード）：全 63 関数を呼んだ。PASS 386、FAIL 0、N/C 40、REF 402。LagrangeFinance 基準は PASS 173、N/C 17。カレンダー（QuantLib C++ 基準）は PASS 213。未比較は `appendix_c_gaps.qmd`。Bermudan は基準値がなく参考行（木との差は 1000 段で 0.0011、2000 段で 0.0002）。CDS の FAIL 2 行の原因は `docs/spec_cds.md` 3.1 節で確定し、LagrangeFinance の入力と基準値を変えずに、Rust 側の引数の追加で解消した（比較資料の CDS の呼び出しはこの引数を指定する）。
- **比較資料の残り**：
  1. 判定できない項目を減らすか決める。PRDC は旧版がモンテカルロの値だけで判定なし、第 5 章と第 13 章は旧版に対応する値がない。
  2. `docs/library_comparison.md` の先頭の空行 1 行は未コミットのまま（由来不明）。扱いをユーザーに確認する。
- **市場データ**：2026-10-06 の OIS 気配と受渡適格銘柄の価格は、描画時に `../../LagrangeFinance` から読むだけで、保存も表示もしない。
- **注意**：bash のヒアドキュメントでは `\\` が `\` になる。バックスラッシュを含む R は Write で書く。

## 未解決の点

- 許容誤差内の微小な差：P2-2 は R と Rust の数学ライブラリーの差を確かめた。P2-3・P2-4 の差も同じ原因と考えているが、未確認の推定。
- カーブ（P2-3）について、v1.42 と `70f240f` の差は確認していない。
- `cargo fmt --check` は既存のコードで差が出る（rustfmt の設定なし、行が長め）。新しいファイルと変更した箇所だけ整形し、無関係なファイルをまとめて整形しない。

## 環境（Windows）

- テスト：`cargo test --release`。clippy：`cargo clippy --all-targets -- -D warnings`。
- 新しいテストの実行ファイルで LNK1104 が一時的に出ることがある。もう一度実行すれば通る。
- R は PATH にないので、フルパスで実行する：`"C:/Program Files/R/R-4.6.0/bin/Rscript.exe"`。R の中のパスは `C:/...` の形で書く（`/c/...` は使えない）。
- 照合用の値は Riemann のルートから `tools/golden/*.R` を実行し、`tests/golden/` に書き出す。仕様が変わらなければ作り直さない。
- 検証レポート：`tools/verify/verify_riemann.qmd`（`quarto render`）と `verify_common.R`、`verify_bonds.R`、`verify_ois.R`、`verify_calendars.R`。R の入口（P4）ができるまで、`crates/riemann/examples/` の `verify_bonds.rs`、`verify_ois.rs`、`verify_calendars.rs` と CSV で受け渡す。OIS は 4 指標の `MonotonicLogCubic` と対数線形を比べる。
- **アプリ制御**：Smart App Control が有効で、新しくビルドした未署名の実行ファイルを止めることがある（`os error 4551`、`Permission denied`）。起動元によらず、同じファイルでも判定が変わる。イベントログは `Microsoft-Windows-CodeIntegrity/Operational`（ID 3033、3077）。回避しない。レポートで止まった例だけ、環境変数 `RIEMANN_VERIFY_SKIP_EXAMPLES`（例：`verify_bonds`）で飛ばす。その CSV 経路は「not run」と表示される。
- **比較文書の描画の中断**：`quarto render` が途中の章で終了コード 1 になり、`WARN: Error encountered when rendering files` だけを出して止まることがある（2026-11-04 に 2 回、3 章と 10 章）。エラーの本文と失敗したチャンク名は出力されなかった。その章は単独では描画でき、同じ時刻にアプリ制御の記録はなく、再実行で全 14 章が通った。原因は未確認で、アプリ制御とも断定しない。 **成功の判定**：外側のシェルの終了コードではなく、`quarto render` 自身の終了コードが 0、ログに `WARN` がない、結果の CSV が 14 章分ある、の三つで判定する。リークチェックはその後に別に実行し、描画とは分けて報告する。 **失敗したときの手順**（全体の再実行を繰り返す前に）：(1) 描画ログ全体を日時つきで保存する。(2) 終了コードと、ログの最後の進捗行（章名と、直前のチャンク名 `[label]`）を記録する。(3) 失敗した章だけを `quarto render chapters/<章>.qmd --log-level debug` で描画し、出力と終了コードを保存する。(4) 同じ章を `Rscript R/check_chapter.R <章>` で実行し、最初のエラーの全文と失敗したチャンクを記録する。(5) そのうえで全体を再実行する。
- `Rscript -e` は引用符で失敗しやすいので、スクリプトファイルにする。パッケージのインストールは `devtools::install` ではなく `R CMD INSTALL`（pak の DLL がアプリ制御で止められる）。
- `rustfmt` にファイルを渡すと `mod` 先まで整形する。テストのファイルでは `tests/common/mod.rs` まで変わるので、`--config skip_children=true` を付けて確かめる。
- CRLF のファイルは MSYS の sed で CR が消える。`tests/golden` の CSV は R が CRLF で書き、git で LF になる（`.gitattributes`）。
- カレンダーの規則は `tools/golden/port_calendars.R` が生成する（`rules/` の `special.rs` 以外）。生成後に rustfmt をかける。C++ のビルドは Rtools の g++ を PATH に入れて、bash から実行する。
- R パッケージ：`R CMD INSTALL crates/riemann-r`（`x86_64-pc-windows-gnu` と Rtools が必要）。配布ファイルは `tools/package/build_source_package.R` で作り、`dist/`（git に入れない）に置く。ソースからのビルドはパスが長いと `LNK1104` で失敗するので、短いパスで行う。テストは `crates/riemann-r/tests/testthat`。このビルドは MSVC の `cargo test` と数学ライブラリーが違い、最下位ビットの差が出る（`docs/spec_ois.md` 10.6 節）。
- QuantLib のソース（v1.42 の確認用）は以前の会話の一時フォルダーにあり、新しい会話では使えない。必要なら一時フォルダーに clone し直す。
