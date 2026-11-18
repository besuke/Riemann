# Dates of an OIS made from a tenor, through the public function. The complete comparison with
# QuantLib's MakeOIS (13,520 cases) is crates/rmql/tests/golden_ois_swap_dates.rs; here are
# a few of its rows (QuantLib serial numbers: days since 1899-12-30) and the R-side behaviour.

from_serial <- function(x) as.Date(x, origin = "1899-12-30")

test_that("rows of the QuantLib comparison are reproduced", {
  # index, evaluation, tenor, settlement days, effective, maturity (QuantLib's reported date)
  cases <- list(
    list("sofr", 46385, "1M", 2L, 46387, 46416),
    list("tonar", 46385, "1D", 3L, 46392, 46393),
    list("tonar", 46376, "1W", 3L, 46380, 46386), # the plain date 46387 is a holiday and moves to 46386
    list("tonar", 46275, "7Y", 2L, 46279, 48836)  # Carmine Studio: 2026-09-10
  )
  for (case in cases) {
    dates <- compute_ois_swap_dates(case[[1]], from_serial(case[[2]]), case[[3]], case[[4]])
    expect_equal(dates$effective_date, from_serial(case[[5]]), info = paste(case[[1]], case[[3]]))
    expect_equal(dates$maturity_date, from_serial(case[[6]]), info = paste(case[[1]], case[[3]]))
  }
})

test_that("a 7Y TONAR swap from 2026-09-10 starts on 2026-09-14 and ends on 2033-09-14", {
  dates <- compute_ois_swap_dates("tonar", "2026-09-10", "7Y")
  expect_s3_class(dates, "tbl_df")
  expect_named(dates, c("effective_date", "termination_date", "maturity_date"))
  expect_equal(dates$effective_date, as.Date("2026-09-14"))
  expect_equal(dates$termination_date, as.Date("2033-09-14"))
  expect_equal(dates$maturity_date, as.Date("2033-09-14"))
})

test_that("the dates make a swap, and an end of month start keeps the end of month", {
  dates <- compute_ois_swap_dates("tonar", "2026-09-10", "7Y")
  swap <- create_ois_swap("payer", 1e10, dates$effective_date, dates$termination_date, "tonar", 0.012)
  expect_s3_class(swap, "rmql_ois_swap")
  # 2026-11-30 is a Monday and the last business day of its month, so 1M ends on the last business day
  # of December (the end of month of a calendar is its last business day): 12-30, as 12-31 is a holiday.
  month_end <- compute_ois_swap_dates("tonar", "2026-11-26", "1M", settlement_days = 2L)
  expect_equal(month_end$effective_date, as.Date("2026-11-30"))
  expect_equal(month_end$termination_date, as.Date("2026-12-30"))
  expect_equal(month_end$maturity_date, as.Date("2026-12-30"))
  not_end <- compute_ois_swap_dates("tonar", "2026-11-25", "1M", settlement_days = 2L)
  expect_equal(not_end$effective_date, as.Date("2026-11-27"))
  expect_equal(not_end$termination_date, as.Date("2026-12-27"))
  expect_equal(not_end$maturity_date, as.Date("2026-12-28")) # 12-27 is a Sunday
})

test_that("tenors and evaluation dates recycle, and settlement days are counted from a business day", {
  dates <- compute_ois_swap_dates("sofr", "2026-09-10", c("1M", "1Y", "5Y"))
  expect_equal(nrow(dates), 3L)
  expect_equal(length(unique(dates$effective_date)), 1L)
  both <- compute_ois_swap_dates("tonar", c("2026-09-10", "2026-09-12"), "1Y", settlement_days = 0L)
  expect_equal(both$effective_date, as.Date(c("2026-09-10", "2026-09-14"))) # 09-12 is a Saturday
  expect_error(compute_ois_swap_dates("tonar", c("2026-09-10", "2026-09-11"), c("1M", "2M", "3M")), "recycle")
})

test_that("invalid input is reported", {
  expect_error(compute_ois_swap_dates("libor", "2026-09-10", "7Y"), "overnight index")
  expect_error(compute_ois_swap_dates("tonar", "2026-09-10", "7 years"), "period")
  expect_error(compute_ois_swap_dates("tonar", "2026-09-10", "7Y", settlement_days = -1L), "negative")
  expect_error(compute_ois_swap_dates("tonar", "2026-09-10", 7), "strings")
  expect_error(compute_ois_swap_dates("tonar", character(), "7Y"), "empty")
  expect_warning(
    bad <- compute_ois_swap_dates("tonar", "2026-09-10", c("7Y", "x"), on_error = "na"),
    "failed"
  )
  expect_equal(is.na(bad$effective_date), c(FALSE, TRUE))
})

test_that("a one-day swap that QuantLib cannot build is refused by create_ois_swap as well", {
  # 3 settlement days from 46381 start on 2026-12-30, and the day after is the holiday 12-31.
  dates <- compute_ois_swap_dates("tonar", from_serial(46381), "1D", settlement_days = 3L)
  expect_error(create_ois_swap("payer", 1, dates$effective_date, dates$termination_date, "tonar", 0.01))
})
