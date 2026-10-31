# Golden values for Riemann P1 (schedule generation) from the QuantLib R binding.
# Usage (from the Riemann root): Rscript tools/golden/p1_schedules.R
# Output: tests/golden/schedules.csv
# Inputs are drawn with a fixed seed. dates and is_regular are space-separated
# (is_regular[i] refers to the period ending at dates[i+1]); ERROR = QuantLib raised an error.

suppressMessages(library(QuantLib))

out_dir <- file.path("tests", "golden")
dir.create(out_dir, recursive = TRUE, showWarnings = FALSE)
ql_version <- as.character(packageVersion("QuantLib"))
set.seed(20261031)

calendars <- list(
  japan = QuantLib::Japan(),
  target = QuantLib::TARGET(),
  us_sofr = QuantLib::UnitedStates("SOFR"),
  weekends_only = QuantLib::WeekendsOnly(),
  null_calendar = QuantLib::NullCalendar()
)
conventions <- c("Following", "ModifiedFollowing", "Preceding", "ModifiedPreceding", "Unadjusted")
rules <- c("Backward", "Forward", "Zero")
tenors <- list(c(1, "Months"), c(3, "Months"), c(6, "Months"), c(1, "Years"), c(2, "Months"),
               c(2, "Weeks"), c(10, "Days"), c(0, "Months"), c(18, "Months"))

s <- function(x) as.integer(QuantLib::Date_serialNumber(x))
mk <- function(y, m, d) QuantLib::Date(as.integer(d), month.name[m], as.integer(y))
from_serial <- function(n) QuantLib::Date(as.integer(n))

effectives <- c(
  s(mk(2024, 1, 31)), s(mk(2024, 2, 29)), s(mk(2023, 2, 28)), s(mk(2023, 11, 30)), s(mk(2024, 4, 29)),
  s(mk(2024, 3, 15)), s(mk(2024, 8, 31)), s(mk(2025, 5, 3)), s(mk(2025, 6, 30)), s(mk(2024, 12, 31)),
  s(mk(2025, 1, 2)), s(mk(2024, 9, 30)), s(mk(2026, 3, 20)), s(mk(2024, 6, 14))
)
years_ahead <- c(1, 2, 3, 5, 10)

n_cases <- 4000
rows <- vector("list", n_cases)
for (k in seq_len(n_cases)) {
  eff <- sample(effectives, 1)
  term_base <- s(QuantLib::Date___add__(from_serial(eff), QuantLib::Period(as.integer(sample(years_ahead, 1)), "Years")))
  term <- term_base + sample(c(0, 0, 0, -1, 1, -15, 17, -40), 1)
  tenor <- tenors[[sample(length(tenors), 1)]]
  cal_name <- sample(names(calendars), 1)
  conv <- sample(conventions, 1)
  term_conv <- sample(conventions, 1)
  rule <- sample(rules, 1)
  eom <- sample(c(FALSE, TRUE), 1)
  stub <- sample(c("none", "none", "first", "next_to_last", "both", "bad_first", "bad_next_to_last"), 1)
  first <- 0L; next_to_last <- 0L
  if (stub %in% c("first", "both")) first <- eff + sample(c(20, 45, 75, 100, 200), 1)
  if (stub %in% c("next_to_last", "both")) next_to_last <- term - sample(c(20, 45, 75, 100, 200), 1)
  if (stub == "bad_first") first <- eff - 10
  if (stub == "bad_next_to_last") next_to_last <- term + 10

  result <- tryCatch({
    sc <- QuantLib::Schedule(
      from_serial(eff), from_serial(term), QuantLib::Period(as.integer(tenor[1]), tenor[2]),
      calendars[[cal_name]], conv, term_conv, rule, eom,
      if (first > 0) from_serial(first) else QuantLib::Date(),
      if (next_to_last > 0) from_serial(next_to_last) else QuantLib::Date()
    )
    n <- QuantLib::Schedule_size(sc)
    dates <- vapply(seq_len(n) - 1L, function(i) s(QuantLib::Schedule_date(sc, i)), integer(1))
    regular <- vapply(seq_len(n - 1L), function(i) as.integer(QuantLib::Schedule_isRegular(sc, i)), integer(1))
    c(paste(dates, collapse = " "), paste(regular, collapse = " "))
  }, error = function(e) c("ERROR", "ERROR"))

  rows[[k]] <- data.frame(
    effective = eff, termination = term, tenor_n = as.integer(tenor[1]), tenor_unit = tenor[2],
    calendar = cal_name, convention = conv, termination_convention = term_conv, rule = rule,
    end_of_month = as.integer(eom), first_date = first, next_to_last_date = next_to_last,
    dates = result[1], is_regular = result[2]
  )
}
schedules <- do.call(rbind, rows)
path <- file.path(out_dir, "schedules.csv")
writeLines(c(
  paste0("# QuantLib R binding ", ql_version, "; Schedule(effective, termination, tenor, calendar, convention, ",
         "terminationConvention, rule, endOfMonth, firstDate, nextToLastDate); 0 = no first/next-to-last date"),
  paste(names(schedules), collapse = ",")
), path)
utils::write.table(schedules, path, sep = ",", row.names = FALSE, col.names = FALSE, append = TRUE, quote = FALSE)
cat("schedules:", nrow(schedules), "errors:", sum(schedules$dates == "ERROR"), "\n")
print(table(schedules$rule, schedules$dates == "ERROR"))
