# PRDC through the public functions. Synthetic inputs.

prdc_curves <- function() {
  list(
    jpy = build_ois_curve("2026-11-02", "2026-11-02", "tonar", "actual_365_fixed", ois_instruments(c("1Y", "3Y", "5Y", "7Y"), c(0.01, 0.012, 0.014, 0.016), 2L, 1L, 0L)),
    usd = build_ois_curve("2026-11-02", "2026-11-02", "sofr", "actual_365_fixed", ois_instruments(c("1Y", "3Y", "5Y", "7Y"), c(0.04, 0.038, 0.036, 0.034), 2L, 1L, 0L))
  )
}

test_that("Monte Carlo agrees with the closed form and does not depend on the threads", {
  k <- prdc_curves()
  a <- prdc_value(150, 0.12, k$jpy, k$usd, 1:5, rep(1, 5), 150, 0.8, 0.78, 0, 0.08, notional = 1e8)
  m1 <- prdc_value(150, 0.12, k$jpy, k$usd, 1:5, rep(1, 5), 150, 0.8, 0.78, 0, 0.08, notional = 1e8, paths = 2e5, seed = 3, threads = 1L)
  m4 <- prdc_value(150, 0.12, k$jpy, k$usd, 1:5, rep(1, 5), 150, 0.8, 0.78, 0, 0.08, notional = 1e8, paths = 2e5, seed = 3, threads = 4L)
  expect_identical(m1$summary, m4$summary)
  expect_lt(abs(m1$summary$npv - a$summary$npv), 5 * m1$summary$standard_error)
  expect_lt(abs(m1$summary$fx_delta - a$summary$fx_delta), 0.03 * abs(a$summary$fx_delta))
  expect_equal(a$summary$standard_error, 0)
  expect_setequal(a$curve_sensitivity$curve, c("domestic", "foreign"))
})

test_that("fixed coupons have no FX risk, and bad terms fail", {
  k <- prdc_curves()
  x <- prdc_value(150, 0.12, k$jpy, k$usd, 1:5, rep(1, 5), 150, 0.8, 0.78, fixed_rate = rep(0.03, 5))
  expect_equal(x$summary$fx_delta, 0)
  y <- prdc_value(150, 0.12, k$jpy, k$usd, 1:5, rep(1, 5), 150, 0.8, 0.78, fixed_rate = c(0.03, NA, NA, NA, NA))
  expect_gt(y$summary$fx_delta, 0)
  expect_error(prdc_value(150, 0.12, k$jpy, k$usd, 1:5, rep(1, 5), 150, 0.8, 0.78, floor = 0.1, cap = 0.05), "below the floor")
  expect_error(prdc_value(150, 0.12, k$jpy, k$usd, 1:5, rep(1, 5), 150, 0.8, 0.78, antithetic = NA), "TRUE or FALSE")
})
