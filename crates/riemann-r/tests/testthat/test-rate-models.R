# Hull-White and SABR through the public functions. Synthetic inputs.

hw_curve <- function() {
  build_ois_curve("2026-11-02", "2026-11-02", "tonar", "actual_365_fixed",
                  ois_instruments(c("1Y", "5Y", "10Y", "20Y"), c(0.01, 0.015, 0.02, 0.022), 2L, 1L, 0L))
}

test_that("a higher volatility raises an option on a bond", {
  v <- vapply(c(0.005, 0.015), function(s) hull_white_bond_option("put", 0.95, 2, 5, 0.1, s, hw_curve()), 0)
  expect_gt(v[2], v[1])
  expect_error(hull_white_bond_option("put", 0.95, 2, 5, -0.1, 0.01, hw_curve()), "must not be negative")
})

test_that("swaption derivatives agree with numeric differences", {
  f <- function(a, s) hull_white_swaption(TRUE, 2, 3:7, rep(0.02, 5), a, s, hw_curve())$value
  x <- hull_white_swaption(TRUE, 2, 3:7, rep(0.02, 5), 0.1, 0.01, hw_curve())
  expect_lt(abs(x$d_volatility - (f(0.1, 0.01 + 1e-6) - f(0.1, 0.01 - 1e-6)) / 2e-6), 1e-7)
  expect_lt(abs(x$d_mean_reversion - (f(0.1 + 1e-6, 0.01) - f(0.1 - 1e-6, 0.01)) / 2e-6), 1e-7)
})

test_that("a Bermudan swaption is worth at least its European swaptions", {
  b <- hull_white_bermudan_swaption(TRUE, 2, 3:7, rep(0.02, 5), 2:6, 0.1, 0.01, hw_curve())
  e <- hull_white_swaption(TRUE, 2, 3:7, rep(0.02, 5), 0.1, 0.01, hw_curve())$value
  expect_gte(b, e - 1e-4)
  one <- hull_white_bermudan_swaption(TRUE, 2, 3:7, rep(0.02, 5), 2, 0.1, 0.01, hw_curve(), points = 1201L, limit = 0.2, spacing = 0.02)
  expect_lt(abs(one - e), 1e-5)
})

test_that("a call lowers a callable bond, and no call changes nothing", {
  x <- hull_white_callable_bond((1:10) / 2, 1.5, c(rep(0, 9), 100), c(NA, rep(100, 8), NA), 0.15, 0.012, hw_curve())
  expect_gt(x$issuer_call_value, 0)
  y <- hull_white_callable_bond((1:10) / 2, 1.5, c(rep(0, 9), 100), rep(NA, 10), 0.15, 0.012, hw_curve())
  expect_lt(abs(y$issuer_call_value), 1e-10)
})

test_that("SABR calibration recovers its own smile", {
  k <- 0.03 * c(0.6, 0.8, 0.9, 1, 1.1, 1.25, 1.5)
  for (kind in c("lognormal", "normal")) {
    v <- sabr_volatility(kind, k, 0.03, 2, 0.03, 0.5, 0.4, -0.3)
    fit <- calibrate_sabr(kind, k, v, 0.03, 2, 0.5)
    expect_lt(fit$root_mean_square_error, 1e-9)
    expect_equal(fit$rho, -0.3, tolerance = 1e-3)
    expect_equal(fit$nu, 0.4, tolerance = 1e-3)
  }
  expect_error(sabr_volatility("volga", 0.03, 0.03, 1, 0.03, 0.5, 0.4, 0), "SABR volatility type")
  expect_error(sabr_volatility("normal", 0.03, 0.03, 1, 0.03, 0.5, 0.4, 1, on_error = "stop"), "rho")
})
