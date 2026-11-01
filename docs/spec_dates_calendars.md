# 日付・カレンダー・日付規約の仕様（草案）

日付と日付に関する金融規約は、**QuantLib の仕様に依拠する**。同じ入力に対して、QuantLib と同じ日付・同じ年数割合になることを照合して確かめる。

## 記述の区分

本書では、根拠の確かさを次の三つで区別する。

| 区分 | 意味 |
|---|---|
| 【ソース確認】 | QuantLib・finmath-lib のソースを読んで確かめた事実、またはソースを読んでの判断。プログラムを実行してはいない |
| 【実行照合済み】 | Riemann の実装を実行し、基準値と照合した結果 |
| 【設計案】 | Riemann としてそうする、という設計。未実装・未検証であり、照合で確かめるまでは目標にとどまる |

**現時点（2026-10-31）で【実行照合済み】の項目は、日付（2.3 節）、休日カレンダー（3.4 節）、営業日調整（4.3 節）、スケジュール生成（5.3 節）、日数計算（6.3 節）である。** それ以外の Riemann の動作に関する記述は【設計案】である。

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

### 2.2 Riemann の設計【設計案】（2.3 で照合済み）

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

### 2.3 QuantLib との照合結果【実行照合済み】

2026-10-31 に、`crates/riemann/src/time/` の実装を、QuantLib R バインディング 1.42 の実行結果（`tools/golden/p1_dates.R` で作成）と照合した（`cargo test --test golden_dates`）。

| 対象 | 件数 | 結果 |
|---|---|---|
| 範囲内のすべての日付（シリアル 367〜109574）の年・月・日・曜日・うるう年・暦の月末・月末の日付。年月日からシリアル値への逆変換も確かめた | 109,208 日 | すべて完全一致 |
| 日付と期間の加減（19 の基準日 × 日・週・月・年。月は −120〜120 か月、年は −120〜120 年） | 9,576 件（うち QuantLib がエラーにした 1,183 件） | すべて完全一致。エラーになる組み合わせも一致した |

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
- 最初に用意するカレンダー：日本、TARGET、米国（SOFR、国債）、英国（Settlement）。その後、QuantLib の全カレンダー・全市場区分を実装した（3.5 節）。

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

### 3.4 実装と QuantLib との照合結果【実行照合済み】

**実装（`crates/riemann/src/time/calendar/`）**

- `Calendar`（規則の種類）：`NullCalendar`、`WeekendsOnly`、`Japan`、`Target`、`UnitedStates(Settlement / GovernmentBond / Sofr)`、`UnitedKingdom(Settlement)`、`Joint(..., JoinHolidays / JoinBusinessDays)`。
- 規則（`rules.rs`）は QuantLib の条件式を移したもので、ファイルの先頭に移植元・コミット・著作権表示を書き、`THIRD_PARTY_NOTICES` に QuantLib のライセンス全文を置いた。
- 復活祭（TARGET・米国国債・SOFR・英国の Good Friday と Easter Monday）は、QuantLib のように表を持たず、グレゴリオ暦の計算式（Meeus/Jones/Butcher）で求める。表と一致することは、下の照合で確かめた。
- 例外日は `HolidayCalendar { calendar, adjustments }` に値として持つ。`with_holiday`・`without_holiday` は QuantLib の `addHoliday`・`removeHoliday` と同じ決まりで、新しい値を返す（元の値は変わらない）。
- 【設計案】QuantLib の `JointCalendar` は、構成するカレンダーそれぞれの例外日を参照する。Riemann では、例外日は合成したカレンダー全体に付ける形にしている。構成するカレンダーごとの例外日が必要になったら、照合を足して扱いを決める。

**照合結果**（2026-10-31、QuantLib R バインディング 1.42、`tools/golden/p1_calendars.R`、`cargo test --test golden_calendars`）

| カレンダー | 週末以外の休日の数（1901〜2199 年） | 結果 |
|---|---|---|
| 日本 | 4,575 | 全日付で完全一致 |
| TARGET | 1,118 | 全日付で完全一致 |
| 米国 Settlement | 3,016 | 全日付で完全一致 |
| 米国 国債 | 3,189 | 全日付で完全一致 |
| 米国 SOFR | 3,235 | 全日付で完全一致 |
| 英国 Settlement | 2,399 | 全日付で完全一致 |
| 週末のみ、NullCalendar | 0 | 全日付で完全一致 |
| 日本と米国 SOFR の合成（休日の和／営業日の和） | 7,298 ／ 512 | 全日付で完全一致 |

- 「全日付」は、シリアル 367〜109574 の 109,208 日すべてである。各カレンダーの週末の曜日も一致した。
- 例外日：日本のカレンダーに、追加・取り消し・取り消しの取り消し・週末の取り消しなど 9 つの操作を順に重ねて行い、各段階で 6 つの日付の営業日判定を比べた（54 件）。すべて一致した。

### 3.5 全カレンダー・全市場区分の対応一覧と照合結果【実行照合済み】

QuantLib C++ v1.42 にある全カレンダーと全市場区分を実装した（41 の国・地域と 69 の市場区分、ほかに `NullCalendar`、`WeekendsOnly`、`BespokeCalendar`、`JointCalendar`）。

**用途の範囲**：新興国のカレンダーは、当面は債券の受渡日と利払日の営業日判定に使う。スワップの規約や金利カーブは追加しない。OIS は TONAR・€STR・SOFR・SONIA の 4 指標に限定する。

**QuantLib との一致と、実際の市場休日の検証は別である。** この節の「一致」は、QuantLib と同じ判定を返すことだけを意味する。どのカレンダーも、取引所や中央銀行の公表資料とは照合していない（一覧の「実市場」はすべて未検証）。

**実装**

- `Calendar` に国ごとの値、複数の市場区分がある国に `BrazilMarket` などの値を追加した。市場区分が一つの国は、区分の値を持たない（`Calendar::Argentina` は Merval）。`Calendar::name` は QuantLib の名前を返す。
- `calendar/rules/{americas,europe,middle_east_africa,asia_pacific,tables}.rs` は、`tools/golden/port_calendars.R` が QuantLib v1.42 のソースから機械的に変換した。条件式と比較しやすいよう、QuantLib の形とコメントを保ち、月は数字で書く。`special.rs`（イスラエル、サウジアラビア、ロシア、中国 IB）と米国の追加市場は手で書いた。
- **共有している規則**：日付の要素（`Day`）、週末の判定、西方と東方の復活祭、日付の表の検索。同じ規則の市場は同じ関数を使う（英国の 3 市場、イスラエルの Settlement と TASE、インドネシアの 3 市場）。規則を土台にする市場はその関数を呼ぶ（米国 SOFR と LiborImpact、ポーランド WSE、ルーマニア BVB、韓国 KRX、ニュージーランド、中国 IB）。国をまたぐ休日の条件は、年の条件が国ごとに違い、ソースと比べにくくなるので共通にしていない。
- 東方の復活祭は、QuantLib の表を持たず、ユリウス暦の Meeus の計算式で求める。全日付の照合で表との一致を確かめた。
- **`Result` への変更**：QuantLib が例外を投げる日付があるので、`is_business_day`、`is_holiday`、`with_holiday`、`without_holiday`、`holidays`、`business_days_between`、`DayCount::day_count` の戻り値を `Result` にした。既存の計算は変えていない。

**照合の条件**：2026-11-01、**QuantLib C++ v1.42 そのもの**（コミット `41b0e14`。カレンダーに必要なソースを無修正でビルド。g++ 14.3.0、Boost は Rtools 4.5 のヘッダー）。`tools/golden/p1_all_calendars.{cpp,sh}`、`cargo test --test golden_all_calendars`。シリアル 367〜109574 の 109,208 日すべてで、営業日の判定、エラーの有無と原因、週末の曜日、名前を比べた。

**R パッケージとの突き合わせ**：Version 1.42（正確な C++ コミットは不明）の `holidayList` を、作れる 67 カレンダーで比べ、差は 0 件だった（週末以外の休日。例外になる範囲と末日 2199-12-31 を除く。`tools/verify/verify_calendars.R`）。`Germany::Euwax` と `Indonesia::IDX` は R パッケージの enum になく作れないので、C++ だけと比べた。

| 地域 | クラス | 市場 | 比べた日数の合計 | C++ との差 |
|---|---|---|---:|---:|
| 南北アメリカ | 6 | 14 | 1,528,912 | 0 |
| 欧州 | 19 | 31 | 3,385,448 | 0 |
| 中東・アフリカ | 5 | 7 | 764,456 | 0 |
| アジア太平洋 | 11 | 17 | 1,856,536 | 0 |

共通のカレンダー（Joint は 3 つの組み合わせと、エラーの伝わり方を見る日本と MOEX の組み合わせ。Bespoke は水曜日と木曜日を週末にした例）：

| キー | 週末以外の休日 | 週末の営業日 | エラーの日 | C++ との差 |
|---|---:|---:|---:|---:|
| `null_calendar` | 0 | 0 | 0 | 0 |
| `weekends_only` | 0 | 0 | 0 | 0 |
| `bespoke_wednesday_thursday` | 0 | 0 | 0 | 0 |
| `joint_three_holidays` | 8,288 | 0 | 0 | 0 |
| `joint_three_business_days` | 220 | 0 | 0 | 0 |
| `joint_japan_moex_holidays` | 5,402 | 0 | 27452 | 0 |
| `joint_japan_moex_business_days` | 313 | 9 | 13090 | 0 |

**例外になる日付**（QuantLib と Riemann で同じ。Riemann は `Err` を返す）

| カレンダー | 日付 | 原因 |
|---|---|---|
| `Russia(MOEX)` | 1901-01-01〜2011-12-31（40,542 日） | 2012 年より前のデータがない |
| `Israel(Settlement, TASE)` | 1901-01-01〜1901-01-21 の平日など 15 日と 2199-12-31 | 規則が前後の日付を参照し、有効範囲を外れる |
| `Israel(SHIR)` | 1901-01-01〜1901-01-21 の平日など 15 日と 2199-12-31 | 同上 |

**収録期間の制限**：「年別データ」のカレンダーは、下の範囲の外では年別の休日が入らず、固定日と規則だけになる。エラーにはならないので（MOEX の 2012 年より前を除く）、範囲の外では祝日が営業日と判定されることがある。実際の休日が分かっていれば `with_holiday` で例外日として補える。「規則のみ」も、将来の臨時休日は入らない。範囲は、ソースの年の記述と、基準値の年ごとの休日数から確かめた。

**対応一覧**

| 地域 | QuantLib のクラス | 市場 | Riemann | 種類 | 年別データ・一度きりの休日 | 注記 | 週末以外の休日 | エラーの日 | C++ との差 | R パッケージ | 実市場 |
|---|---|---|---|---|---|---|---:|---:|---:|---|---|
| 南北アメリカ | `Argentina` | Merval | `Calendar::Argentina` | 規則のみ | — |  | 2,948 | 0 | 0 | 差 0 | 未検証 |
| 南北アメリカ | `Brazil` | Settlement | `Calendar::Brazil(BrazilMarket::Settlement)` | 規則のみ | — | 年の条件は規則の変更だけ（例：11/20）。年別の表なし | 3,024 | 0 | 0 | 差 0 | 未検証 |
| 南北アメリカ | `Brazil` | Exchange | `Calendar::Brazil(BrazilMarket::Exchange)` | 規則のみ | — | 年の条件は規則の変更だけ（例：11/20）。年別の表なし | 3,721 | 0 | 0 | 差 0 | 未検証 |
| 南北アメリカ | `Canada` | Settlement | `Calendar::Canada(CanadaMarket::Settlement)` | 規則のみ | — |  | 3,361 | 0 | 0 | 差 0 | 未検証 |
| 南北アメリカ | `Canada` | TSX | `Calendar::Canada(CanadaMarket::Tsx)` | 規則のみ | — |  | 2,883 | 0 | 0 | 差 0 | 未検証 |
| 南北アメリカ | `Chile` | SSE | `Calendar::Chile` | 規則と一度きりの休日 | 2017〜2022 | 先住民の日は 2021〜2199 年の表で全期間をカバー | 3,455 | 0 | 0 | 差 0 | 未検証 |
| 南北アメリカ | `Mexico` | BMV | `Calendar::Mexico` | 規則のみ | — |  | 2,708 | 0 | 0 | 差 0 | 未検証 |
| 南北アメリカ | `UnitedStates` | Settlement | `Calendar::UnitedStates(UsMarket::Settlement)` | 規則と一度きりの休日 | 1954〜2025 |  | 3,016 | 0 | 0 | 差 0 | 未検証 |
| 南北アメリカ | `UnitedStates` | NYSE | `Calendar::UnitedStates(UsMarket::Nyse)` | 規則と一度きりの休日 | 1954〜2025 |  | 2,853 | 0 | 0 | 差 0 | 未検証 |
| 南北アメリカ | `UnitedStates` | GovernmentBond | `Calendar::UnitedStates(UsMarket::GovernmentBond)` | 規則と一度きりの休日 | 1954〜2025 |  | 3,189 | 0 | 0 | 差 0 | 未検証 |
| 南北アメリカ | `UnitedStates` | NERC | `Calendar::UnitedStates(UsMarket::Nerc)` | 規則と一度きりの休日 | 1954〜2025 |  | 1,667 | 0 | 0 | 差 0 | 未検証 |
| 南北アメリカ | `UnitedStates` | LiborImpact | `Calendar::UnitedStates(UsMarket::LiborImpact)` | 規則と一度きりの休日 | 1954〜2025 |  | 2,963 | 0 | 0 | 差 0 | 未検証 |
| 南北アメリカ | `UnitedStates` | FederalReserve | `Calendar::UnitedStates(UsMarket::FederalReserve)` | 規則と一度きりの休日 | 1954〜2025 |  | 2,823 | 0 | 0 | 差 0 | 未検証 |
| 南北アメリカ | `UnitedStates` | SOFR | `Calendar::UnitedStates(UsMarket::Sofr)` | 規則と一度きりの休日 | 1954〜2025 |  | 3,235 | 0 | 0 | 差 0 | 未検証 |
| 欧州 | `Austria` | Settlement | `Calendar::Austria(AustriaMarket::Settlement)` | 規則のみ | — |  | 3,079 | 0 | 0 | 差 0 | 未検証 |
| 欧州 | `Austria` | Exchange | `Calendar::Austria(AustriaMarket::Exchange)` | 規則のみ | — |  | 2,358 | 0 | 0 | 差 0 | 未検証 |
| 欧州 | `CzechRepublic` | PSE | `Calendar::CzechRepublic` | 規則と一度きりの休日 | 2004 |  | 2,835 | 0 | 0 | 差 0 | 未検証 |
| 欧州 | `Denmark` | （区分なし） | `Calendar::Denmark` | 規則のみ | — |  | 3,081 | 0 | 0 | 差 0 | 未検証 |
| 欧州 | `Finland` | （区分なし） | `Calendar::Finland` | 規則のみ | — |  | 2,689 | 0 | 0 | 差 0 | 未検証 |
| 欧州 | `France` | Settlement | `Calendar::France(FranceMarket::Settlement)` | 規則のみ | — |  | 2,436 | 0 | 0 | 差 0 | 未検証 |
| 欧州 | `France` | Exchange | `Calendar::France(FranceMarket::Exchange)` | 規則のみ | — |  | 1,882 | 0 | 0 | 差 0 | 未検証 |
| 欧州 | `Germany` | Settlement | `Calendar::Germany(GermanyMarket::Settlement)` | 規則のみ | — |  | 2,776 | 0 | 0 | 差 0 | 未検証 |
| 欧州 | `Germany` | FrankfurtStockExchange | `Calendar::Germany(GermanyMarket::FrankfurtStockExchange)` | 規則のみ | — |  | 1,668 | 0 | 0 | 差 0 | 未検証 |
| 欧州 | `Germany` | Xetra | `Calendar::Germany(GermanyMarket::Xetra)` | 規則のみ | — |  | 1,668 | 0 | 0 | 差 0 | 未検証 |
| 欧州 | `Germany` | Eurex | `Calendar::Germany(GermanyMarket::Eurex)` | 規則のみ | — |  | 1,882 | 0 | 0 | 差 0 | 未検証 |
| 欧州 | `Germany` | Euwax | `Calendar::Germany(GermanyMarket::Euwax)` | 規則のみ | — |  | 1,967 | 0 | 0 | R で作れない | 未検証 |
| 欧州 | `Hungary` | （区分なし） | `Calendar::Hungary` | 規則のみ | — |  | 2,491 | 0 | 0 | 差 0 | 未検証 |
| 欧州 | `Iceland` | ICEX | `Calendar::Iceland` | 規則のみ | — |  | 3,141 | 0 | 0 | 差 0 | 未検証 |
| 欧州 | `Italy` | Settlement | `Calendar::Italy(ItalyMarket::Settlement)` | 規則と一度きりの休日 | 1999 |  | 2,362 | 0 | 0 | 差 0 | 未検証 |
| 欧州 | `Italy` | Exchange | `Calendar::Italy(ItalyMarket::Exchange)` | 規則と一度きりの休日 | 1999 |  | 2,096 | 0 | 0 | 差 0 | 未検証 |
| 欧州 | `Norway` | （区分なし） | `Calendar::Norway` | 規則のみ | — |  | 2,689 | 0 | 0 | 差 0 | 未検証 |
| 欧州 | `Poland` | Settlement | `Calendar::Poland(PolandMarket::Settlement)` | 規則のみ | — |  | 2,443 | 0 | 0 | 差 0 | 未検証 |
| 欧州 | `Poland` | WSE | `Calendar::Poland(PolandMarket::Wse)` | 規則のみ | — |  | 2,871 | 0 | 0 | 差 0 | 未検証 |
| 欧州 | `Romania` | Public | `Calendar::Romania(RomaniaMarket::Public)` | 規則と一度きりの休日 | 2014 |  | 2,635 | 0 | 0 | 差 0 | 未検証 |
| 欧州 | `Romania` | BVB | `Calendar::Romania(RomaniaMarket::Bvb)` | 規則と一度きりの休日 | 2014 |  | 2,637 | 0 | 0 | 差 0 | 未検証 |
| 欧州 | `Russia` | Settlement | `Calendar::Russia(RussiaMarket::Settlement)` | 年別データ | 2012〜2020 | 追加の休日と振替営業日は 2020 年まで。MOEX は 2012 年より前がエラー | 2,959 | 0 | 0 | 差 0 | 未検証 |
| 欧州 | `Russia` | MOEX | `Calendar::Russia(RussiaMarket::Moex)` | 年別データ | 2012〜2020 | 追加の休日と振替営業日は 2020 年まで。MOEX は 2012 年より前がエラー | 1,140 | 40542 | 0 | 差 0 | 未検証 |
| 欧州 | `Slovakia` | BSSE | `Calendar::Slovakia` | 規則と一度きりの休日 | 2004〜2005 |  | 3,383 | 0 | 0 | 差 0 | 未検証 |
| 欧州 | `Sweden` | （区分なし） | `Calendar::Sweden` | 規則のみ | — |  | 2,934 | 0 | 0 | 差 0 | 未検証 |
| 欧州 | `Switzerland` | （区分なし） | `Calendar::Switzerland` | 規則のみ | — |  | 2,477 | 0 | 0 | 差 0 | 未検証 |
| 欧州 | `TARGET` | （区分なし） | `Calendar::Target` | 規則と一度きりの休日 | 1998〜2001 |  | 1,118 | 0 | 0 | 差 0 | 未検証 |
| 欧州 | `Ukraine` | USE | `Calendar::Ukraine` | 規則のみ | — |  | 2,786 | 0 | 0 | 差 0 | 未検証 |
| 欧州 | `UnitedKingdom` | Settlement | `Calendar::UnitedKingdom(UkMarket::Settlement)` | 規則と一度きりの休日 | 1995〜2023 |  | 2,399 | 0 | 0 | 差 0 | 未検証 |
| 欧州 | `UnitedKingdom` | Exchange | `Calendar::UnitedKingdom(UkMarket::Exchange)` | 規則と一度きりの休日 | 1995〜2023 |  | 2,399 | 0 | 0 | 差 0 | 未検証 |
| 欧州 | `UnitedKingdom` | Metals | `Calendar::UnitedKingdom(UkMarket::Metals)` | 規則と一度きりの休日 | 1995〜2023 |  | 2,399 | 0 | 0 | 差 0 | 未検証 |
| 中東・アフリカ | `Botswana` | （区分なし） | `Calendar::Botswana` | 規則のみ | — |  | 2,989 | 0 | 0 | 差 0 | 未検証 |
| 中東・アフリカ | `Israel` | Settlement | `Calendar::Israel(IsraelMarket::Settlement)` | 年別データ | 2000〜2050 | 範囲の両端の日付は QuantLib も Riemann もエラー | 679 | 16 | 0 | 差 0 | 未検証 |
| 中東・アフリカ | `Israel` | TASE | `Calendar::Israel(IsraelMarket::Tase)` | 年別データ | 2000〜2050 | 範囲の両端の日付は QuantLib も Riemann もエラー | 679 | 16 | 0 | 差 0 | 未検証 |
| 中東・アフリカ | `Israel` | SHIR | `Calendar::Israel(IsraelMarket::Shir)` | 年別データ | 2000〜2050 | 範囲の両端の日付は QuantLib も Riemann もエラー | 1,797 | 16 | 0 | 差 0 | 未検証 |
| 中東・アフリカ | `SaudiArabia` | Tadawul | `Calendar::SaudiArabia` | 年別データ | 1998〜2029 | 犠牲祭は 2022 年まで、断食明けは 2029 年まで | 6,278 | 0 | 0 | 差 0 | 未検証 |
| 中東・アフリカ | `SouthAfrica` | （区分なし） | `Calendar::SouthAfrica` | 規則と一度きりの休日 | 2004〜2024 |  | 3,123 | 0 | 0 | 差 0 | 未検証 |
| 中東・アフリカ | `Turkey` | （区分なし） | `Calendar::Turkey` | 年別データ | 2004〜2034 | 2024 年以降は要検証とソースに記載 | 1,576 | 0 | 0 | 差 0 | 未検証 |
| アジア太平洋 | `Australia` | Settlement | `Calendar::Australia(AustraliaMarket::Settlement)` | 規則と一度きりの休日 | 2022 |  | 2,903 | 0 | 0 | 差 0 | 未検証 |
| アジア太平洋 | `Australia` | ASX | `Calendar::Australia(AustraliaMarket::Asx)` | 規則と一度きりの休日 | 2022 |  | 2,305 | 0 | 0 | 差 0 | 未検証 |
| アジア太平洋 | `China` | SSE | `Calendar::China(ChinaMarket::Sse)` | 年別データ | 2004〜2026 | IB の振替営業日は 2005〜2026 年 | 1,872 | 0 | 0 | 差 0 | 未検証 |
| アジア太平洋 | `China` | IB | `Calendar::China(ChinaMarket::Ib)` | 年別データ | 2004〜2026 | IB の振替営業日は 2005〜2026 年 | 1,871 | 0 | 0 | 差 0 | 未検証 |
| アジア太平洋 | `HongKong` | HKEx | `Calendar::HongKong` | 年別データ | 2004〜2025 |  | 2,201 | 0 | 0 | 差 0 | 未検証 |
| アジア太平洋 | `India` | NSE | `Calendar::India` | 年別データ | 2005〜2025 | 2024〜2025 年の Ramzan Id・Bakri Id・Id-E-Milad は推定とソースに記載 | 1,736 | 0 | 0 | 差 0 | 未検証 |
| アジア太平洋 | `Indonesia` | BEJ | `Calendar::Indonesia(IndonesiaMarket::Bej)` | 年別データ | 2005〜2014 |  | 1,358 | 0 | 0 | 差 0 | 未検証 |
| アジア太平洋 | `Indonesia` | JSX | `Calendar::Indonesia(IndonesiaMarket::Jsx)` | 年別データ | 2005〜2014 |  | 1,358 | 0 | 0 | 差 0 | 未検証 |
| アジア太平洋 | `Indonesia` | IDX | `Calendar::Indonesia(IndonesiaMarket::Idx)` | 年別データ | 2005〜2014 |  | 1,358 | 0 | 0 | R で作れない | 未検証 |
| アジア太平洋 | `Japan` | （区分なし） | `Calendar::Japan` | 規則と一度きりの休日 | 1959〜2021 | 春分・秋分は近似式 | 4,575 | 0 | 0 | 差 0 | 未検証 |
| アジア太平洋 | `NewZealand` | Wellington | `Calendar::NewZealand(NewZealandMarket::Wellington)` | 年別データ | 2022〜2052 | 年別は Matariki だけで、他は規則 | 3,252 | 0 | 0 | 差 0 | 未検証 |
| アジア太平洋 | `NewZealand` | Auckland | `Calendar::NewZealand(NewZealandMarket::Auckland)` | 年別データ | 2022〜2052 | 年別は Matariki だけで、他は規則 | 3,252 | 0 | 0 | 差 0 | 未検証 |
| アジア太平洋 | `Singapore` | SGX | `Calendar::Singapore` | 年別データ | 2004〜2026 |  | 1,337 | 0 | 0 | 差 0 | 未検証 |
| アジア太平洋 | `SouthKorea` | Settlement | `Calendar::SouthKorea(SouthKoreaMarket::Settlement)` | 年別データ | 2004〜2050 |  | 2,592 | 0 | 0 | 差 0 | 未検証 |
| アジア太平洋 | `SouthKorea` | KRX | `Calendar::SouthKorea(SouthKoreaMarket::Krx)` | 年別データ | 2004〜2050 |  | 2,893 | 0 | 0 | 差 0 | 未検証 |
| アジア太平洋 | `Taiwan` | TSEC | `Calendar::Taiwan` | 年別データ | 2002〜2026 |  | 1,100 | 0 | 0 | 差 0 | 未検証 |
| アジア太平洋 | `Thailand` | （区分なし） | `Calendar::Thailand` | 年別データ | 2000〜2025 |  | 3,879 | 0 | 0 | 差 0 | 未検証 |

**確かめていないこと**：実際の市場休日との一致。例外日は、日本での 54 件の照合（3.4 節）とトルコの簡単なテストだけで、全カレンダーでは確かめていない。Joint の例外日の扱いは 3.4 節の【設計案】のままである。

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

### 4.3 実装と QuantLib との照合結果【実行照合済み】

**実装（`crates/riemann/src/time/business_day.rs`）**

- `BusinessDayConvention`：Following、ModifiedFollowing、Preceding、ModifiedPreceding、Unadjusted、HalfMonthModifiedFollowing、Nearest。
- `HolidayCalendar` の関数として、`adjust`、`advance`、営業日の月末・月初（`end_of_month`・`is_end_of_month`・`start_of_month`・`is_start_of_month`）、`business_days_between` を持つ。例外日も判定に含まれる。
- 日付の範囲（1901-01-01〜2199-12-31）を越える調整は、QuantLib が例外にする場合と同じく `Err` を返す。
- 調整と日付の進め方の処理は `ql/time/calendar.cpp` から移したものとして扱い、ファイルの先頭に移植元と著作権表示を書いた（`THIRD_PARTY_NOTICES` にも記載）。

**照合結果**（2026-10-31、QuantLib R バインディング 1.42、`tools/golden/p1_business_days.R`、`cargo test --test golden_business_days`）

対象のカレンダーは、日本、TARGET、米国 SOFR、英国 Settlement の 4 つである。

| 対象 | 内容 | 件数 | 結果 |
|---|---|---|---|
| 調整 | 2023〜2026 年のすべての日と範囲の両端 15 日ずつ × 7 規約 | 41,748 件（QuantLib のエラー 21 件を含む） | すべて一致 |
| 営業日の月末・月初 | 同じ日付について、月末・月末かどうか・月初・月初かどうか | 5,964 件 | すべて一致 |
| 日付を進める | 39 の基準日（月末、営業日の月末、休日、範囲の端など）× 日・週・月・年の 29 通りの期間 × 7 規約 × 月末の扱いの有無（日単位は Following・月末なしのみ） | 47,112 件（QuantLib のエラー 2,281 件を含む） | すべて一致。エラーになる組み合わせも一致した |
| 営業日数 | 10 組の日付（同日、逆順、年をまたぐ組を含む）× 両端を含むかどうか 4 通り | 160 件 | すべて一致 |

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

### 5.3 実装と QuantLib との照合結果【実行照合済み】

**実装（`crates/riemann/src/time/schedule.rs`）**

- `ScheduleSpec`（開始日、終了日、期間、カレンダー、営業日調整、終了日の営業日調整、生成規則、月末の扱い、first date、next-to-last date）を受け取り、`Schedule { dates, is_regular }` を返す関数 `generate`。
- 生成規則は Backward・Forward・Zero を実装した。ThirdWednesday・Twentieth・IMM・CDS 系は `Error::Unsupported` を返す（CDS の段階で足す）。
- QuantLib との違い：
  - 開始日は必須とした。QuantLib は、開始日がなく Backward の場合に、グローバルな評価日から仮の開始日を作るが、Riemann はグローバル状態を持たない。
  - QuantLib の最後の安全確認には、日付が 2 つのときに `isRegular_[1]` を書き換える箇所がある（配列の範囲外）。Riemann は、その要素があるときだけ書き換える。
- 処理は `ql/time/schedule.cpp` から移したものとして扱い、ファイルの先頭に移植元と著作権表示を書いた（`THIRD_PARTY_NOTICES` にも記載）。

**照合結果**（2026-10-31、QuantLib R バインディング 1.42、`tools/golden/p1_schedules.R`、`cargo test --test golden_schedules`）

入力の組み合わせを、乱数の種を固定して 4,000 通り選んだ（開始日 14 種類、期間は日・週・月・年と 0、カレンダー 5 種類、営業日調整 5 種類、生成規則 3 種類、月末の扱いの有無、端数期間の指定と範囲外の指定）。

| 対象 | 件数 | 結果 |
|---|---|---|
| スケジュールが作られた組み合わせ（日付と `isRegular` を比較） | 2,003 件（first date あり 668 件、next-to-last date あり 646 件、月末の扱いあり 912 件、規則どおりでない期間を含むもの 1,441 件） | すべて一致 |
| QuantLib がエラーにした組み合わせ | 1,997 件 | Riemann もすべてエラーを返した。エラーの理由までは比べていない |

エラーの大半は、Zero の規則に端数期間を指定した場合と、範囲外の端数期間を意図的に指定した場合である。

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

### 6.3 実装と QuantLib との照合結果【実行照合済み】

**実装（`crates/riemann/src/time/day_count.rs`）**

- `DayCount`：Actual/360（最終日を含むかどうか）、Actual/364、Actual/365.25（同）、Actual/365 Fixed（Standard・Canadian・NoLeap）、Actual/366（同）、Actual/Actual（ISDA・ISMA・AFB）、30/360（USA・BondBasis・European・Italian・ISDA（終了日つき）・NASD）、30/365、1/1、Simple、Business/252（カレンダーつき）。QuantLib の別名（Bond = ISMA、Historical・Actual365 = ISDA、Euro = AFB、EurobondBasis = European、ISMA = BondBasis、German = ISDA）は同じ値に対応させた。
- `day_count(d1, d2)` と `year_fraction(d1, d2, ref_start, ref_end)`。参照期間が必要な種類で不正な参照期間を渡すと `Err` を返す。
- Business/252 は、QuantLib が月・年単位のキャッシュで足し合わせる営業日数を、キャッシュなしで `business_days_between` で数える（結果は下の照合で一致した）。
- 未実装：スケジュールを渡す Actual/Actual（ISMA/Bond）。QuantLib がスケジュールなしで使う参照期間の方式は実装した。
- 式は `ql/time/daycounters/` から移したものとして扱い、ファイルの先頭に移植元と著作権表示を書いた（`THIRD_PARTY_NOTICES` にも記載）。

**照合結果**（2026-10-31、QuantLib R バインディング 1.42、`tools/golden/p1_day_counts.R`、`cargo test --test golden_day_counts`）

- 日付の組：範囲全体から乱数（種を固定）で選んだ 2,000 組と、月末・2 月 28 日・29 日・30 日・31 日・範囲の両端を含む 20 日付の総当たり 400 組（同日・逆順を含む）。参照期間は、なし・1/3/6/12 か月の正しい期間・不正な期間を乱数で付けた。
- 日数計算 30 種類（別名を含む）。Business/252 は、間隔が 1,500 日以内の組に限った。

| 対象 | 件数 | 結果 |
|---|---|---|
| 日数（`dayCount`） | 70,660 件 | すべて完全一致 |
| 年数割合（`yearFraction`）が求まった組 | 67,747 件 | すべて完全一致（浮動小数の最後の桁まで。7.3 の許容誤差を使う必要はなかった） |
| QuantLib がエラーにした組 | 2,913 件（Actual/365 Canadian 1,221、Actual/Actual ISMA・Bond 各 797、AFB・Euro 各 46、ISDA・Historical・Actual365 各 2） | Riemann もすべてエラーを返した。エラーの理由までは比べていない |

ISDA の 2 件は、2199 年に始まる期間で、QuantLib が翌年 1 月 1 日（2200 年）を作れずにエラーにするものである。

## 7. QuantLib との照合計画（P1 で実施済み。結果は各章の照合結果の節）

基準値は `tools/golden/` の R スクリプトで、QuantLib の R バインディング（1.42）から CSV に書き出す。以下は、P1 を始める前に立てた照合の計画である。実施した内容と結果は、2.3、3.4、4.3、5.3、6.3 節に記録した。比べ方は仕様書 5.1 の許容誤差に従う。

| 対象 | 基準値の内容 | 一致の基準 |
|---|---|---|
| 日付 | 範囲内のすべての日（シリアル 367〜109574）の年・月・日・曜日・うるう年・暦の月末 | 完全一致 |
| 日付と期間の加減 | 月末・2月29日・年末を含む日付と、−120〜120 か月・年の組み合わせ。範囲外になる組み合わせは例外になることも確かめる | 完全一致 |
| 休日 | カレンダーごとに、1901〜2199 年のすべての日の営業日・休日の判定 | 完全一致 |
| 例外日 | 追加・取り消し・取り消しの取り消しを含む例。QuantLib の側は例外日が全体で共有されるので、例ごとに `resetAddedAndRemovedHolidays` で元に戻してから作る | 完全一致 |
| 営業日調整 | 7種類の規約 × 月末・連休・年末年始を含む日付 | 完全一致 |
| 日付を進める | 日・週・月・年 × 規約 × 月末の扱いの有無 | 完全一致 |
| スケジュール | 開始日・終了日・期間・生成規則・月末の扱い・端数期間の組み合わせ。日付と `isRegular` を比べる | 完全一致 |
| 日数計算 | 日付の組 × 種類・変種。日数と年数割合 | 日数は完全一致、年数割合は仕様書 5.1 の「四則演算だけの計算」の基準 |

## 8. ライセンス上の扱い

休日の規則は QuantLib のコードを読んで同じ判定を作るので、QuantLib のコードから派生したものとして扱う。カレンダーの規則を書くファイルには、元のファイル（`ql/time/calendars/*.cpp`）・コミット・QuantLib の著作権表示と修正 BSD ライセンスの条件を記載する。リポジトリに `THIRD_PARTY_NOTICES` を置き、QuantLib のライセンス全文を収める。
