# Curves given directly. Synthetic inputs; the values are QuantLib's closed forms.

test_that("a flat curve discounts at its rate in its compounding", {
  continuous <- create_flat_curve("2024-10-03", 0.05, "actual_360")
  expect_equal(compute_discount(continuous, as.Date("2029-10-03")), exp(-0.05 * 1826 / 360), tolerance = 1e-14)
  annual <- create_flat_curve("2024-10-03", 0.05, "actual_365_fixed", "compounded", 1L)
  expect_equal(compute_discount(annual, as.Date("2026-10-03")), 1.05^(-730 / 365), tolerance = 1e-14)
})

test_that("a discount curve reproduces its nodes and a zero curve its rates", {
  dates <- as.Date("2019-05-02") + c(0, 365, 731)
  curve <- create_discount_curve(dates, c(1, 0.99, 0.97), "actual_360")
  expect_equal(compute_discount(curve, dates), c(1, 0.99, 0.97), tolerance = 1e-14)
  expect_equal(get_discount_nodes(curve)$discount, c(1, 0.99, 0.97))
  zero <- create_zero_curve(dates, c(0.01, 0.01, 0.02), "actual_365_fixed")
  expect_equal(compute_zero_rate(zero, dates[3], "actual_365_fixed", "continuous"), 0.02, tolerance = 1e-14)
})

test_that("a spread shifts the zero rates, and a given curve has no quote sensitivities", {
  base <- create_flat_curve("2024-01-02", 0.03, "actual_365_fixed")
  shifted <- create_zero_spreaded_curve(base, 0.01)
  expect_equal(compute_zero_rate(shifted, as.Date("2030-01-02"), "actual_365_fixed", "continuous"), 0.04, tolerance = 1e-14)
  expect_error(get_discount_nodes(base), "not given by discount factors")
  bond <- create_fixed_rate_bond(2L, 100, "2023-01-02", "2033-01-02", "6M", "japan", "following", "following", "backward", FALSE,
                                 0.01, "act_act_isma", "following", 100)
  expect_error(compute_quote_sensitivity(bond, base, "2024-01-02"), "not bootstrapped")
  expect_error(create_discount_curve(as.Date("2024-01-02") + 0:1, c(0.99, 0.98), "actual_360"), "first discount")
  expect_output(print(base), "flat curve")
})

test_that("a piecewise spread is linear between its dates and flat outside them", {
  base <- create_flat_curve("2024-01-02", 0.03, "actual_365_fixed")
  dates <- as.Date(c("2025-01-01", "2027-01-01"))
  tilted <- with_extrapolation(create_zero_spreaded_curve(base, c(0.01, -0.01), spread_dates = dates))
  zero <- function(curve, d) compute_zero_rate(curve, as.Date(d), "actual_365_fixed", "continuous")
  expect_equal(zero(tilted, "2024-06-01"), 0.04, tolerance = 1e-12)
  expect_equal(zero(tilted, "2026-01-01"), 0.03, tolerance = 1e-12)
  expect_equal(zero(tilted, "2030-01-01"), 0.02, tolerance = 1e-12)
  # a constant spread through two dates is the plain zero-spreaded curve
  same <- with_extrapolation(create_zero_spreaded_curve(base, c(0.01, 0.01), spread_dates = dates))
  expect_equal(compute_discount(same, as.Date("2031-05-05")), compute_discount(create_zero_spreaded_curve(base, 0.01), as.Date("2031-05-05")), tolerance = 1e-15)
  expect_error(create_zero_spreaded_curve(base, c(0.01, 0.02)), "one number")
  expect_error(compute_discount(create_zero_spreaded_curve(base, c(0, 0), spread_dates = dates), as.Date("2028-01-01")), "past max curve time")
})

test_that("a curve from bond prices reprices them, in either interpolation", {
  today <- as.Date("2026-11-02")
  bond <- function(years, coupon) create_fixed_rate_bond(1L, 100, today, seq(today, by = paste(years, "years"), length.out = 2)[2], "6M",
    "weekends_only", "unadjusted", "unadjusted", "backward", FALSE, coupon, "thirty_360_bond_basis", "following", 100)
  bonds <- purrr::map2(c(5, 1, 2), c(0.03, 0.01, 0.02), bond)
  purrr::walk(c("linear_zero", "log_linear_discount"), \(interpolation) {
    curve <- bootstrap_bond_curve(today, bonds, c(101, 99.8, 100), interpolation = interpolation)
    expect_lt(max(abs(curve$repricing$implied_price - curve$repricing$price)), 1e-9)
    v <- purrr::map_dbl(bonds, \(b) compute_bond_value_on_curve(b, curve, today)$clean_price)
    expect_equal(v, c(101, 99.8, 100), tolerance = 1e-10)
  })
  expect_error(bootstrap_bond_curve(today, bonds, c(101, 99.8)), "one price per bond")
  expect_error(bootstrap_bond_curve(today, list(1), 100), "create_fixed_rate_bond")
  expect_error(bootstrap_bond_curve(today, bonds, c(101, 99.8, 100), interpolation = "cubic"), "interpolation")
})
