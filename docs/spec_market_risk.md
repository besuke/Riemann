# P13 市場VaR、期待ショートフォールとバックテスト

- 実装と照合：2026-11-15（P12 の `docs/spec_portfolio.md` 4 章の後続）
- 対象：LagrangeFinance I-9「市場リスクと市場VaR」（`part1_empirical/ch09_market_risk_var.qmd`、コミット `b9ddc2a`）
- 比べる相手：原稿の計算を R でそのまま書いたもの（`quantile()`、`qnorm()`、`dnorm()`）、原稿のモンテカルロのチャンク（furrr）、Kupiec・Christoffersen の検定を R で書いたもの（`pchisq()`）、バーゼルの信号の区分（`pbinom()`）
- 再実行：`Rscript tools/golden/p13_market_risk.R`（参照値 `tests/golden/market_risk.csv`）、`cargo test --release --test golden_market_risk`

## 1. 原稿の計算【ソース確認】

| 節 | 計算 |
|---|---|
| I-9.3 | 最初のリバランス年度の時価総額上位 20 銘柄の等ウエイト。各月、データのある銘柄のリターンの平均（`mean(return, na.rm = TRUE)`）。損失 `L_t = −R_t`、71 か月 |
| I-9.4 | 分散共分散法：`−μ + qnorm(α) σ`（σ は標本標準偏差） |
| I-9.5 | ヒストリカル法：損失の `quantile(type = 7)` |
| I-9.6 | モンテカルロ法：`N(μ, σ²)` を 20 バッチ × 5,000 本（furrr、シード 20260918）、損失の `quantile()` |
| I-9.8 | ES：VaR 以上の損失の平均 |
| I-9.9 | リスク寄与：`w_i (Σw)_i / σ_p`（P12 の `compute_portfolio_risk()` にある） |
| I-9.10 | バックテスト：全期間のヒストリカル VaR を超えた回数。原稿は、ローリング・ウィンドウへの拡張が重要と述べる |
| I-9.1・9.11 | FRTB は背景の説明。標準的方式の資本計算は扱わない |

## 2. 実装【設計案から実装済み】

`crates/riemann/src/market_risk.rs`、R は `crates/riemann-r/R/market_risk.R`。

| R の関数 | 内容 |
|---|---|
| `compute_loss_risk(losses, confidence_level)` | 正規の VaR・ES、ヒストリカル VaR（type 7 と type 1）、ES 二つ（VaR 以上の平均＝原稿、Rockafellar–Uryasev）。信頼水準はベクトルで渡せる |
| `compute_monte_carlo_risk(weights, mean, covariance, ...)` | 多変量正規のシナリオでポートフォリオの VaR・ES。乱数はカウンター方式（シード・シナリオ・資産だけで決まる）なので、スレッド数によらず同じ。標準誤差はバッチ平均（既定 20 バッチ、原稿と同じ分け方） |
| `compute_var_backtest(losses, var, confidence_level)` | 超過回数、Kupiec の POF 検定、Christoffersen の独立性検定と条件付きカバレッジ検定（尤度比と p 値。自由度 1・1・2 のカイ二乗の上側確率は正規分布関数と指数関数で閉じた形） |
| `compute_rolling_var(losses, window, ...)` | 各時点の直前 `window` 個の損失から VaR（ヒストリカル type 7 または正規） |
| `compute_traffic_light(exceptions, observations, confidence_level)` | バーゼルの信号の区分：二項分布の累積確率が 95% 未満はグリーン、99.99% 未満はイエロー、それ以上はレッド |

正規分布の分位点は、QuantLib の近似にニュートン法を 2 回加えた `math::normal::compute_normal_quantile()`（P12 で `portfolio.rs` に置いたものを移した）。

## 3. 照合の結果【実行照合済み】

照合日 2026-11-15。R 4.6.0、furrr 0.4.0、future 1.75.0（`tests/golden/market_risk.csv` の先頭に記録）。

| 比べたもの | 判定 |
|---|---|
| 原稿の 71 か月の損失：正規の VaR・ES、ヒストリカル VaR（type 7・1）、ES 二つ（信頼水準 95%・97.5%・99%） | 相対 1e-13 以内 |
| 原稿のバックテスト（全期間の VaR、95%）：観測 71、超過 4、遷移の数、検定統計量と p 値 | 個数は一致、統計量と p 値は相対 1e-12 以内 |
| ローリング（窓 36、ヒストリカルと正規）：各時点の VaR 35 個、検定 | VaR は相対 1e-13 以内、検定は上と同じ |
| バーゼルの信号（250 日、99%、超過 0〜15 回）：累積確率と区分 | 累積確率は相対 1e-12 以内、区分は一致（グリーン 0〜4、イエロー 5〜9、レッド 10〜） |
| モンテカルロ（10 万本、シード 20260918） | 下の表。乱数の系列が R と違うので、標準誤差で判定 |

原稿の主な値（95%）：正規 VaR 0.0648、ヒストリカル VaR 0.0660、ES（原稿の定義）0.0870、Rockafellar–Uryasev の ES 0.0894、超過 4 回（期待 3.55 回）、Kupiec の p 値 0.810。ローリング（窓 36）では 35 か月で超過 4 回、p 値 0.132。

| モンテカルロ | RiemannFinMath | 比べる値 | 標準誤差 |
|---|---|---|---|
| 1 変量（原稿と同じ μ・σ）の VaR 95% | 0.064790 | 正規の式 0.064805、原稿の furrr 0.064715 | 0.00031 |
| I-8 の 20 銘柄・等ウエイトの VaR 95% | 0.060713 | 正規の式 0.060570 | 0.00029 |
| 同 ES 95% | 0.079509 | 正規の式 0.079457 | 0.00025 |
| 同 VaR 99% | 0.091816 | 正規の式 0.091373 | 0.00044 |
| 同 ES 99% | 0.106253 | 正規の式 0.106690 | 0.00047 |

判定は、正規の式との差が標準誤差の 4 倍以内、原稿のモンテカルロとの差が標準誤差の 4√2 倍以内（独立な二つの推定量の差）。スレッド数 1 と 4 で結果が同じであることは別のテストで確かめた。

## 4. 未対応

- FRTB の内部モデル方式の ES（97.5%、流動性ホライズンによる合成）と標準的方式の資本計算。原稿も扱わない。信頼水準 97.5% の ES 自体は `compute_loss_risk()` で計算できる
- 正規以外のモンテカルロ（t 分布、ヒストリカル・ブートストラップ）、フィルタード・ヒストリカル法
- ES のバックテスト（Acerbi–Székely など）
