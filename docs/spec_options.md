# オプションの仕様（P6）

株のオプションと、フォワードのオプションである。実装は `crates/riemann/src/options.rs`、`crates/riemann/src/math/normal.rs`、`crates/riemann/src/math/random.rs`、R の入口は `crates/riemann-r/R/options.R` である。

## 1. 対象

| 計算 | 関数（R） | 手法 | QuantLib での対応 |
|---|---|---|---|
| ヨーロピアン | `compute_european_option_value` | Black–Scholes–Merton | `AnalyticEuropeanEngine` |
| ヨーロピアン、アメリカン | `compute_binomial_option_value` | Cox–Ross–Rubinstein の二項木 | `BinomialCRRVanillaEngine` |
| アメリカン（近似式） | `compute_american_approximation_value` | Barone-Adesi–Whaley、Bjerksund–Stensland（1993） | `BaroneAdesiWhaleyApproximationEngine`、`BjerksundStenslandApproximationEngine` |
| バリア（連続監視） | `compute_barrier_option_value` | Reiner–Rubinstein の解析解 | `AnalyticBarrierEngine` |
| フォワードのオプション | `compute_forward_option_value` | Black、Bachelier | `blackFormula`、`bachelierBlackFormula` |
| インプライド・ボラティリティ | `compute_implied_volatility` | Brent（1e-7〜4） | `impliedVolatility` |
| モンテカルロ | `compute_monte_carlo_european_value` | 満期まで 1 ステップ、対称変数 | なし（解析値と比べる） |

入力は数値である。金利と配当利回りは一定の連続複利で、時間は年である。カーブと日付で与えた場合の QuantLib の計算と同じ値になるのは、`時間 = 日数 / 365` のフラットなカーブの場合である。ボラティリティ 0 と時間 0 は、QuantLib では計算できるが、ここではエラーとする。

## 2. グリークス

解析式を書かず、価格の関数を自動微分する。QuantLib の解析式との一致が、微分の検証になる。

| 列 | 意味 |
|---|---|
| `delta`、`gamma` | スポットについての 1 階、2 階の微分 |
| `vega`、`rho`、`dividend_rho` | ボラティリティ、金利、配当利回りについての微分 |
| `theta` | 時間についての微分の符号を変えた値（年あたり） |
| `strike_sensitivity` | 行使価格についての微分 |

**二項木の扱い**：木の価格はスポットについて区分的に線形なので、木そのものの 2 階微分は 0 になる。そこでデルタとガンマは、QuantLib と同じく木の最初の段の値から求める。他の感応度は、木の全ノードを記録して自動微分する。QuantLib の木のセータは Black–Scholes 方程式から求めるので、Riemann の `theta` とは定義が違う。

**モンテカルロのガンマ**：ペイオフの折れ目のパスごとの 2 階微分は 0 である。折れ目を残したまま微分しても正しいガンマにならないので、`smoothing` を与えない場合は `NA` を返す。与えた場合は `(x + sqrt(x^2 + w^2)) / 2` を 2 階微分する。LagrangeFinance III-5 と同じ考え方で、平滑化の偏りを含む。

## 3. 乱数

乱数はカウンター方式である。あるパス・次元の乱数は、種、パス番号、次元の番号だけで決まる（SplitMix64 の混合関数）。スレッド数と計算の順序に依存しない。正規乱数は分布関数の逆関数（QuantLib `InverseCumulativeNormal`、相対誤差 1.15e-9 未満）で作る。

Sobol 列と Longstaff–Schwartz 法（LagrangeFinance II-4.6）は P9 で扱う。

## 4. 照合結果【実行照合済み】（2026-11-01）

基準値は QuantLib R パッケージ Version 1.42 で、入力はすべて合成である（`tools/golden/p6_options.R`）。差は `|a - b| / (1 + |b|)` で表す。

| 対象 | 件数 | 最大の差 |
|---|---|---|
| ヨーロピアンの価格と 7 種のグリークス | 300 | 1.6e-14 |
| バリアの価格（4 種、リベートあり・なし） | 320 | 1.6e-14 |
| Black、Bachelier | 400 | 2.3e-15 |
| 二項木の価格、デルタ、ガンマ（5 章の 11 件を除く） | 189 | 2.8e-11 |
| インプライド・ボラティリティ | 100 | 2.2e-11 |

モンテカルロは 20 万パスで、解析値に対して価格は標準誤差の 5 倍以内、1 階の感応度は 1.5% 以内である。スレッド数 1、2、4 の結果は完全に一致する。

アメリカンの正しさは QuantLib との一致で確かめたものである。解析解はない。市場価格との照合もしていない。

## 5. QuantLib との差の原因【実行照合済み】

差の原因は二つあり、どちらも実験で確かめた。Riemann は QuantLib に合わせず、本来の計算のままとする。

**(1) 数値的なドリフト（1e-10 程度の差）**：`GeneralizedBlackScholesProcess::drift` は、ドリフトを `t` から `t + 0.0001` のフォワードレートで求める。一定の金利でも、割引係数の比の対数を 0.0001 で割るので 1e-12 程度の丸めが入り、木の確率に効く。同じ丸めを Riemann の木に一時的に入れると、最大の差は 2.8e-11 から 1.3e-14 に下がった。

**(2) 時間格子の丸め（QuantLib の不具合）**：QuantLib の時間格子の最後の時刻は `dt * n` で計算される。これが丸めで満期 `T` より小さくなると、満期の時刻が `stoppingTimes_[1]` を超えるので、`DiscretizedVanillaOption` は満期の行使を行わず、値が 0 のまま後退計算が始まる。アメリカンの価格は本来より低くなる。

- 合成データのアメリカン 151 件のうち、この条件に当たるのは 11 件である。うち 10 件で差があり、最大は 1.44 である。
- 満期のペイオフを 0 にした木を別に計算すると、11 件すべてが QuantLib と差 3.2e-10 以内で一致した。残りの 140 件は、普通の木と差 4.6e-10 以内で一致した。

GaussQuant の `make_american_option_GQL` はこのエンジンを使うので、同じ条件で同じ影響を受ける。これは GaussQuant を変更せず、ここに記録するだけとする。

## 6. アメリカンの近似式（2026-11-04）【ソース確認・実行照合済み】

実装は `crates/riemann/src/american_approximations.rs`。QuantLib v1.42 の `baroneadesiwhaleyengine.cpp` と `bjerksundstenslandengine.cpp` の価格の計算を移した。グリークスは移さず、価格を自動微分する（2 章と同じ）。

- **Barone-Adesi–Whaley**：臨界価格を QuantLib と同じ種と Newton 反復（行使価格に対する相対誤差 1e-6 まで）で求め、二次近似の項を足す。配当がない（割引係数が 1 以上の）コールはヨーロピアンの値。負の金利（無リスクの割引係数が 1 より大きい）はエラー（QuantLib と同じ）。反復が正でない価格に達した場合もエラーにする（QuantLib は `blackFormula` で失敗する）。自動微分は Newton 反復の各段を通すので、微分には反復の打ち切り誤差が残る。QuantLib はこの場合グリークスを返さない。
- **Bjerksund–Stensland**：1993 年の一定の境界の近似。プットはプット・コールの対称性（スポットと行使価格、金利と配当利回りを入れ替える）でコールに直す。境界 `I` 以上は即時行使、`ln(I / F) / σ√T > 12.5` はヨーロピアン、近似値がヨーロピアンより小さい場合もヨーロピアンとし（QuantLib の `americanCallApproximation` の最後）、最後に即時行使の値と比べる。`r < q < 0` のコールはエラー（QuantLib と同じ）。

**照合**（2026-11-04、QuantLib R パッケージ 1.42、`tools/golden/p11_american_approximations.R`、`crates/riemann/tests/golden_american_approximations.rs`）：2 手法 × コール・プット × スポット 5 × 金利 4（-1%〜8%）× 配当利回り 3 × ボラティリティ 2 × 期間 3 の 1440 件。値のある 1270 件は相対差 2.8e-14 以内で一致した。QuantLib がエラーにする 155 件（BAW の負の金利、臨界価格の反復の失敗）は、Riemann もエラーにする。

**QuantLib との違い（意図したもの）**：QuantLib の Bjerksund–Stensland が NaN を返す 15 件（`r = -1%`、`q = 0` のコール。`beta` の平方根の中が負になる）を、Riemann はエラーにする。

**LagrangeFinance II-4.5** のアメリカン・プット（スポット 100、行使価格 100、金利 5%、配当 0、ボラティリティ 20%、2024-03-01 から 2025-03-03）：BAW 6.110018383735435（基準値との差 -3.6e-15）、Bjerksund–Stensland 5.995058742279532（差 -1.4e-14）。

**公式 Examples の EquityOption**：`inst/examples/quantlib_examples/equity_option.R`。Black–Scholes、二つの近似式、801 段の CRR 木（ヨーロピアン、アメリカン）が、手元の QuantLib 1.42 と差 7.8e-12 以内で一致した（木の差は 5 章の (1)）。

**未対応**：QD+（`QdFpAmericanEngine`）、Bjerksund–Stensland の 2002 年版、近似式のバミューダン。
