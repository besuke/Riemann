# OIS スワップ本体の仕様（P11-2）

実装は `crates/riemann/src/ois_swap.rs`、R の入口は `crates/riemann-r/R/ois_swaps.R` である。LagrangeFinance II-3.2〜II-3.4 の計算を対象にする。

## 1. LagrangeFinance の計算【ソース確認】

GaussQuant の `trade_ois_swap_py_GQH` を通して、QuantLib の `MakeOIS` で作る。効果日と満期日を渡し、頻度は年 1 回、支払いの遅れは 0、固定レグの日数計算は指標のものである。

`fixed_schedule_tenor` と `floating_schedule_tenor` は、GaussQuant の中で `Schedule` の作成にだけ使われ、`MakeOIS` には渡らない。したがって II-3.3.1 の `floating_schedule_tenor = "1D"` は、スワップの値を変えない。

## 2. QuantLib v1.42 の仕様【ソース確認】

| 項目 | 仕様 | ソース |
|---|---|---|
| スケジュール | 固定・翌日物とも後ろ向き、修正後続、指標のカレンダー。開始日が月末なら月末 | `makeois.cpp` |
| 固定レグの支払日 | 修正後続（`floatingSchedule.businessDayConvention()`）で遅れを足す | `fixedvsfloatingswap.cpp` |
| 翌日物レグの支払日 | 後続で遅れを足す | `makeois.hpp`、`overnightindexedcoupon.cpp` |
| 日次の日付 | 開始日から終了日までの営業日（1 日刻み、後続、後ろ向き）。各日は Fixing 日で、指標の日数計算で年数にする | `overnightindexedcoupon.cpp` |
| 複利 | 評価日より前の Fixing は必須（なければ `Missing ... fixing`）。評価日当日は、あれば使い、なければ予測する。残りは `割引係数(残りの開始) / 割引係数(終了)` | `overnightindexedcouponpricer.cpp` |
| 利率 | `(複利 - 1) / 期間の年数 + スプレッド`。スプレッドは日次で複利にしない | 同上 |
| NPV | `DiscountingSwapEngine`。割引カーブの基準日で評価し、支払日がその日以前のフローを含めない | `discountingswapengine.cpp`、`cashflows.cpp` |
| 満期済み | 全フローの支払日が**評価日**以前なら、NPV と BPS は 0、公正レートと公正スプレッドは値なし | `swap.cpp`、`fixedvsfloatingswap.cpp` |
| 公正レート | `固定レート - NPV / (固定レグの BPS / 1bp)`。公正スプレッドも同じ形 | `fixedvsfloatingswap.cpp` |
| 符号 | ペイヤーは固定レグ -1、翌日物レグ +1。レシーバーはその逆 | `fixedvsfloatingswap.cpp` |

**基準日固定のカーブと評価日【ソース確認・実行照合済み】**：`IterativeBootstrap::calculate` は、カーブが moving のときだけ初期化し直す。基準日固定のカーブで評価日を動かすと、日付相対のヘルパーの日付は動き、カーブは解き直されるが、節点の日付は古いまま残る。QuantLib のコメントも「評価日が変わるなら、基準日固定のカーブに日付相対のヘルパーを使う意味はほとんどない」と書いている（`iterativebootstrap.hpp`）。

## 3. 実装

- `create_ois_swap`：`OisSwapSpec` から 2 つのレグを作る。値は不変である。
- `compute_ois_swap_value`：予測カーブと割引カーブ、評価日、Fixing を引数に取る。グローバルな状態はない。カーブの型について総称で、自動微分にも使う。
- `compute_ois_swap_daily_rates`：日次の明細。`compute_ois_swap_quote_sensitivity`：カーブの節点の自動微分と、陰関数定理で気配へ。カーブは予測と割引の両方に使う。
- R：`create_ois_swap`、`compute_ois_swap_value`、`compute_ois_swap_cashflows`、`compute_ois_swap_daily_rates`、`compute_ois_swap_quote_sensitivity`。予測カーブが別の指標ならエラーにする。

**QuantLib との違い**

| 項目 | QuantLib | Riemann |
|---|---|---|
| 評価日、Fixing | グローバル（`Settings`、指標名ごとの保管場所） | 引数 |
| 評価日を動かしたときのカーブ | 上のとおり解き直される | 値なので変わらない |
| 日次の日付 | 全日付か短縮版（telescopic）を選べる | 全日付 |
| 未対応 | － | ルックバック、ロックアウト、観測シフト、算術平均、日次複利のスプレッド |

## 4. 照合結果【実行照合済み】（2026-11-03）

| 対象 | 基準 | 件数 | 結果 | 許容誤差 |
|---|---|---|---|---|
| NPV、両レグの NPV と BPS、公正レート、公正スプレッド、全クーポンの額と支払日 | QuantLib R パッケージ 1.42（`tools/golden/p11_ois_swap.R`） | 300 件 | 292 件の評価で差は想定元本あたり最大 2.2e-16。支払日は完全一致 | 相対 1e-11 |
| Fixing の不足 | 同上 | 8 件 | 同じ件が同じ原因で失敗 | 文面の一部の一致 |
| 旧版 II-3.3.2 のスワップを QuantLib の節点で評価 | QuantLib R パッケージ（`p11_ois_swap_moved_date.R`） | 1 件 | NPV、両レグ、公正レートとも差 0。NPV は旧版と同じ 6.650498742801762 | 相対 1e-11 |
| 気配の感応度 | カーブを作り直す差分（Rust のテスト、R のテスト） | 4 気配 | 基準内 | 相対 1e-3、R は絶対 1e-3 |

入力は合成である。旧版との比較は `comparison/chapters/ch04b_ois_swaps.qmd`（許容誤差は絶対 1e-4）にある。

## 5. 旧版について気づいた点

旧版は変更していない。

1. **II-3.3.2 の値は、節点の古いカーブによる値である。** 評価日を 8/25 に動かした後の NPV 6.6505 は、2 章の状態のカーブから出ている。Riemann のカーブで同じ値は出ない（変更なしのカーブで 6.6987、新しい評価日で作り直すと 7.6877）。比較資料では N/C にした。
2. **II-3.3.1 の `daily_compounding_result` はすべて NA である。** `jpy_daily_float_cf_tbl` にない列 `rate` を見ている。基準値に入れていない。

## 6. 未対応

- EONIA の例（II-3.5、II-3.6）：OIS は 4 指標に限定する方針のため対象外。
- IBOR のスワップとマルチカーブ（II-3.4.3、II-3.4.4）：OIS に絞る方針のため対象外。割引カーブだけは別に渡せる。
- `MakeOIS` のテナー指定の **R の入口**（Rust の中核は 7 章で完了）。R は効果日と満期日を渡す形のまま。

## 7. テナーからの日付（2026-11-18、Carmine Studio の要望）

【実行照合済み・Rust の中核】`riemann::ois::compute_ois_swap_dates(index, evaluation_date, tenor, settlement_days)` が、QuantLib `MakeOIS(swapTenor, index)` に受渡日数を指定した場合の日付を返す。`OisSwapDates` は `effective_date`、`termination_date`（調整前。スワップにはこちらを渡す）、`maturity_date`（修正翌営業日で調整した値。QuantLib のスワップが報告する満期日）を持つ。

- **規則**：評価日を翌営業日に寄せ、受渡日数だけ営業日で進めて開始日とする。満期日は、開始日にテナーを足し、開始日がその月の最終営業日でテナーが 1 か月以上なら月末にする。それを修正翌営業日で調整したものが `maturity_date`。
- **照合**（`tools/golden/p11_ois_swap_dates.R`、`tests/golden/ois_swap_dates.csv`、`tests/golden_ois_swap_dates.rs`）：QuantLib R 1.42 の `MakeOIS`。4 指標、評価日 6 区間（週末・祝日・月末・年末年始と、Carmine Studio の 2026-09-10 を含む）、テナー 13 種（1D から 30Y、7Y を含む）、受渡日数 0〜3 の 13,520 件。**成功 13,514 件で開始日と満期日が完全一致**。QuantLib が失敗する 6 件（12/30〜12/31 の 1 日 OIS で、スケジュールが 1 日に縮退）は、調整前の満期日を渡したスワップの作成も同じく失敗する。
- **気づいた点**：満期日を「開始日＋テナー」のままにしても、調整しても、スケジュールの結果は同じだが、1 日 OIS の縮退の再現には調整前の日付を渡す必要がある。Carmine Studio の今の作り方（`seq()` で 7 年後）は、2026-09-10 の条件では一致する（開始 2026-09-14、満期 2033-09-14）。
- **未了（引継ぎ）**：R の関数 `compute_ois_swap_dates(index, evaluation_date, tenor, settlement_days = 2L)`【設計案】。`crates/riemann-r/src/rust/src/lib.rs` に extendr のラッパー（評価日とテナーのベクトル、3 列を返す）、`R/ois_swaps.R` に R 関数、roxygen と NAMESPACE、テスト、使用例を足し、Carmine Studio の `server/R/tona_ois_model.R` の `TONA_SWAP_DATES` をこの関数に置き換える。版は 0.3.5 に上げて公開する。
