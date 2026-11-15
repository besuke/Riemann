# IRRBB shock scenarios (SRP31.90-31.93) through the public functions. Synthetic inputs.

irrbb_curve <- function() {
  bootstrap_ois_curve("2026-10-06", "2026-10-06", "tonar", "actual_365_fixed",
                      create_ois_instruments(c("1Y", "2Y", "5Y", "10Y", "20Y", "30Y"), c(0.008, 0.009, 0.011, 0.015, 0.02, 0.022), 2L, 1L, 0L))
}
irrbb_dates <- function(curve) {
  last <- max(get_quote_repricing(curve)$pillar_date)
  unique(c(seq(as.Date("2026-10-06"), last, by = "month"), last))
}

test_that("the yen shocks: parallel +-100 bp at every tenor, steepener +25.4 bp and flattener -1.6 bp at 3.5 years", {
  jpy <- get_irrbb_shock_sizes("JPY")
  expect_equal(unlist(jpy), c(parallel_bp = 100, short_bp = 100, long_bp = 100))
  t <- c(0, 0.25, 1, 3.5, 10, 25, 30)
  expect_equal(compute_irrbb_shocks(t, "parallel_up", jpy), rep(100, length(t)))
  expect_equal(compute_irrbb_shocks(t, "parallel_down", jpy), rep(-100, length(t)))
  expect_equal(round(compute_irrbb_shocks(3.5, "steepener", jpy), 1), 25.4)
  expect_equal(round(compute_irrbb_shocks(3.5, "flattener", jpy), 1), -1.6)
  expect_equal(round(compute_irrbb_shocks(3.5, "short_up", jpy), 1), 41.7)
  e <- exp(-t / 4)
  expect_equal(compute_irrbb_shocks(t, "steepener", jpy), -65 * e + 90 * (1 - e), tolerance = 1e-13)
  expect_equal(compute_irrbb_shocks(t, "flattener", jpy), 80 * e - 60 * (1 - e), tolerance = 1e-13)
  expect_equal(compute_irrbb_shocks(t, "short_down", jpy), -100 * e, tolerance = 1e-13)
  expect_equal(unlist(get_irrbb_shock_sizes("USD")), c(parallel_bp = 200, short_bp = 300, long_bp = 225))
  expect_error(get_irrbb_shock_sizes("XXX"), "no IRRBB shock sizes")
})

test_that("the shocked curve is the base zero rates plus the shock, and the floor is applied", {
  curve <- irrbb_curve()
  dates <- irrbb_dates(curve)
  p <- compute_irrbb_curve_points(curve, "steepener", dates)
  base <- compute_zero_rate(curve, dates[-1], "actual_365_fixed", "continuous")
  expect_equal(p$base_zero[-1], base, tolerance = 1e-14)
  expect_equal(p$shock_bp, compute_irrbb_shocks(p$time, "steepener", get_irrbb_shock_sizes("JPY")), tolerance = 1e-12)
  shocked <- create_irrbb_shocked_curve(curve, "steepener", dates)
  expect_equal(compute_zero_rate(shocked, dates[-1], "actual_365_fixed", "continuous"), p$shocked_zero[-1], tolerance = 1e-12)
  # parallel down from rates below 1%: a 0% floor keeps them at 0, and a rate already below the floor stays
  down <- compute_irrbb_curve_points(curve, "parallel_down", dates, floor = 0)
  expect_true(all(down$shocked_zero >= pmin(down$base_zero, 0) - 1e-15))
  expect_true(any(down$floored))
  expect_error(compute_irrbb_curve_points(curve, "parallel_up", dates, floor = 0.001), "not be greater than zero")
})

test_that("bond values on the shocked curve are the discounted cash flows, and a parallel rise lowers them", {
  curve <- irrbb_curve()
  dates <- irrbb_dates(curve)
  bond <- create_fixed_rate_bond(2L, 1e8, "2026-09-20", "2036-09-20", "6M", "japan", "unadjusted", "unadjusted", "backward", FALSE,
                                 0.016, "actual_365_fixed", "following", 100)
  value <- function(scenario) compute_bond_value_on_curve(bond, create_irrbb_shocked_curve(curve, scenario, dates), "2026-10-06")$npv
  base <- value("base")
  up <- value("parallel_up")
  down <- value("parallel_down")
  expect_lt(up, base)
  expect_gt(down, base)
  # independent: the cash flows after the evaluation date, discounted at the linearly interpolated zero rates
  pts <- compute_irrbb_curve_points(curve, "parallel_up", dates)
  cf <- get_bond_cashflows(bond)
  cf <- cf[cf$payment_date > as.Date("2026-10-06"), ]
  t <- as.numeric(cf$payment_date - as.Date("2026-10-06")) / 365
  z <- approx(pts$time, pts$shocked_zero, xout = t)$y
  expect_equal(up, sum(cf$amount * exp(-z * t)), tolerance = 1e-12)
})
