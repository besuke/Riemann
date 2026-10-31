# Golden values for Riemann P1 (day count conventions) from the QuantLib R binding.
# Usage (from the Riemann root): Rscript tools/golden/p1_day_counts.R
# Output: tests/golden/day_counts.csv
# ref_start/ref_end = 0 means no reference period; year_fraction is printed with 17 significant
# digits; ERROR = QuantLib raised an error.

suppressMessages(library(QuantLib))

out_dir <- file.path("tests", "golden")
dir.create(out_dir, recursive = TRUE, showWarnings = FALSE)
ql_version <- as.character(packageVersion("QuantLib"))
set.seed(20261031)

s <- function(x) as.integer(QuantLib::Date_serialNumber(x))
mk <- function(y, m, d) QuantLib::Date(as.integer(d), month.name[m], as.integer(y))
from_serial <- function(n) if (n == 0) QuantLib::Date() else QuantLib::Date(as.integer(n))
termination <- mk(2024, 2, 29)

day_counters <- list(
  actual_360 = QuantLib::Actual360(), actual_360_inc = QuantLib::Actual360(TRUE),
  actual_364 = QuantLib::Actual364(),
  actual_36525 = QuantLib::Actual36525(), actual_36525_inc = QuantLib::Actual36525(TRUE),
  actual_366 = QuantLib::Actual366(), actual_366_inc = QuantLib::Actual366(TRUE),
  actual_365_fixed = QuantLib::Actual365Fixed(), actual_365_canadian = QuantLib::Actual365Fixed("Canadian"),
  actual_365_no_leap = QuantLib::Actual365Fixed("NoLeap"),
  act_act_isda = QuantLib::ActualActual("ISDA"), act_act_historical = QuantLib::ActualActual("Historical"),
  act_act_actual365 = QuantLib::ActualActual("Actual365"),
  act_act_isma = QuantLib::ActualActual("ISMA"), act_act_bond = QuantLib::ActualActual("Bond"),
  act_act_afb = QuantLib::ActualActual("AFB"), act_act_euro = QuantLib::ActualActual("Euro"),
  thirty_360_usa = QuantLib::Thirty360("USA"), thirty_360_bond_basis = QuantLib::Thirty360("BondBasis"),
  thirty_360_european = QuantLib::Thirty360("European"), thirty_360_eurobond_basis = QuantLib::Thirty360("EurobondBasis"),
  thirty_360_italian = QuantLib::Thirty360("Italian"), thirty_360_german = QuantLib::Thirty360("German", termination),
  thirty_360_isma = QuantLib::Thirty360("ISMA"), thirty_360_isda = QuantLib::Thirty360("ISDA", termination),
  thirty_360_nasd = QuantLib::Thirty360("NASD"), thirty_365 = QuantLib::Thirty365(),
  one = QuantLib::OneDayCounter(), simple = QuantLib::SimpleDayCounter(),
  business_252_japan = QuantLib::Business252(QuantLib::Japan())
)

# Date pairs: random over the valid range, plus edge cases.
min_s <- 367L; max_s <- 109574L
d1r <- sample(min_s:(max_s - 4000L), 2000, replace = TRUE)
d2r <- pmin(max_s, pmax(min_s, d1r + sample(c(-400:-1, 1:4000), 2000, replace = TRUE)))
edge_dates <- c(mk(2023, 2, 28), mk(2024, 2, 28), mk(2024, 2, 29), mk(2023, 3, 31), mk(2024, 1, 30), mk(2024, 1, 31),
                mk(2024, 3, 30), mk(2024, 3, 31), mk(2024, 8, 31), mk(2024, 9, 30), mk(2025, 2, 28), mk(2025, 3, 1),
                mk(2024, 12, 31), mk(2025, 1, 1), mk(2100, 2, 28), mk(2000, 2, 29),
                mk(1901, 1, 1), mk(1901, 6, 30), mk(2199, 6, 1), mk(2199, 12, 31))
edge <- expand.grid(a = vapply(edge_dates, s, integer(1)), b = vapply(edge_dates, s, integer(1)))
pairs <- data.frame(d1 = c(d1r, edge$a), d2 = c(d2r, edge$b))

# Reference periods (used by Actual/Actual ISMA/Bond and Actual/365 Canadian; ignored by the others):
# none, a period around d1 of 1, 3, 6 or 12 months, or an invalid one.
ref_for <- function(d1, d2) {
  kind <- sample(c("none", "regular", "regular", "invalid"), 1)
  if (kind == "none") return(c(0L, 0L))
  if (kind == "invalid") { st <- min(max(d1, d2) + 10L, max_s); return(c(st, st - 5L)) }
  start <- min(d1, d2) - sample(0:60, 1)
  months <- sample(c(1L, 3L, 6L, 12L), 1)
  end <- tryCatch(s(QuantLib::Date___add__(from_serial(start), QuantLib::Period(months, "Months"))), error = function(e) 0L)
  if (end == 0L) c(0L, 0L) else c(start, end)
}
refs <- t(mapply(ref_for, pairs$d1, pairs$d2))
pairs$ref_start <- refs[, 1]; pairs$ref_end <- refs[, 2]

fmt <- function(x) sprintf("%.17g", x)
rows <- lapply(names(day_counters), function(name) {
  dc <- day_counters[[name]]
  p <- if (name == "business_252_japan") pairs[abs(pairs$d2 - pairs$d1) <= 1500, ] else pairs
  res <- t(mapply(function(a, b, rs, re) {
    dcount <- tryCatch(as.character(QuantLib::DayCounter_dayCount(dc, from_serial(a), from_serial(b))), error = function(e) "ERROR")
    yf <- tryCatch(fmt(QuantLib::DayCounter_yearFraction(dc, from_serial(a), from_serial(b), from_serial(rs), from_serial(re))),
                   error = function(e) "ERROR")
    c(dcount, yf)
  }, p$d1, p$d2, p$ref_start, p$ref_end))
  data.frame(day_counter = name, d1 = p$d1, d2 = p$d2, ref_start = p$ref_start, ref_end = p$ref_end,
             day_count = res[, 1], year_fraction = res[, 2])
})
out <- do.call(rbind, rows)
path <- file.path(out_dir, "day_counts.csv")
writeLines(c(
  paste0("# QuantLib R binding ", ql_version, "; DayCounter::dayCount(d1, d2) and yearFraction(d1, d2, refStart, refEnd); ",
         "German and ISDA 30/360 use termination date ", s(termination)),
  paste(names(out), collapse = ",")
), path)
utils::write.table(out, path, sep = ",", row.names = FALSE, col.names = FALSE, append = TRUE, quote = FALSE)
cat("rows:", nrow(out), "errors (year_fraction):", sum(out$year_fraction == "ERROR"), "\n")
print(tapply(out$year_fraction == "ERROR", out$day_counter, sum))
