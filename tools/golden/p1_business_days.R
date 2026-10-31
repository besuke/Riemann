# Golden values for Riemann P1 (business day conventions) from the QuantLib R binding.
# Usage (from the Riemann root): Rscript tools/golden/p1_business_days.R
# Output: tests/golden/bdc_adjust.csv, bdc_month.csv, bdc_advance.csv, bdc_between.csv
# ERROR means that QuantLib raised an error (e.g. stepping outside the valid date range).

suppressMessages(library(QuantLib))

out_dir <- file.path("tests", "golden")
dir.create(out_dir, recursive = TRUE, showWarnings = FALSE)
ql_version <- as.character(packageVersion("QuantLib"))

calendars <- list(
  japan = QuantLib::Japan(),
  target = QuantLib::TARGET(),
  us_sofr = QuantLib::UnitedStates("SOFR"),
  uk_settlement = QuantLib::UnitedKingdom("Settlement")
)
conventions <- c("Following", "ModifiedFollowing", "Preceding", "ModifiedPreceding",
                 "Unadjusted", "HalfMonthModifiedFollowing", "Nearest")

s <- function(x) as.integer(QuantLib::Date_serialNumber(x))
from_serial <- function(n) QuantLib::Date(as.integer(n))
mk <- function(y, m, d) QuantLib::Date(as.integer(d), month.name[m], as.integer(y))
safe <- function(expr) tryCatch(as.character(expr), error = function(e) "ERROR")
write_golden <- function(df, file, note) {
  path <- file.path(out_dir, file)
  writeLines(c(paste0("# QuantLib R binding ", ql_version, "; ", note), paste(names(df), collapse = ",")), path)
  utils::write.table(df, path, sep = ",", row.names = FALSE, col.names = FALSE, append = TRUE, quote = FALSE)
}

# Dates: every day of 2023-2026, plus both ends of the valid range.
dates <- c(s(mk(2023, 1, 1)):s(mk(2026, 12, 31)), 367:381, 109560:109574)

# 1. adjust(date, convention)
adjust <- do.call(rbind, lapply(names(calendars), function(name) {
  cal <- calendars[[name]]
  do.call(rbind, lapply(conventions, function(conv) {
    data.frame(calendar = name, convention = conv, serial = dates,
               result = vapply(dates, function(n) safe(s(QuantLib::Calendar_adjust(cal, from_serial(n), conv))), ""))
  }))
}))
write_golden(adjust, "bdc_adjust.csv", "Calendar::adjust(date, convention)")

# 2. Business-day start and end of month
month <- do.call(rbind, lapply(names(calendars), function(name) {
  cal <- calendars[[name]]
  data.frame(
    calendar = name, serial = dates,
    end_of_month = vapply(dates, function(n) safe(s(QuantLib::Calendar_endOfMonth(cal, from_serial(n)))), ""),
    is_end_of_month = vapply(dates, function(n) safe(as.integer(QuantLib::Calendar_isEndOfMonth(cal, from_serial(n)))), ""),
    start_of_month = vapply(dates, function(n) safe(s(QuantLib::Calendar_startOfMonth(cal, from_serial(n)))), ""),
    is_start_of_month = vapply(dates, function(n) safe(as.integer(QuantLib::Calendar_isStartOfMonth(cal, from_serial(n)))), "")
  )
}))
write_golden(month, "bdc_month.csv", "Calendar::endOfMonth/isEndOfMonth/startOfMonth/isStartOfMonth")

# 3. advance(date, n, unit, convention, endOfMonth)
base <- c(
  # calendar month ends and nearby days, business month ends, holidays, February
  s(mk(2024, 1, 31)), s(mk(2024, 2, 29)), s(mk(2023, 2, 28)), s(mk(2024, 3, 29)), s(mk(2024, 3, 31)),
  s(mk(2024, 4, 29)), s(mk(2024, 4, 30)), s(mk(2024, 5, 31)), s(mk(2024, 6, 28)), s(mk(2024, 6, 30)),
  s(mk(2024, 8, 30)), s(mk(2024, 8, 31)), s(mk(2024, 9, 30)), s(mk(2024, 11, 29)), s(mk(2024, 12, 31)),
  s(mk(2025, 1, 1)), s(mk(2025, 1, 2)), s(mk(2025, 1, 31)), s(mk(2025, 2, 28)), s(mk(2025, 4, 18)),
  s(mk(2025, 4, 30)), s(mk(2025, 5, 5)), s(mk(2025, 5, 30)), s(mk(2025, 7, 15)), s(mk(2025, 8, 15)),
  s(mk(2025, 10, 31)), s(mk(2025, 12, 24)), s(mk(2025, 12, 31)), s(mk(2026, 1, 30)), s(mk(2026, 2, 27)),
  s(mk(2026, 3, 20)), s(mk(2026, 9, 21)), s(mk(2026, 10, 9)), s(mk(2026, 11, 30)), s(mk(2026, 12, 31)),
  # range ends
  367L, 380L, 109560L, 109574L
)
steps <- list(Days = c(-10, -3, -1, 0, 1, 3, 10, 250), Weeks = c(-4, -1, 1, 4),
              Months = c(-24, -12, -6, -3, -1, 1, 2, 3, 6, 12, 24), Years = c(-5, -1, 1, 2, 5, 10))
advance <- list()
for (name in names(calendars)) {
  cal <- calendars[[name]]
  for (unit in names(steps)) for (n in steps[[unit]]) {
    convs <- if (unit == "Days") "Following" else conventions
    eoms <- if (unit == "Days") FALSE else c(FALSE, TRUE)
    for (conv in convs) for (eom in eoms) {
      advance[[length(advance) + 1L]] <- data.frame(
        calendar = name, serial = base, n = n, unit = unit, convention = conv, end_of_month = as.integer(eom),
        result = vapply(base, function(b) safe(s(QuantLib::Calendar_advance(cal, from_serial(b), as.integer(n), unit, conv, eom))), "")
      )
    }
  }
}
advance <- do.call(rbind, advance)
write_golden(advance, "bdc_advance.csv", "Calendar::advance(date, n, unit, convention, endOfMonth)")

# 4. businessDaysBetween(from, to, includeFirst, includeLast)
pairs <- rbind(
  data.frame(from = s(mk(2024, 4, 26)), to = c(s(mk(2024, 4, 26)), s(mk(2024, 4, 29)), s(mk(2024, 5, 7)), s(mk(2024, 12, 31)))),
  data.frame(from = s(mk(2025, 12, 24)), to = c(s(mk(2025, 12, 20)), s(mk(2026, 1, 5)), s(mk(2027, 1, 4)))),
  data.frame(from = s(mk(2024, 5, 6)), to = c(s(mk(2024, 5, 6)), s(mk(2024, 5, 3)), s(mk(2023, 1, 1))))
)
between <- list()
for (name in names(calendars)) for (inc_first in c(FALSE, TRUE)) for (inc_last in c(FALSE, TRUE)) {
  cal <- calendars[[name]]
  between[[length(between) + 1L]] <- data.frame(
    calendar = name, from = pairs$from, to = pairs$to,
    include_first = as.integer(inc_first), include_last = as.integer(inc_last),
    result = mapply(function(f, t) safe(QuantLib::Calendar_businessDaysBetween(cal, from_serial(f), from_serial(t), inc_first, inc_last)),
                    pairs$from, pairs$to)
  )
}
between <- do.call(rbind, between)
write_golden(between, "bdc_between.csv", "Calendar::businessDaysBetween(from, to, includeFirst, includeLast)")

cat("adjust:", nrow(adjust), "month:", nrow(month), "advance:", nrow(advance), "between:", nrow(between), "\n")
cat("errors: adjust", sum(adjust$result == "ERROR"), "advance", sum(advance$result == "ERROR"),
    "month", sum(month$end_of_month == "ERROR" | month$start_of_month == "ERROR"), "\n")
