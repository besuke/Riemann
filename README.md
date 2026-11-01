# RiemannFinMath

Rust を中核とする金融計算ライブラリーである。副作用の局所化と参照透過性を重視し、評価日・市場データ・計算規約を明示的な引数として与える関数型の設計を採用する。金融上の選択肢は実行時のデータとして表現し、利用言語に特定の型構成や継承関係を要求しない。

Rust の Send・Sync トレイトと、副作用を抑制した設計を基盤として、データ競合を防止しながら並列に計算する。計算の正当性と浮動小数点演算の再現性は、言語による安全性の保証とは区別し、数値照合と並列実行テストで検証する。

計算機能は Rust の中核に集約し、R と TypeScript を主要な利用言語とする。言語間の接続は薄い層に限定する。WebAssembly を介したブラウザーと Excel の Office アドインへの展開、および並列処理と自動微分によるリスク感応度計算を開発目標とする。

## 現在の状態

- **実装済み**：日付、全カレンダー、固定利付債の価格・利回り・リスク、OIS カーブ（TONAR・€STR・SOFR・SONIA）、R パッケージ `RiemannFinMath`、債券と OIS カーブの並列バッチ、自動微分による債券の感応度、オプション（ヨーロピアン、アメリカン、バリア、Black、Bachelier、1 ステップの MC）
- **作業中**：R パッケージの配布整備
- **開発目標（未実装）**：TypeScript、WebAssembly、Excel の Office アドイン、多期間のモンテカルロ、金利モデル、CDS

リポジトリ名、Rust の crate 名（`riemann`、`riemann-r`）、API 名は `riemann` のままであり、いずれも RiemannFinMath を指す。

## 文書

- 設計仕様書（草案）：[docs/specification.md](docs/specification.md)
- R パッケージ：[crates/riemann-r/README.md](crates/riemann-r/README.md)、[docs/spec_r.md](docs/spec_r.md)
- 参考資料：[docs/library_comparison.md](docs/library_comparison.md)

## 著作権

Copyright (c) 2026 Eiichi Horigome。ただし、QuantLib から派生した部分の著作権は、[THIRD_PARTY_NOTICES](THIRD_PARTY_NOTICES) と各ファイルの先頭に記載した著作権者にある。

## ライセンス

次のどちらかを選んで利用できる（MIT と Apache-2.0 の併用）。

- MIT License（[LICENSE-MIT](LICENSE-MIT)）
- Apache License 2.0（[LICENSE-APACHE](LICENSE-APACHE)）

明示的な断りがない限り、本リポジトリへの貢献は、Apache License 2.0 の定義に従い、追加の条件なしに上記の併用ライセンスで提供されたものとする。

## 参考にしたライブラリー

- [QuantLib](https://github.com/lballabio/QuantLib)（修正 BSD ライセンス）
- [finmath-lib](https://github.com/finmath/finmath-lib)（Apache License 2.0）

数式・仕様・規約の参考にしている。QuantLib から移した部分の著作権表示とライセンスは [THIRD_PARTY_NOTICES](THIRD_PARTY_NOTICES) に載せている。扱いの決まりは仕様書の「ライセンス」を参照。
