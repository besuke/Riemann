# Golden values for Riemann P1 (dates) from the QuantLib R binding.
# Usage (from the Riemann root): Rscript tools/golden/p1_dates.R
# Output: tests/golden/dates_all.csv, tests/golden/dates_period.csv

suppressMessages(library(QuantLib))

out_dir <- file.path("tests", "golden")
dir.create(out_dir, recursive = TRUE, showWarnings = FALSE)
ql_version <- as.character(packageVersion("QuantLib"))

weekday_number <- c(Sunday = 1L, Monday = 2L, Tuesday = 3L, Wednesday = 4L,
                    Thursday = 5L, Friday = 6L, Saturday = 7L)
month_number <- setNames(1:12, month.name)

date_from_serial <- function(serial) QuantLib::Date(as.integer(serial))

describe <- function(d) {
  c(
    serial = QuantLib::Date_serialNumber(d),
    year = QuantLib::Date_year(d),
    month = unname(month_number[QuantLib::Date_month(d)]),
    day = QuantLib::Date_dayOfMonth(d),
    weekday = unname(weekday_number[QuantLib::Date_weekday(d)]),
    is_leap = as.integer(QuantLib::Date_isLeap(QuantLib::Date_year(d))),
    is_end_of_month = as.integer(QuantLib::Date_isEndOfMonth(d)),
    end_of_month = QuantLib::Date_serialNumber(QuantLib::Date_endOfMonth(d))
  )
}

# 1. Every date in the valid range.
min_serial <- QuantLib::Date_serialNumber(QuantLib::Date_minDate())
max_serial <- QuantLib::Date_serialNumber(QuantLib::Date_maxDate())
all_dates <- do.call(rbind, lapply(min_serial:max_serial, function(s) describe(date_from_serial(s))))
all_dates <- as.data.frame(all_dates)
writeLines(c(
  paste0("# QuantLib R binding ", ql_version, "; all dates from Date_minDate() to Date_maxDate()"),
  paste(names(all_dates), collapse = ",")
), file.path(out_dir, "dates_all.csv"))
utils::write.table(all_dates, file.path(out_dir, "dates_all.csv"), sep = ",",
                   row.names = FALSE, col.names = FALSE, append = TRUE)

# 2. Date + Period, including month ends, February 29 and the range limits.
base <- list(
  c(31, 1, 2024), c(30, 1, 2023), c(29, 2, 2024), c(28, 2, 2023), c(28, 2, 2024),
  c(31, 3, 2023), c(30, 4, 2023), c(31, 5, 2024), c(15, 6, 2025), c(31, 8, 2025),
  c(30, 9, 2025), c(31, 10, 2026), c(30, 11, 2026), c(31, 12, 2026),
  c(1, 1, 1901), c(31, 1, 1901), c(31, 12, 2199), c(29, 2, 2196), c(15, 7, 2199)
)
units <- c(Days = "Days", Weeks = "Weeks", Months = "Months", Years = "Years")
steps <- list(
  Days = c(-40000, -1000, -366, -365, -31, -1, 0, 1, 31, 365, 366, 1000, 40000),
  Weeks = c(-520, -53, -52, -1, 0, 1, 52, 53, 520),
  Months = -120:120,
  Years = -120:120
)
rows <- list()
for (b in base) {
  d <- QuantLib::Date(as.integer(b[1]), month.name[b[2]], as.integer(b[3]))
  s0 <- QuantLib::Date_serialNumber(d)
  for (u in names(units)) for (n in steps[[u]]) {
    res <- tryCatch(
      QuantLib::Date_serialNumber(QuantLib::Date___add__(d, QuantLib::Period(as.integer(n), units[[u]]))),
      error = function(e) NA_integer_
    )
    rows[[length(rows) + 1L]] <- data.frame(serial = s0, n = n, unit = u, result = res)
  }
}
period <- do.call(rbind, rows)
period$result <- ifelse(is.na(period$result), "ERROR", as.character(period$result))
writeLines(c(
  paste0("# QuantLib R binding ", ql_version, "; Date + Period(n, unit); ERROR = QuantLib raised an error"),
  "serial,n,unit,result"
), file.path(out_dir, "dates_period.csv"))
utils::write.table(period, file.path(out_dir, "dates_period.csv"), sep = ",",
                   row.names = FALSE, col.names = FALSE, append = TRUE, quote = FALSE)

cat("dates_all:", nrow(all_dates), "rows; dates_period:", nrow(period), "rows;",
    sum(period$result == "ERROR"), "errors\n")
