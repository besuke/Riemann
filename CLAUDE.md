# RiemannFinMath（リポジトリ名・crate 名は Riemann）

Rust の金融計算ライブラリー。利用の主体は R（extendr）で、TypeScript の Node 版（napi-rs）とブラウザー版（wasm-bindgen）は開発目標である。ユーザーとのやり取りは日本語。

**Riemann は計算サーバー（計算エンジン）である**（2026-11-18 のユーザーの指示）。クライアントのリポジトリは金融計算を自分で実装せず、RiemannFinMath を呼ぶ。足りない計算はここに足す。作業の会話は Riemann のフォルダーで始め（リポジトリ直下の `Riemann-Claude.bat`）、クライアントもここから扱う。全リポジトリ共通のきまりはこのファイルにまとめた（親の `../CLAUDE.md` は置かない）。

仕様の本体は `docs/` にある。ここには、次の会話で作業を判断するための決定事項・状態・手順だけを書く。

## クライアントと関連リポジトリ（`C:\AnalyticFin\Projects`）

| フォルダー | 内容 | GitHub | 扱い |
|---|---|---|---|
| `carmine-studio` | Carmine Studio『エンタープライズ金融モデル』（旧 LagrangeFinLab）。React＋R サーバー（httpuv）。画面 8081、R API 8787 | besuke/carmine-studio | **クライアント** |
| `InfragoQuant` | Infrago Quant（旧 QuantLet）。Curie の後継の業務 Web アプリ。画面 8091、R API 8790 | besuke/infrago-quant（非公開） | **クライアント** |
| `crimson-proof` | Crimson Proof『Leanから見た金融数学』（旧 LagrangeAcademy）。Quarto＋Lean 4。R は `R/utility_LAD.R` だけから呼ぶ | besuke/crimson-proof（公開） | **クライアント**。QuantLib を一切使わない・言及しない |
| `coral-book` | Coral Book『金融工学の応用基礎』（旧 LagrangeFinance）。照合の基準 | besuke/coral-book | **変更しない** |
| `GaussQuant`、`Curie` | 既存の R の計算・業務ツール。照合の基準 | besuke/GaussQuant ほか | **変更しない** |
| `QuantLib_tidy`、`LeanStudy`、`TypeScriptStudy` | 学習・試作 | | |

- クライアントを扱う前に、そのリポジトリの `CLAUDE.md` を読む（自動では読まれない）。
- **クライアントが RiemannFinMath を入れる・更新する・呼ぶ方法は `docs/client_guide.md`**。クライアントの `CLAUDE.md` や README からここを指す。
- 別の会話が同じリポジトリで作業中のことがある。未コミットの変更があるリポジトリの共有ファイルは、頼まれない限り編集しない（2026-11-18、Infrago Quant に 34 件の未コミットの変更があった）。
- RiemannFinMath を入れ直す前に、その DLL を使っている R API（Infrago Quant、Carmine Studio）を止める。

### クライアントのニーズ（2026-11-18 に各リポジトリを読んで整理）

**使っている関数**（`RiemannFinMath::` の呼び出し）：

- Carmine Studio は 73 関数を使う。カーブ・債券・日本式利回り・OIS・ASW・国債先物・CDS・オプション・Hull-White・SABR・LMM・LS・MBS・PRDC・バッチと、ほぼ全域にわたる。
- Infrago Quant は 50 関数を使う。債券・日本式利回り・OIS・ASW・CDS・TRS・IRRBB。
- Crimson Proof は 7 関数を使う。オプション、PRDC、FX スマイル。
- 3 つとも、呼んでいる関数はすべて現在の公開関数にある（呼び出しの切れはない）。
- 公開関数の名前・引数を変えると、これらが壊れる。変えるときは呼び出し側を grep で確かめる。

**要望と未対応**：

| クライアント | 要望 | 出典 | Riemann の状態 |
|---|---|---|---|
| Carmine Studio | OIS スワップを期間（`7Y`）で作る（`MakeOIS` の規則） | `reconciliation/RECONCILIATION.md` 6 章 #1 | **対応済み（2026-11-18、未公開）**：`compute_ois_swap_dates()`。公開は 0.3.5。クライアント側の置き換えは未了（「まず読む」） |
| Carmine Studio | 照合に使った RiemannFinMath のコミットを固定する | 同 #5 | 0.3.2 は Releases の zip と `RemoteSha` で固定できるようになった。クライアント側の記録は未更新 |
| Carmine Studio | ブラウザーで直接動かす（WebAssembly 版） | 同 #6 | L1〜L3 は未着手 |
| Carmine Studio | 元本リセット型 CCS、FX スワップによる受渡日の調整、CCS の期間構造の校正 | `README.md` の円換算 ASW の「未対応」 | 一部の部品（`create_xccy_instruments` の `resetting`、`bootstrap_xccy_curve`）はある。クライアントから使える形かは未確認 |
| Infrago Quant | 他の PC で動かすための TRS・IRRBB の公開 | `CLAUDE.md` の状態（2026-11-16） | 解消。`main` に入り、0.3.2 の zip に含まれる |
| Crimson Proof | 描画時に Rust をビルドしない（導入済みのパッケージを使う） | `CLAUDE.md` | 0.3.2 の zip で満たせる |

## 共通の作業のきまり（全リポジトリ）

- コミットと push は頼まれたときだけ（Riemann の例外は「作業のきまり」）。
- 確かめていないことを断定しない。推定は「未確認の推定」と書く。照合日・確認日は実行ログの実際の日付を書く。数値例が合うことは「この例で成り立つ」ことで、命題の証明ではない。
- 市場の気配値（Bloomberg のデータ）を表や出力に出さない・保存しない。例の入力は合成の値と明記する。
- 安全の仕組み（Smart App Control、サイトのボット対策、認証）を回避しない。止められたら「not run」として記録し、ユーザーに伝える。
- 他の R のセッション（Positron など）を終了させない。
- R のコードは tidyverse の書き方（`purrr::map_*()`、`walk()`）。
- ユーザーのメールアドレスなど個人の情報を外部のサービスに送らない。

## 文書の索引（`docs/`）

- 全体：`specification.md`（原則 R1〜R16、7 章の開発段階、5.1 の許容誤差）、`naming_migration.md`（旧名と新名）、`client_guide.md`（クライアントの入れ方・呼び方）、`design_distributed.md`（分散計算・GPU の構想）、`library_comparison.md`（QuantLib と finmath-lib の比較）
- 照合の範囲：`lagrangefinance_coverage.md`（Coral Book の計算の網羅状況）、`quantlib_guide_inventory.md`（QuantLib Guide と公式 Examples の棚卸し）
- 段階ごとの仕様と照合結果：

| 段階 | 文書 |
|---|---|
| P1 日付・カレンダー | `spec_dates_calendars.md` |
| P2 債券（2〜5 章が P2-1〜P2-4） | `spec_bonds.md` |
| P3 OIS カーブ | `spec_ois.md` |
| P4 R の入口・配布 | `spec_r.md`（配布は 8 章） |
| P5 自動微分 | `spec_autodiff.md` |
| P6 株のオプション | `spec_options.md` |
| P7 ASW・国債先物 | `spec_asset_swaps.md` |
| P8 CDS | `spec_cds.md` |
| P9 Hull-White・SABR | `spec_hull_white.md` |
| P10 PRDC・FX スマイル | `spec_prdc.md` |
| P11 OIS スワップ／LS・Sobol／MBS／LMM | `spec_ois_swap.md`、`spec_longstaff_schwartz.md`、`spec_mbs.md`、`spec_lmm.md` |
| 金利・通貨の拡張 | `spec_rates_fx_extension.md` |
| P12〜P15 ポートフォリオ／市場VaR／信用VaR／トランシェ | `spec_portfolio.md`、`spec_market_risk.md`、`spec_credit_risk.md`、`spec_tranching.md` |
| TRS・IRRBB（Infrago Quant 用） | `spec_trs.md`、`spec_irrbb.md` |

## 決定事項

- **設計の主題**：オブジェクト指向とテンプレートからの脱却。値と純粋関数、enum と match。継承・オブザーバー・グローバル状態（評価日など）は使わない。利用者が選ぶものは実行時の値。総称型は内部（自動微分など）に留めて公開 API に出さない。`#![forbid(unsafe_code)]`。
- **対応言語**：R と TypeScript が主。C ABI は不要。Python は必要になってから。言語間の一致は計算ごとの許容誤差で確かめ、ネイティブと WebAssembly は分けて扱う。
- **開発の目的**：自分の研究と分析の基盤。Coral Book（旧 LagrangeFinance）の第 II 部と第 III 部、第 I 部の I-8〜I-11（ポートフォリオ最適化、市場VaR、信用VaR、トランチング。2026-11-14 の指示）に相当する計算を備える。I-1〜I-7 は R の統計関数で足りるので対象外。対象は、市場価格の再現、解析解、無裁定の関係、最適性条件で検証できる計算（仕様書 1.2 節）。自動微分は torch ではなく自前。
- **照合の正**：QuantLib R バインディング 1.42。仕様を読むソースは `70f240f`（1.45-dev）だが、計算が変わっている部分は v1.42（`41b0e146`）に合わせる。P3 とカレンダーは QuantLib C++ v1.42（R パッケージとの照合は「正確な C++ コミットが不明な Version 1.42 との照合」と区別して記録）。QuantLib の既定を市場の規約として扱わない。
- **日付・カレンダー・規約**：QuantLib に従う。日本のカレンダーは「QuantLib 互換の実装」であり、祝日法の説明とは区別する。新興国カレンダーは当面、債券の受渡日と利払日の判定だけに使う。OIS は TONAR・€STR・SOFR・SONIA の 4 指標に限る。
- **日本式債券計算**：`docs/spec_bonds.md` 5.1 節の規則で、GaussQuant の `R/bond_yen_GQL.R`（`4ff081c`）と照合する。計算規則を直接説明し、VBA を根拠にしない（「VBA 互換」と書かない）。二次情報の「市場慣行」だけで規則を変えない。2 月 29 日の応当日が平年なら 2 月 28 日とし、その期間を 1 年とする。この規則は日本式債券計算だけのもの。
- **自動微分**：finmath-lib の方式（アリーナのグラフ、グローバルな ID カウンターなし）。2 階微分は逆伝播の記録、気配の微分は解いたカーブでの陰関数定理。
- **正式名称**：RiemannFinMath（R パッケージ名も同じ）。リポジトリ名、crate 名（`riemann`、`riemann-r`）、API 名は変えない。CRAN への公開申請はしていない。
- **著作権とライセンス**：著作権者は Eiichi Horigome、MIT と Apache-2.0 の併用。QuantLib から移したコードは、著作権表示をソースから写し（記憶で書かない）、ファイルの先頭と `THIRD_PARTY_NOTICES` に載せる。配布は社内のみの想定。リポジトリは公開で、非公開化は保留。
- **関連リポジトリの改名（2026-11-15）**：実行時に読むパス（`tools/golden/p9`・`p11`・`p12`・`p13`、`comparison/R/book.R`）は新しい名前に直した。仕様書と照合記録の旧名は当時の名前として残す。

## 命名の方針

- **R と Rust の公開トップレベル関数は動詞から始める**（2026-11-03 確定）。`create_`、`generate_`、`get_`、`compute_` が基本。`bootstrap_`、`calibrate_`、`is_`、`has_`、`with_` も認める。`make_`、`calculate_`、`value_` と名詞だけの名前は使わない。Rust のメソッドは慣用名のまま。比較先のライブラリーの名前は変えない。
- 関数・変数・モジュールは snake_case、型・enum・trait は PascalCase、定数は SCREAMING_SNAKE_CASE。Rust と R で公開関数名・引数名を揃え、不要な改名は避ける。詳細は `docs/specification.md` 4.4 節。

## 作業のきまり

- 文書の記述は【ソース確認】【実行照合済み】【設計案】を分ける。エラーの一致は、件数だけでなく原因も確かめる。
- GaussQuant・Coral Book・Curie は、Riemann の作業では変更しない。
- 段階ごとの手順：照合用 R スクリプト → Rust の実装 → 照合テスト → 仕様書と `THIRD_PARTY_NOTICES` の更新 → 全テストと clippy → 報告。
- 機能の拡充は、段階ごとにテストと clippy が通ったら確認なしで commit と push をしてよい（2026-11-01 の指示）。それ以外のコミットと push は頼まれたときだけ。
- 回帰テストの許容誤差は変えない。
- 新しい項目は、Coral Book の入力・規約・計算方法と QuantLib C++ の該当実装を読んでから着手し、Rust 中核、R 公開 API、ヘルプと使用例、比較資料への追加までを一組にする（2026-11-03 の指示）。

## 状態と引継ぎ（2026-11-18）

### まず読む：いまの作業と次の一手

**作業中：OIS スワップをテナー（`7Y` など）から作る機能**（Carmine Studio の要望）。**引継ぎはこのファイルだけに書く**（仕様書の `docs/spec_ois_swap.md` 7 章は仕様と照合結果だけ）。

| 層 | 状態 |
|---|---|
| Rust の中核 `riemann::ois::compute_ois_swap_dates` | **完了**。QuantLib 1.42 `MakeOIS` と 13,520 件で照合し、成功 13,514 件は完全一致、失敗 6 件も同じ（`docs/spec_ois_swap.md` 7 章）。コミット済み（`d8c5ba7`） |
| R の関数 `compute_ois_swap_dates(index, evaluation_date, tenor, settlement_days = 2L, on_error)` | **実装とテストは完了、コミット済み（2026-11-18）**。ラッパーは `crates/riemann-r/src/rust/src/lib.rs`、R 関数は `R/ois_swaps.R`、ヘルプは `man/compute_ois_swap_dates.Rd`、テストは `tests/testthat/test-ois-swap-dates.R`。R のテスト全体は 156 件成功、Rust は 278 件成功、`cargo clippy --all-targets` は通る |
| Carmine Studio の置き換え | **未了**。`../carmine-studio/server/R/tona_ois_model.R` の `TONA_SWAP_DATES` を `compute_ois_swap_dates("tonar", evaluation_date, "7Y")` に置き換える。結果は同じ日付（開始 2026-09-14、満期 2033-09-14）のはず。置き換えたら `reconciliation/RECONCILIATION.md` 6 章 #1 を「解消」に更新する（クライアントの会話で、頼まれたときだけコミット） |
| 版と公開 | **未了**。`DESCRIPTION` を 0.3.5 にして、下の「配布」の手順で公開する。公開してから、Carmine Studio は `stopifnot(packageVersion("RiemannFinMath") >= "0.3.5")` を足す |

**注意**：

- 既定のライブラリーの `RiemannFinMath` は 0.3.2 のまま（別の R が DLL を使っている）。新しい関数を試すには、`R CMD INSTALL -l <別ライブラリー> crates/riemann-r` に入れる（今回は `C:/rfmdev/lib`）。
- `crates/riemann-r`（ワークスペースの外）の `cargo clippy` は、もとから 13 件のエラーが出る（引数の多さ、不要な型変換など）。今回の追加分ではない。プロジェクトの検査は、ルートの `cargo clippy --all-targets -- -D warnings`。
- 1 日の OIS が QuantLib で縮退して失敗するケース（年末年始）は、`compute_ois_swap_dates` は日付を返し、`create_ois_swap` が同じく失敗する（テストで確認済み）。

**決定（2026-11-18、ユーザー）：Windows で Rust を不要にする入れ方は、`install.packages(repos = "https://besuke.github.io/Riemann")` に一本化する**（Rtools も要らない。すでに動く）。`remotes::install_github()` は常にソースからビルドするので Rust と Rtools が要り、これは変えない（今の `main` で動作確認済み。Rust がなければ `configure.win` が上の入れ方を案内して止まる）。

検討したが採用しなかった案：ビルド済みの静的ライブラリー `libriemann_r.a`（約 35 MB）を Releases に置き、`configure.win` が `RemoteSha` と SHA-256 の一致を確かめて取得して cargo を飛ばす案。リンクのために Rtools は要るままで、タグごとの添付も要るので、(b) で足りるとしてやめた。必要になったら、版・コミット・SHA-256 が一致するときだけ使い、不一致なら cargo にもどす形にする。

### 完了したもの

**実装と照合**（詳細と件数は各仕様書の照合結果の節）：

- P1〜P15、P11-1〜P11-6、TRS、IRRBB は実装と照合が済んでいる。
- 残っている主なもの：
  - P3：±8% で振れる入力の 3 カーブが、入力気配の再現基準に届かない（`spec_ois.md` 10.3 節）。GaussQuant・Coral Book との照合（仕様書 7 章）は未実施。
  - P9：LMM 全体は未実装（P11-6 は Caplet と相関の例の再現だけ）。
  - P10：スマイルの下での経路依存の条項、相関の推定、FX と金利の同時校正は未対応（`spec_prdc.md`）。
  - P14：DRC は関係を整理しただけで、基準本文の確認と実装は未了。
  - P1：一致は QuantLib とのもので、実際の市場休日は検証していない。
- QuantLib Guide と公式 Examples：`quantlib_guide_inventory.md` 4 章の優先順位 1〜6 は実装済み。残りは「後回し」の項目、公式 Examples のサンプル追加、比較資料の全体の再描画。サンプルは `crates/riemann-r/inst/examples/quantlib_guide/` と `quantlib_examples/`。
- L1〜L3（TypeScript の Node 版、ブラウザー版、Excel）は未着手。

**配布（`docs/spec_r.md` 8 章）**：

- 最新は 0.3.4（2026-11-18）。
  - **Windows の入れ方（Rust なし）**：`install.packages("RiemannFinMath", repos = c("https://besuke.github.io/Riemann", "https://cloud.r-project.org"))`。公開した URL から、Rust を外した PATH で導入と smoke test 34 項目を確認済み（`dist/smoke_from_pages_0.3.4.log`）。
  - `remotes::install_github("besuke/Riemann", subdir = "crates/riemann-r")` は、`configure` が `RemoteSha` のコミットの中核を GitHub から取る（`e12a68a`）。Rust と Rtools が要る。2026-11-18 に今の `main` で再確認済み（34 項目成功）。`subdir` なしは 404。Rust がなければ `configure.win` が上の入れ方を示して止まる。
  - Windows zip と tar.gz は GitHub Releases の `v0.3.4` にもある。
- **`gh-pages` ブランチ**は R リポジトリ（zip、tar.gz、`PACKAGES` 索引）だけを持つ配布専用で、`main` とは別。GitHub Pages が https://besuke.github.io/Riemann/ で公開する。Riemann の作業フォルダーには取り込まず、一時フォルダーから push する。
  - **版を上げるとき**：(1) `DESCRIPTION` の版を変える。(2) zip を `R CMD INSTALL --build -l <別ライブラリー>`、tar.gz を `tools/package/build_source_package.R` で作る。(3) `Rscript tools/package/build_binary_repo.R <zip> <tar.gz> <フォルダー>` で索引を更新する（古い版は残り、`install.packages` は新しい版を選ぶ）。(4) そのフォルダーを `gh-pages` に push する（Pages の設定は済み）。(5) 公開 URL から導入して smoke test を実行する。(6) Releases に `vX.Y.Z` を作る。
  - zip と tar.gz は同じ版にする。そろわないと Windows が新しいソースをビルドしようとする。ソースを置かないと、Windows の R は「このパッケージはない」と判断して失敗する。
  - 既定のライブラリーに別の R（Positron など）が `RiemannFinMath.dll` を使っていると、`R CMD INSTALL --build` が失敗する。別のライブラリーに作り、その R は終了させない。
- tar.gz の `R CMD check` とソース導入は 0.3.0 以降、アプリ制御に止められて未完了。Windows 以外は未確認。
- 配布ファイルの記録は `dist/MANIFEST.txt`（`dist/` は git に入れない）。クライアント向けの手順は `docs/client_guide.md`。

### 未解決の点

- 許容誤差内の微小な差：P2-2 は R と Rust の数学ライブラリーの差を確かめた。P2-3・P2-4 も同じ原因と考えているが、未確認の推定。
- カーブ（P2-3）について、v1.42 と `70f240f` の差は確認していない。
- `docs/library_comparison.md` の先頭の空行 1 行は由来不明で、扱いはユーザーに確認していない。
- Infrago Quant に別の会話の未コミットの変更がある（2026-11-18 に 34 件）。頼まれない限り触らない。

### その他の候補（ユーザーの指示待ち）

- クライアントの要望（「クライアントのニーズ」の表）。
- 比較資料の判定できない項目（PRDC、第 5・13 章）を減らすか。
- 配布の Windows 以外の確認。

### 別の PC でダウンロードして続ける

1. 取得：`git clone https://github.com/besuke/Riemann.git`（または `git pull`）。RStudio では `Riemann.Rproj` を開く。
2. 準備：R 4.6、Rtools 4.5、Rust（`rustup target add x86_64-pc-windows-gnu`）、Claude Code。R パッケージだけ使うなら、上の `install.packages(...)` の 1 行で足りる。
3. 起動：`Riemann-Claude.bat`（または `Riemann-Claude.ps1`）を実行する。この PC に会話の履歴があれば続きから、なければ新しい会話で始まる。新しい会話では、**このファイル（CLAUDE.md）の「まず読む」から続ける**。
4. 会話の履歴と記憶は PC ごとにあり、git には入らない。引継ぎはこのファイルが担う。作業を切るときは、ここを更新してコミットする。
5. クライアント（`../carmine-studio`、`../InfragoQuant`、`../crimson-proof`）と参照元（`../coral-book`、`../GaussQuant`）は、同じ親フォルダーに clone する。

## QuantLib との比較資料（`comparison/`）

Coral Book の入力と計算例で、RiemannFinMath を基準と比べる英語の資料（文書に日本語を入れない）。

- **基準**：カレンダーは QuantLib C++ v1.42（`tests/golden/calendar_all_*.csv`）、それ以外は Coral Book の計算結果（`comparison/baseline/lagrange_values.csv`、作り直しは `comparison/baseline/README.md`）。QuantLib R パッケージなどを直接呼ぶ行は参考（REF）で判定しない。同じ計算でないものは N/C。
- **判定**：絶対誤差 1e-4、表示は小数 4 桁、判定は丸める前、単位を明記。この資料だけの基準。
- **出力**：`comparison/` で `quarto render --to html` → `_book/index.html`（git に入れない）。丸める前の結果は `comparison/results/*.csv`（git に入れる）。成功の判定には、「Windows の環境」の quarto の 2 条件に加えて、結果の CSV が 14 章分あることを確かめる。リークチェックは描画とは別に実行して報告する。章だけを確かめるときは `Rscript R/check_chapter.R <章>`。
- **最後の結果**（2026-11-02、`6a5d9d7`）：PASS 386、FAIL 0、N/C 40、REF 402。未比較は `chapters/appendix_c_gaps.qmd`。
- **市場データ**：2026-10-06 の OIS 気配と受渡適格銘柄の価格は、描画時に Coral Book から読むだけで、保存も表示もしない。

## Windows の環境（全リポジトリ共通）

- **R** は PATH にない：`"C:/Program Files/R/R-4.6.0/bin/Rscript.exe"`。R の中のパスは `C:/...`（`/c/...` は使えない）。`Rscript -e` は引用符で失敗しやすいので、スクリプトファイルに書いて実行する。
- パッケージの導入は `R CMD INSTALL`（`devtools::install` は pak の DLL がアプリ制御で止められる）。
- **RiemannFinMath の入れ方**：
  - 開発中：ルートで `R CMD INSTALL crates/riemann-r`。Rust の `x86_64-pc-windows-gnu` と Rtools が必要。
  - 配布版：GitHub Releases の Windows zip（Rust 不要）、または `remotes::install_github("besuke/Riemann", subdir = "crates/riemann-r")`（`docs/spec_r.md` 8.7 節）。
  - パスが長いとビルドが `LNK1104` で失敗する。短いパスで行う。
- **Smart App Control** が、新しくビルドした未署名の実行ファイルを止めることがある。
  - 症状：`os error 4551`、`Permission denied`。
  - 記録：イベントログ `Microsoft-Windows-CodeIntegrity/Operational` の ID 3033・3077。
  - 同じファイルでも判定が変わる。回避しない。
- bash のヒアドキュメントでは `\\` が `\` になる（2026-11-18 にも R の `\\*` が崩れた）。バックスラッシュを含むコードは Write でファイルに書く。CRLF のファイルは MSYS の sed で CR が消える。
- **`quarto render` の中断**：途中の章で `WARN: Error encountered when rendering files` だけを出して止まることがある。比較資料でも Crimson Proof でも起き、原因は未確認で、アプリ制御とも断定しない。
  - 成功の判定：`quarto render` 自身の終了コードが 0、かつログに `WARN` がない。
  - 失敗したら、全体の再実行を繰り返す前に次を行う：
    1. ログを日時つきで保存する。
    2. 終了コードと、最後の進捗行（章名と直前のチャンク名）を記録する。
    3. その章だけを `quarto render <章>.qmd --log-level debug` で描画する。
    4. そのうえで全体を再実行する。
- 会話の再開：リポジトリ直下の `Riemann-Claude.bat`（または `.ps1`）が、そのフォルダーで `claude --continue` を起動する（会話がなければ新しい会話）。デスクトップの `Riemann-Claude.bat` は旧パス固定の複製で、リポジトリのものを使う。

## Riemann 固有の環境

- テスト：`cargo test --release`。clippy：`cargo clippy --all-targets -- -D warnings`。新しいテストの実行ファイルで LNK1104 が一時的に出ることがある。もう一度実行すれば通る。
- `cargo fmt --check` は既存のコードで差が出る。新しいファイルと変更した箇所だけ整形する。`rustfmt` にファイルを渡すと `mod` 先まで整形するので、`--config skip_children=true` を付ける。
- 照合用の値は、ルートから `tools/golden/*.R` を実行して `tests/golden/` に書き出す。仕様が変わらなければ作り直さない。`tests/golden` の CSV は R が CRLF で書き、git で LF になる（`.gitattributes`）。
- カレンダーの規則は `tools/golden/port_calendars.R` が生成する（`rules/` の `special.rs` 以外）。生成後に rustfmt をかける。C++ のビルドは Rtools の g++ を PATH に入れて、bash から実行する。
- 検証レポート：`tools/verify/verify_riemann.qmd`。アプリ制御で止まった例だけ、環境変数 `RIEMANN_VERIFY_SKIP_EXAMPLES`（例：`verify_bonds`）で飛ばす。その経路は「not run」と表示される。
- R パッケージ：テストは `crates/riemann-r/tests/testthat`。このビルド（windows-gnu）は MSVC の `cargo test` と数学ライブラリーが違い、最下位ビットの差が出る（`docs/spec_ois.md` 10.6 節）。
- 配布ファイル：
  - ソースパッケージ：`tools/package/build_source_package.R`
  - zip：`R CMD INSTALL --build crates/riemann-r`
  - 導入の確認：`tools/package/smoke_test.R <版> <ライブラリー>`。`env -i` で環境変数を全部消すと終了時に落ちるが、`tibble` 側の問題（8.4 節）。PATH から Rust を外すだけで確かめる。
- QuantLib のソース（v1.42 の確認用）は手元にない。必要なら一時フォルダーに clone し直す。
