# Riemann

Rust で一から作る金融計算ライブラリー（計画段階）。主な利用言語は R と TypeScript（Node.js とブラウザー）とする。

- **オブジェクト指向からの脱却**：継承・仮想関数・状態を持つオブジェクト・オブザーバーをやめ、値（データ）と関数だけで組み立てる。
- **テンプレートからの脱却**：利用者が選ぶ金融上の選択肢を型の引数で表すのをやめ、実行時の値として渡す。ジェネリクスの全面禁止ではなく、自動微分など内部の数値計算に必要な型の抽象化は使う。
- **Rust の高速性とスレッド安全性を活かす**：ネイティブの速度で計算し、言語が保証するスレッド安全性の上で並列に評価する。計算の中核を一つにして、R と TypeScript から、計算ごとに定めた許容誤差の範囲で同じ結果が得られるようにする。

評価日などのグローバル状態も持たず、評価に必要なものはすべて引数で渡す。書き換えない値と副作用のない関数で作るので、スレッド間の共有も言語間の受け渡しも安全に行える。

- 設計仕様書（草案）：[docs/specification.md](docs/specification.md)
- 参考資料（QuantLib と finmath-lib の比較）：[docs/library_comparison.md](docs/library_comparison.md)

現在はまだコードはない。

## ライセンス

次のどちらかを選んで利用できる（MIT と Apache-2.0 の併用）。

- MIT License（[LICENSE-MIT](LICENSE-MIT)）
- Apache License 2.0（[LICENSE-APACHE](LICENSE-APACHE)）

明示的な断りがない限り、本リポジトリへの貢献は、Apache License 2.0 の定義に従い、追加の条件なしに上記の併用ライセンスで提供されたものとする。

## 参考にしたライブラリー

- [QuantLib](https://github.com/lballabio/QuantLib)（修正 BSD ライセンス）
- [finmath-lib](https://github.com/finmath/finmath-lib)（Apache License 2.0）

どちらのコードも移植せず、数式・仕様・規約の参考にしている。移植する場合の扱いは仕様書の「ライセンス」を参照。
