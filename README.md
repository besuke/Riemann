# Riemann

Rust で一から作る金融計算ライブラリー（計画段階）。R からも呼べるようにする。

- 設計仕様書（草案）：[docs/specification.md](docs/specification.md)
- QuantLib と finmath-lib の比較：[docs/library_comparison.md](docs/library_comparison.md)

現在はまだコードはない。

## ライセンス

次のどちらかを選んで利用できる（MIT と Apache-2.0 の併用）。

- MIT License（[LICENSE-MIT](LICENSE-MIT)）
- Apache License 2.0（[LICENSE-APACHE](LICENSE-APACHE)）

明示的な断りがない限り、本リポジトリへの貢献は、Apache License 2.0 の定義に従い、追加の条件なしに上記の併用ライセンスで提供されたものとする。

## 参考にしたライブラリー

- [QuantLib](https://github.com/lballabio/QuantLib)（修正 BSD ライセンス）
- [finmath-lib](https://github.com/finmath/finmath-lib)（Apache License 2.0）

どちらのコードも移植せず、設計と仕様の参考にしている。移植する場合の扱いは仕様書の「ライセンスと帰属表示」を参照。
