# 日付・カレンダー・日付規約の仕様（草案）

日付と日付に関する金融規約は、**QuantLib の仕様に依拠する**。同じ入力に対して、QuantLib と同じ日付・同じ年数割合になることを照合して確かめる。

## 記述の区分

本書では、根拠の確かさを次の三つで区別する。

| 区分 | 意味 |
|---|---|
| 【ソース確認】 | QuantLib・finmath-lib のソースを読んで確かめた事実、またはソースを読んでの判断。プログラムを実行してはいない |
| 【実行照合済み】 | Riemann の実装を実行し、基準値と照合した結果 |
| 【設計案】 | Riemann としてそうする、という設計。未実装・未検証であり、照合で確かめるまでは目標にとどまる |

**現時点（2026-10-30）で【実行照合済み】の項目はない。** Riemann はまだ実装しておらず、本書の Riemann の動作に関する記述はすべて【設計案】である。

## 0. 参照した QuantLib

| 用途 | バージョン | コミット | 備考 |
|---|---|---|---|
| 仕様を読んだソース | 1.45-dev | `70f240f29ddb84927ff654502f1d75b3de24ba43`（2026-10-29） | `ql/time/` 以下 |
| 照合の基準値を作る R バインディング | 1.42（R パッケージ、2026-04-06 ビルド） | タグ `v1.42` = `41b0e1460f3e4991087917707ca29a9125e200db` | GaussQuant が使っている QuantLib |

【ソース確認】両者の `ql/time/` の差分（`git diff v1.42 70f240f`）を読んだ結果は次のとおりである。

- 差分なし：`calendar.hpp`／`calendar.cpp`、`businessdayconvention.hpp`、`dategenerationrule.hpp`、`calendars/japan.cpp`・`target.cpp`・`unitedstates.cpp`・`unitedkingdom.cpp`・`jointcalendar.cpp`
- 差分はあるが、読んだ範囲では判定に関わらない変更：
  - `date.cpp`：今日の日付の取得をスレッド安全にした（Riemann の中核では使わない）、既定の日付の作り方の整理
  - `schedule.hpp`／`schedule.cpp`：`ext::optional` から `std::optional` への置き換え
  - 日数計算の各ファイル：`make_shared` への置き換え。`actualactual.cpp` で削除された `yearFractionGuess` は、v1.42 でも定義があるだけで、`ql/` の中に呼び出しはなかった（`git grep`）

これは差分を読んでの判断であり、二つの版を実行して同じ入力で比べたわけではない（未実施）。

そこで、照合の正（基準）は **1.42 の R バインディングの実行結果**とする。`70f240f` のソースは、仕様を読むための参照にとどめる。1.42 の実行結果と、`70f240f` のソースから読み取った仕様が食い違う場合は、1.42 の実行結果に合わせ、食い違いを本書に記録する。QuantLib の版を上げるときは、差分の確認と基準値の作り直しを行い、この表を更新する。

## 1. 二つの「規約」の区別

似た名前の規約が二つあり、役割がまったく違う。

| 規約 | 役割 | 例 | 本書の章 |
|---|---|---|---|
| Business Day Convention（営業日調整） | 休日に当たった日付をどう動かすか | Following、Modified Following、Preceding、Unadjusted | 4 |
| Day Count Convention（日数計算） | 利息計算の期間を何年と数えるか | Actual/360、Actual/365 Fixed、Actual/Actual、30/360 | 6 |

設計と実装の順番は、**日付 → 休日カレンダー → 営業日調整 → スケジュール生成 → 日数計算**とする。後の章は前の章の結果を使う。

## 2. 日付

### 2.1 QuantLib の仕様（`ql/time/date.hpp`、`date.cpp`）【ソース確認】

| 項目 | QuantLib の仕様 |
|---|---|
| 表現 | 日のシリアル値（`serial_type` = `std::int_fast32_t`） |
| 有効範囲 | シリアル 367（1901-01-01）〜 109574（2199-12-31）。範囲外は例外 |
| シリアル値の基準 | Excel と同じ（1900 年をうるう年とする Excel の誤りに合わせた番号付け）。1900 年は範囲外なので、範囲内の日付には影響しない |
| うるう年 | 表で持つ（`YearIsLeap`）。1900 年は Excel に合わせてうるう年 |
| 曜日 | シリアル値を 7 で割った余り。0 なら土曜（Sunday = 1 … Saturday = 7） |
| 日の加減 | シリアル値の足し算。範囲外は例外 |
| 月の加減（`Date + n Months`） | 年月を進め、日がその月の日数を超えたら月末に丸める（1月31日 + 1か月 = 2月28日／29日） |
| 年の加減 | 2月29日で、移った先がうるう年でなければ 2月28日にする |
| 月末を保つ扱い | 日付の加算では行わない。カレンダーの `advance` の `endOfMonth` で行う（4.2） |
| 補助関数 | `endOfMonth`（暦の月末）、`isEndOfMonth`、`nextWeekday`、`nthWeekday` |
| 空の日付 | `Date()`（シリアル 0）を「日付なし」として使う |
| 時刻付きの日付 | `QL_HIGH_RESOLUTION_DATE` の場合だけ。Riemann では扱わない |

### 2.2 Riemann の設計【設計案】

- `Date` は、QuantLib と同じシリアル値を持つ値とする（`i32`）。範囲も同じ 1901-01-01〜2199-12-31 とし、範囲外は `Err` を返す。
- 「日付なし」をシリアル 0 で表さず、`Option<Date>` で表す。
- 年月日とシリアル値の変換は、暦の計算式（日数と年月日の相互変換）で行い、範囲内のすべての日で QuantLib と同じ結果になることを照合する。
- `Weekday` は QuantLib と同じ番号（Sunday = 1 … Saturday = 7）にする。
- 期間は `Period { n: i32, unit: TimeUnit }`、`TimeUnit` は `Days`・`Weeks`・`Months`・`Years`。日付と期間の加減は 2.1 の規則と同じにし、月末を保つ扱いは入れない。
- 今日の日付を返す関数は中核に置かない（R15）。必要なら言語の入口で求めて渡す。

**既存の crate を使うか**：使わず、自前で実装する。

- `time`・`chrono` は日付の範囲もシリアル値の基準も QuantLib と違う。月の加算の規則などを、結局は自前の層で QuantLib に合わせる必要がある。
- 必要な処理（シリアル値と年月日の変換、曜日、うるう年、月の加減）は小さく、依存を増やさない方が WebAssembly でも扱いやすい。
- `chrono` などとの変換が必要になったら、言語の入口の側に変換関数を置く。

## 3. 休日カレンダー

### 3.1 QuantLib の仕様（`ql/time/calendar.hpp`、`calendar.cpp`、`calendars/`）【ソース確認】

| 項目 | QuantLib の仕様 |
|---|---|
| 規則 | カレンダーごとの `Impl::isBusinessDay` に、休日の規則をコードで書く。日本の規則の内容は 3.3 に分けて書く |
| 日本（`japan.cpp`） | QuantLib 独自の書き方で、祝日法の規定をそのまま書いたものではない。内容は 3.3 |
| 例外日 | `addHoliday`／`removeHoliday` で臨時休日の追加と、休日の取り消しができる。`addedHolidays`・`removedHolidays` の集合で持つ |
| 判定の順序 | ① 追加した休日なら休日 → ② 取り消した休日なら営業日 → ③ 規則で判定 |
| 例外日の追加の決まり | 取り消してあった本来の休日を追加すると、取り消しを元に戻す。すでに休日なら何もしない |
| 例外日の共有範囲 | 同じ種類のカレンダーは、すべての実体が一つの実装を共有する（`Japan()` では `static` の `Impl`）。追加・取り消しは、プログラム全体の同じ種類のカレンダーに効く |
| 合成 | `JointCalendar`：休日の和（どちらかが休日なら休日）または営業日の和 |
| その他 | `isHoliday`、`isWeekend`、営業日の月末（`endOfMonth`、`isEndOfMonth`）、休日の一覧、2日間の営業日数（`businessDaysBetween`） |

### 3.2 Riemann の設計【設計案】

- カレンダーの種類は値とする：`enum Calendar { Japan, Target, UnitedStates(UsMarket), UnitedKingdom(UkMarket), WeekendsOnly, NullCalendar, Joint(Vec<Calendar>, JointRule) }`。
- 規則による判定は、カレンダーの種類ごとの副作用のない関数（`fn is_business_day_by_rule(cal: &Calendar, d: Date) -> bool`）とする。規則、近似式、一度きりの休日は QuantLib と同じにする（そうしないと照合できない）。日本については 3.3 のとおり、QuantLib 互換の実装とする。
- **例外日はデータとして持ち、規則と組み合わせる**：
  - `CalendarAdjustments { added: BTreeSet<Date>, removed: BTreeSet<Date> }` を値として持つ。
  - 判定の順序は QuantLib と同じ（① 追加 → ② 取り消し → ③ 規則）。
  - 追加・取り消しの決まり（取り消しの取り消しなど）も QuantLib と同じにし、「新しい `CalendarAdjustments` を返す関数」として作る。
  - QuantLib と違い、例外日はプログラム全体で共有しない。カレンダーと例外日の組（`HolidayCalendar { calendar, adjustments }`）を値として、評価の入力に明示的に渡す（R8）。
  - R・TypeScript からは、例外日を表（CSV・JSON）で渡す。政府が発表した臨時休日や、近似式と実際の春分・秋分の日がずれる年は、この例外日のデータで補う。
- 最初に用意するカレンダー：日本、TARGET、米国（SOFR、国債）、英国（Settlement）。必要に応じて足す。

### 3.3 日本の休日：QuantLib 互換の実装

Riemann の `Calendar::Japan` は、**QuantLib の `Japan` カレンダー（`ql/time/calendars/japan.cpp`）と同じ判定を返す、QuantLib 互換の実装**とする。祝日法（国民の祝日に関する法律）そのものを実装するものではない。

**QuantLib の書き方【ソース確認】**

| 項目 | `japan.cpp` の書き方 |
|---|---|
| 週末 | 土曜・日曜は休日 |
| 固定日の祝日 | 日付で判定する。多くの祝日は「その日、または翌日が月曜ならその月曜」と書かれている |
| ハッピーマンデー | 「第2月曜」などを、曜日と日の範囲で判定する。導入前の年は固定日で判定する |
| ゴールデンウィーク | 5月3日・4日・5日に加え、「5月6日が月・火・水曜なら休日」とする |
| 春分・秋分の日 | 近似式 `20.69115 + (y-2000)*0.242194 - うるう年の数`（秋分は 23.09）で求めた日と、その翌日が月曜ならその月曜 |
| 国民の休日（秋） | 敬老の日と秋分の日に挟まれた火曜を休日とする条件 |
| 銀行休業日 | 1月2日・3日、12月31日を休日とする |
| 一度きりの休日 | 1959年4月10日、1989年2月24日、1990年11月12日、1993年6月9日、2019年4月30日・5月1日・5月2日・10月22日を、年を条件にして書く |
| 東京オリンピック | 2020年・2021年の海の日・山の日などの移動を、年を条件にして書く |

**祝日法の一般的な説明との区別**

- 上の表は、QuantLib がそう判定するという事実であり、祝日法の規定をそのまま書いたものではない。たとえば、春分の日・秋分の日は法律上、天文学的な観測に基づいて毎年公表される日であるが、QuantLib は近似式で求めている。振替休日も、法律の文言ではなく、祝日ごとの「翌日が月曜なら」という条件で表している。
- 1月2日・3日と12月31日は、国民の祝日ではなく、金融機関の休業日として休日に含めている。
- QuantLib の判定と、祝日法・政府の公表・実際の市場の休業日が、すべての年で一致するかは確かめていない（未確認）。
- Riemann はまず QuantLib との一致を目標とする（照合の基準が QuantLib のため）。実際の休業日と違う日が分かった場合は、規則を変えずに、例外日のデータ（`CalendarAdjustments`）で補う。規則そのものを祝日法に合わせて書き直す場合は、QuantLib 互換とは別の種類（例：`Calendar::JapanStatutory`）として足し、`Calendar::Japan` は QuantLib 互換のまま残す。

## 4. 営業日調整

### 4.1 調整（`Calendar::adjust`）

| 規約 | QuantLib の動作 |
|---|---|
| Unadjusted | 動かさない |
| Following | 営業日になるまで翌日へ進める |
| ModifiedFollowing | Following で月が変わったら、Preceding にする |
| HalfMonthModifiedFollowing | ModifiedFollowing に加え、15日以前の日付が16日以降に動いたら Preceding にする |
| Preceding | 営業日になるまで前日へ戻す |
| ModifiedPreceding | Preceding で月が変わったら、Following にする |
| Nearest | 前後に1日ずつ広げ、先に営業日に当たった方。前後が同時に営業日なら後（翌日側）を選ぶ |

### 4.2 日付を進める（`Calendar::advance`）

| 単位 | QuantLib の動作 |
|---|---|
| n = 0 | 調整だけ行う |
| Days | 営業日を n 日数える（休日を飛ばして1日ずつ進める・戻す） |
| Weeks | 暦で n 週進めてから調整する |
| Months・Years | 暦で進め（2.1 の月・年の加算）、月末の扱い（`endOfMonth`）が有効なら、次のどちらかにする。そうでなければ調整する。<br>・規約が Unadjusted で、元の日付が暦の月末なら、進めた先の暦の月末<br>・それ以外で、元の日付が営業日の月末なら、進めた先の営業日の月末 |

【設計案】Riemann では、`adjust(cal, d, bdc) -> Result<Date>` と `advance(cal, d, period, bdc, end_of_month) -> Result<Date>` を、QuantLib と同じ動作の関数として作る。上の 4.1・4.2 の表は【ソース確認】である。`cal` は `HolidayCalendar`（例外日付き）である。

## 5. スケジュール生成

### 5.1 QuantLib の仕様（`ql/time/schedule.hpp`、`schedule.cpp`）【ソース確認】

- 入力：開始日、終了日、期間（tenor）、カレンダー、営業日調整、終了日の営業日調整、日付の生成規則、月末の扱い、最初の日付（first date）、最後から2番目の日付（next-to-last date）。
- 生成規則（`DateGeneration::Rule`）：Backward、Forward、Zero、ThirdWednesday、ThirdWednesdayInclusive、Twentieth、TwentiethIMM、OldCDS、CDS、CDS2015。
- 月末の扱いは、期間が1か月以上の月・年単位のときだけ有効（`allowsEndOfMonth`）。
- 端数期間（stub）：first date・next-to-last date を指定でき、各期間が規則どおりかを `isRegular` で返す。

### 5.2 Riemann の設計【設計案】

- `ScheduleSpec`（上の入力をすべて持つ値）を受け取り、`Schedule { dates, is_regular }` を返す関数 `generate(&ScheduleSpec) -> Result<Schedule>` とする。日付を直接並べて作るスケジュールも受け付ける。
- 生成規則は段階的に足す：まず Backward・Forward・Zero（債券・OIS に必要）、CDS の段階で Twentieth・IMM・CDS 系。
- 入力の矛盾（first date が範囲外など）は、QuantLib が例外にする条件と同じ条件で `Err` を返す。

## 6. 日数計算

### 6.1 QuantLib の種類（`ql/time/daycounters/`）【ソース確認】

| 種類 | 変種 |
|---|---|
| Actual/360 | 最終日を含めるか（`includeLastDay`） |
| Actual/364、Actual/365.25、Actual/366 | — |
| Actual/365 (Fixed) | Standard、Canadian、NoLeap |
| Actual/Actual | ISMA、Bond（スケジュールか参照期間が要る）、ISDA、Historical、Actual365、AFB、Euro |
| 30/360 | USA、BondBasis、European、EurobondBasis、Italian、German、ISMA、ISDA、NASD（German・ISDA は終了日が要る） |
| 30/365、Business/252（カレンダーが要る）、One、Simple | — |

### 6.2 Riemann の設計【設計案】

- `enum DayCount` に、QuantLib の種類と変種を値として持たせる（例：`Thirty360(Thirty360Kind::BondBasis)`、`ActualActual(ActActKind::Isma { reference })`）。
- `day_count(dc, d1, d2) -> i32` と `year_fraction(dc, d1, d2, ref_start, ref_end) -> Result<f64>` を関数として作る。参照期間やスケジュールが要る種類で、それが与えられていなければ `Err` を返す。
- 最初に作る種類：Actual/360、Actual/365 Fixed、30/360 BondBasis、Actual/Actual ISDA、Actual/Actual ISMA（債券）。GaussQuant・LagrangeFinance で使っている種類である。

## 7. QuantLib との照合【設計案・未実施】

基準値は `tools/golden/` の R スクリプトで、QuantLib の R バインディング（1.42）から CSV に書き出す。基準値はまだ作っておらず、照合もまだ行っていない。以下は照合の計画である。比べ方は仕様書 7.3 の許容誤差に従う。

| 対象 | 基準値の内容 | 一致の基準 |
|---|---|---|
| 日付 | 範囲内のすべての日（シリアル 367〜109574）の年・月・日・曜日・うるう年・暦の月末 | 完全一致 |
| 日付と期間の加減 | 月末・2月29日・年末を含む日付と、−120〜120 か月・年の組み合わせ。範囲外になる組み合わせは例外になることも確かめる | 完全一致 |
| 休日 | カレンダーごとに、1901〜2199 年のすべての日の営業日・休日の判定 | 完全一致 |
| 例外日 | 追加・取り消し・取り消しの取り消しを含む例。QuantLib の側は例外日が全体で共有されるので、例ごとに `resetAddedAndRemovedHolidays` で元に戻してから作る | 完全一致 |
| 営業日調整 | 7種類の規約 × 月末・連休・年末年始を含む日付 | 完全一致 |
| 日付を進める | 日・週・月・年 × 規約 × 月末の扱いの有無 | 完全一致 |
| スケジュール | 開始日・終了日・期間・生成規則・月末の扱い・端数期間の組み合わせ。日付と `isRegular` を比べる | 完全一致 |
| 日数計算 | 日付の組 × 種類・変種。日数と年数割合 | 日数は完全一致、年数割合は仕様書 7.3 の「四則演算だけの計算」の基準 |

## 8. ライセンス上の扱い

休日の規則は QuantLib のコードを読んで同じ判定を作るので、QuantLib のコードから派生したものとして扱う。カレンダーの規則を書くファイルには、元のファイル（`ql/time/calendars/*.cpp`）・コミット・QuantLib の著作権表示と修正 BSD ライセンスの条件を記載する。リポジトリに `THIRD_PARTY_NOTICES` を置き、QuantLib のライセンス全文を収める。
