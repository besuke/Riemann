# クライアント向けの手引き：rmql の入れ方と使い方

対象：rmql（計算サーバー）を呼ぶクライアント（Carmine Studio、Infrago Quant、Crimson Proof、今後のもの）。クライアントは金融計算を自分で書かず、R から `rmql` を呼ぶ。2026-11-18 作成。入れ方の確認記録は `docs/spec_r.md` 8.7 節。

## 1. 入れる（Windows、R 4.6）：Rust も Rtools も不要

```r
install.packages(
  "rmql",
  repos = c("https://besuke.github.io/rmql", "https://cloud.r-project.org")
)
```

- バイナリと依存パッケージ（`purrr`、`tibble` とその依存）が自動で入る。
- **`remotes::install_github()` は使わない。** 常にソースからビルドするので Rust が要る。
- 公開しているのは R 4.6 用のバイナリだけ。他の R の版では、ソースからのビルド（Rust が要る）になる。確認していない。
- Windows 以外（Linux、macOS）は未確認。同じ 1 行でソースからビルドされ、Rust が要るはずだが、実行していない。

zip を直接入れる場合（ネットワークが GitHub Pages に届かないときなど）：

```r
install.packages(c("purrr", "tibble"))
install.packages(
  "https://github.com/besuke/rmql/releases/download/v0.3.4/rmql_0.3.4.zip",
  repos = NULL, type = "binary"
)
```

## 2. 入れ替える（版を上げる）

1. **その DLL を使っている R のプロセスを先に止める**。Infrago Quant の R API（8790）、Carmine Studio の R API（8787）、Positron や RStudio の R セッション。止めないと `cannot remove earlier installation, is it in use?` で失敗する。他の人のセッションは、こちらから終了させない。
2. 上の `install.packages(...)` をもう一度実行する。
3. 版を確かめる：`packageVersion("rmql")`。

## 3. 版を確かめる・固定する

クライアントの起動時（R API の先頭や QMD の `setup`）に、必要な版を確かめる。

```r
stopifnot(packageVersion("rmql") >= "0.3.4")
```

- 照合を固定するときは、版と配布ファイルの SHA-256（`dist/MANIFEST.txt`、Releases の説明）を記録する。0.3.4 の zip は `b2e2ed00e8fe428b9716dee6c6b96f37f1e9e9ad38d513d34622d3b93a1599ab`。
- 古い版を入れ直したいときは、Releases（https://github.com/besuke/rmql/releases）の zip を指定する。

## 4. 呼び方のきまり

- **公開関数だけを呼ぶ**。`rmql::` を付けるか、`library(rmql)` する。内部の関数（`:::`）は使わない。
- **関数名は動詞から始まる**（`create_`、`generate_`、`get_`、`compute_`、`bootstrap_`、`calibrate_`、`is_`、`has_`、`with_`）。使える関数は `?rmql::rmql` と、各関数のヘルプ（`?compute_bond_values`）にある。
- **使用例**：`system.file("examples", "quickstart.R", package = "rmql")`（全体）、`inst/examples/quantlib_guide/`、`inst/examples/quantlib_examples/`。
- **日付**は R の `Date`（または `"2026-11-18"` の文字列）。評価日は引数で渡す（グローバルな評価日はない）。
- **エラー**：入力の不正は R のエラーとして返る（Rust の panic で R が落ちることはない。`docs/spec_r.md` 4 章）。クライアントは、不正な入力を自分の API で先に検査して 400 を返す。計算の失敗は、メッセージをそのまま見せずに、利用者に分かる日本語にする。
- **並列**：一括評価の関数（`compute_bond_values_batch` など）は Rust のスレッドで並列に動く。R の `furrr` と二重にしない。スレッド数は引数 `threads` で決める（`compute_bond_values_batch(bonds, threads = 2L)`。既定の 0 は自動）。結果と順序はスレッド数によらない。
- **計算し直さない**：価格・カーブ・感応度を、クライアント側で書き直さない（TypeScript でも R でも）。足りない計算は rmql に足す（5 章）。

## 5. 足りない機能を頼む

1. 足りない計算、入力、期待する値（照合の基準：Coral Book、独立な計算、QuantLib のどれか）を、rmql の会話（デスクトップの `rmql-Claude.bat`）に伝える。
2. rmql 側で、照合 → 実装 → テスト → 仕様書 → 配布の順に作る（`CLAUDE.md` の「作業のきまり」）。
3. 新しい版を `gh-pages` に公開したら、クライアントは「2. 入れ替える」で更新し、必要な版を「3.」の `stopifnot` に書く。

現在の未対応の要望は `CLAUDE.md` の「クライアントのニーズ」の表にある（OIS スワップを期間 `7Y` で作る、元本リセット型 CCS、ブラウザー版など）。

## 6. 公開関数を変えるときの約束（rmql 側）

- 公開関数の名前と引数を、クライアントに知らせずに変えない。変える前に、3 つのクライアントの `rmql::` の呼び出しを grep で確かめる（2026-11-18 時点：Carmine Studio 73 関数、Infrago Quant 50 関数、Crimson Proof 7 関数。いずれも現在の公開関数にある）。
- 変えたときは `docs/naming_migration.md` に旧名と新名を書き、版を上げる。
