# Golden values for Riemann P1 (holiday calendars) from the QuantLib R binding.
# Usage (from the Riemann root): Rscript tools/golden/p1_calendars.R
# Output: tests/golden/calendar_holidays.csv, tests/golden/calendar_adjustments.csv

suppressMessages(library(QuantLib))

out_dir <- file.path("tests", "golden")
dir.create(out_dir, recursive = TRUE, showWarnings = FALSE)
ql_version <- as.character(packageVersion("QuantLib"))

calendars <- list(
  japan = QuantLib::Japan(),
  target = QuantLib::TARGET(),
  us_settlement = QuantLib::UnitedStates("Settlement"),
  us_government_bond = QuantLib::UnitedStates("GovernmentBond"),
  us_sofr = QuantLib::UnitedStates("SOFR"),
  uk_settlement = QuantLib::UnitedKingdom("Settlement"),
  weekends_only = QuantLib::WeekendsOnly(),
  null_calendar = QuantLib::NullCalendar(),
  joint_japan_us_sofr_holidays = QuantLib::JointCalendar(QuantLib::Japan(), QuantLib::UnitedStates("SOFR"), "JoinHolidays"),
  joint_japan_us_sofr_business_days = QuantLib::JointCalendar(QuantLib::Japan(), QuantLib::UnitedStates("SOFR"), "JoinBusinessDays")
)

serials <- function(v) {
  n <- QuantLib::DateVector___len__(v)
  if (n == 0) return(integer())
  vapply(seq_len(n) - 1L, function(i) as.integer(QuantLib::Date_serialNumber(QuantLib::DateVector___getitem__(v, i))), integer(1))
}

first <- QuantLib::Date_minDate()
# holidayList() steps one day past its end date, so it cannot end at Date_maxDate();
# end one day earlier and check the last valid date separately.
last <- QuantLib::Date(30L, "December", 2199L)
max_date <- QuantLib::Date_maxDate()

# 1. Holidays other than weekends, for every calendar over the whole valid range.
holidays <- do.call(rbind, lapply(names(calendars), function(name) {
  cal <- calendars[[name]]
  s <- serials(QuantLib::Calendar_holidayList(cal, first, last, FALSE))
  if (QuantLib::Calendar_isHoliday(cal, max_date) &&
      !QuantLib::Calendar_isWeekend(cal, QuantLib::Date_weekday(max_date))) {
    s <- c(s, as.integer(QuantLib::Date_serialNumber(max_date)))
  }
  data.frame(calendar = rep(name, length(s)), serial = s)
}))
# Weekend days per calendar (weekday numbers, Sunday = 1).
weekday_names <- c("Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday")
weekends <- vapply(names(calendars), function(name) {
  paste(which(vapply(weekday_names, function(w) QuantLib::Calendar_isWeekend(calendars[[name]], w), logical(1))), collapse = " ")
}, character(1))
writeLines(c(
  paste0("# QuantLib R binding ", ql_version, "; holidayList(cal, minDate, 2199-12-30, includeWeekEnds = FALSE) plus a separate check of 2199-12-31"),
  paste0("# weekend: ", paste(names(weekends), weekends, sep = "=", collapse = "; ")),
  "calendar,serial"
), file.path(out_dir, "calendar_holidays.csv"))
utils::write.table(holidays, file.path(out_dir, "calendar_holidays.csv"), sep = ",",
                   row.names = FALSE, col.names = FALSE, append = TRUE, quote = FALSE)

# 2. Holiday exceptions on Japan: a sequence of add/remove operations.
#    QuantLib shares added/removed holidays across all Japan() instances, so reset first and last.
jp <- QuantLib::Japan()
QuantLib::Calendar_resetAddedAndRemovedHolidays(jp)
mk <- function(y, m, d) QuantLib::Date(as.integer(d), month.name[m], as.integer(y))
probe <- list(mk(2024, 1, 1), mk(2024, 3, 5), mk(2024, 3, 6), mk(2024, 3, 9), mk(2024, 3, 20), mk(2024, 5, 6))
ops <- list(
  list("add", mk(2024, 3, 5)),     # business day -> holiday
  list("remove", mk(2024, 1, 1)),  # genuine holiday -> business day
  list("add", mk(2024, 1, 1)),     # re-adding a removed genuine holiday reverts the removal
  list("remove", mk(2024, 3, 5)),  # removing an added holiday
  list("add", mk(2024, 3, 9)),     # Saturday: already a holiday, no change
  list("remove", mk(2024, 3, 6)),  # business day: nothing to remove
  list("remove", mk(2024, 3, 20)), # vernal equinox 2024
  list("remove", mk(2024, 3, 9)),  # removing a weekend day makes it a business day
  list("add", mk(2024, 5, 6))      # already a (substitute) holiday, no change
)
rows <- list()
for (k in seq_along(ops)) {
  op <- ops[[k]]
  if (op[[1]] == "add") QuantLib::Calendar_addHoliday(jp, op[[2]]) else QuantLib::Calendar_removeHoliday(jp, op[[2]])
  for (p in probe) {
    rows[[length(rows) + 1L]] <- data.frame(
      step = k, op = op[[1]], date = QuantLib::Date_serialNumber(op[[2]]),
      probe = QuantLib::Date_serialNumber(p), is_business_day = as.integer(QuantLib::Calendar_isBusinessDay(jp, p))
    )
  }
}
QuantLib::Calendar_resetAddedAndRemovedHolidays(jp)
adj <- do.call(rbind, rows)
writeLines(c(
  paste0("# QuantLib R binding ", ql_version, "; Japan calendar, operations applied cumulatively in step order"),
  "step,op,date,probe,is_business_day"
), file.path(out_dir, "calendar_adjustments.csv"))
utils::write.table(adj, file.path(out_dir, "calendar_adjustments.csv"), sep = ",",
                   row.names = FALSE, col.names = FALSE, append = TRUE, quote = FALSE)

cat("holidays:", nrow(holidays), "rows; adjustments:", nrow(adj), "rows\n")
print(table(holidays$calendar))
print(weekends)
