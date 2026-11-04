# Dates, schedules, day counts and cash-flow values. Synthetic inputs; values from QuantLib's rules.

test_that("calendars adjust and advance like QuantLib", {
  expect_equal(compute_adjusted_date("target", as.Date("2023-05-01")), as.Date("2023-05-02"))
  expect_equal(compute_adjusted_date("us_government_bond", as.Date("2023-06-19"), "preceding"), as.Date("2023-06-16"))
  expect_equal(compute_advanced_date("target", as.Date("2023-06-19"), c("7D")), as.Date("2023-06-28"))
  expect_equal(compute_end_of_month("target", as.Date("2023-05-01")), as.Date("2023-05-31"))
  expect_error(compute_adjusted_date("nowhere", as.Date("2023-05-01")), "calendar")
})

test_that("an optional date is the same not given and NA, and is used when given", {
  none <- generate_schedule("2021-02-11", "2025-05-11", "6M", "target", rule = "forward")
  expect_identical(generate_schedule("2021-02-11", "2025-05-11", "6M", "target", rule = "forward", first_date = NA), none)
  long <- generate_schedule("2021-02-11", "2025-05-11", "6M", "target", rule = "forward", first_date = "2021-11-11")
  expect_equal(long$date[2], as.Date("2021-11-11"))
  expect_false(long$is_regular[1])
  expect_true(is.na(tail(long$is_regular, 1)))
  yf <- compute_year_fraction("act_act_isma", "2018-01-01", "2018-01-15", c("2018-01-01", NA), c("2018-07-01", NA))
  expect_equal(yf, c(14 / 181 / 2, 14 / 365), tolerance = 1e-15)
})

test_that("cash flows are valued on a curve, without the ones that occurred", {
  curve <- create_flat_curve("2025-01-02", 0.04, "actual_365_fixed")
  flows <- tibble::tibble(date = as.Date(c("2025-01-02", "2026-01-02", "2027-01-02")), amount = c(5, 5, 105),
                          nominal = c(100, 100, NA), accrual = c(1, 1, NA))
  v <- compute_cashflow_npv(flows, curve)
  df <- exp(-0.04 * c(365, 730) / 365)
  expect_equal(v$npv, 5 * df[1] + 105 * df[2], tolerance = 1e-13)
  expect_equal(v$bps, 100 * 1 * df[1] * 1e-4, tolerance = 1e-15)
  expect_equal(compute_cashflow_npv(flows, curve, include_settlement_cashflows = TRUE)$npv, 5 + v$npv, tolerance = 1e-13)
  expect_equal(get_curve_reference_date(curve), as.Date("2025-01-02"))
  expect_error(compute_cashflow_npv(tibble::tibble(x = 1), curve), "needs `date`")
})
