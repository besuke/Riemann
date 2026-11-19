# トータル・リターン・スワップ（TRS）の両レッグ

QuantLet の C1（TRS 確認書）から、キャッシュフロー・スケジュールを作るために追加した（2026-11-15）。中核は `crates/rmql/src/trs.rs`、R の入口は `R/trs.R`。QuantLib のソースは使っていない（`THIRD_PARTY_NOTICES` の対象外）。

## 1. 範囲【設計】

| 関数（R） | 内容 |
|---|---|
| `create_trs_funding_leg()` | FundingLeg：計算期間、リセット日、金利決定日、観測期間（ルックバック）、支払日 |
| `get_trs_funding_schedule()`・`get_trs_funding_observations()` | 期間ごとの日付、営業日ごとの観測日と重み |
| `compute_trs_funding_cashflows()` | 翌日物の複利（ルックバックあり、観測期間のシフトなし）＋スプレッドの金額と、その状態 |
| `create_trs_return_leg()`・`get_trs_return_schedule()` | ReturnLeg：計算期間、評価日（支払日または期間終了日から営業日数）、明記された最終評価日、支払日 |
| `compute_trs_return_cashflows()` | `notional × (期末水準 / 期初水準 − 1)`。最初の期間は当初価格から |

- **計算期間**：効力発生日から終了日まで、年 `frequency` 回、`period_convention` で調整（`generate_schedule` と同じ。調整前の日も返す）
- **支払日**：期間終了日（調整後）から支払カレンダーの `payment_lag` 営業日後、`payment_convention` で調整。確認書が定めていなければ `payment_lag = NULL` とし、支払日は `NA`（推測で作らない）
- **FundingLeg の複利**：期間内の各営業日 `d_i`（指標のカレンダー）に、`lookback` 営業日前の金利を当てる。重みは `d_i` から次の営業日までの指標の年数（計算期間の暦日。シフトなし）。`rate = (Π(1 + r_i τ_i) − 1) / τ(start, end)`、金額は `notional × (rate + spread) × accrual`（`accrual` はレッグの日数計算）。リセット日・金利決定日は確認書の記載どおりに返すが、複利の計算には日々の観測日を使う
- **金額の状態**：`fixed`（公表値だけ）、`projected`（予測カーブを使う。各観測日の翌日物の先渡金利＝割引係数の比）、`scenario`（ReturnLeg のシナリオの水準を使う）、`missing`（前提が不足。金額は `NA` で理由を返す）。評価日より前の観測日は公表値が必要で、評価日当日は公表値があれば使い、なければ予測カーブを使う（`ois_swap.rs` と同じ扱い）
- **ReturnLeg の評価日**：支払日（`"payment_date"`）または期間終了日（`"period_end"`）から `valuation_offset` 営業日。明記された最終評価日は最後の評価日を置き換え、規則による日（`valuation_date_by_rule`）も並べて返す
- 未対応：観測期間のシフト、ロックアウト、算術平均、金利の下限、元本の交換・リセット、配当・クーポンの別払い（指数が配当込みならその水準で扱う）

## 2. 照合【実行照合済み】

2026-11-15、`tests/testthat/test-trs.R`（68 項目、すべて合格）と `trs.rs` の単体テスト 3 件。全テスト（`cargo test --release`）274 件合格、`cargo clippy --all-targets -- -D warnings` 警告なし。

| 照合 | 相手 | 結果 |
|---|---|---|
| 計算期間（2例：TONA 四半期、SOFR 月次・月末） | QuantLib R 1.42 の `Schedule`（Backward） | 一致 |
| 支払日・金利決定日・観測日（営業日の加減） | QuantLib の `Calendar_advance`・`Calendar_adjust`（Japan、SOFR の fixing calendar） | 一致 |
| 期間内の営業日、重みの合計 = 期間の年数 | QuantLib の `Calendar_isBusinessDay` | 一致（重みの合計の差 1e-14 以内） |
| ルックバック付き複利と金額（日付ごとに異なる合成の公表値） | R で直接計算した積 | 相対差 1e-12 以内（実測 2e-13 程度。積の順序による丸め） |
| 予測カーブによる複利 | R で割引係数の比から直接計算 | 相対差 1e-12 以内（実測 9e-16 の絶対差） |
| ReturnLeg の評価日（期間終了日基準・支払日基準） | QuantLib の営業日の加減 | 一致 |

**デモの確認書での気づき**（QuantLet の C1 デモ、架空）：評価日の規則「Two Business Days prior to each Payment Date」を FundingLeg の支払日（終了日の 2 営業日後）と読むと、最後の評価日は 2027-10-08 になり、確認書の最終評価日 2027-10-06 と一致しない。期間終了日（2027-10-08）の 2 営業日前と読むと一致する。どちらの読み方も関数の引数で選べる。

## 3. 未了

- 配布ファイル（`dist/`）は作り直していない。版は 0.3.0 のまま（`R CMD INSTALL .` で利用者のライブラリーに入れた）
- `docs/specification.md` の段階表と比較資料（`comparison/`）への追加
