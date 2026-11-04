# Options through the public functions, against QuantLib where the package is present. Synthetic inputs.

test_that("a European value has the columns and agrees with parity", {
  call <- compute_european_option_value("call", 100, 105, 0.03, 0.01, 0.25, 1.5)
  put <- compute_european_option_value("put", 100, 105, 0.03, 0.01, 0.25, 1.5)
  expect_equal(call$value - put$value, 100 * exp(-0.01 * 1.5) - 105 * exp(-0.03 * 1.5), tolerance = 1e-12)
  expect_true(all(c("delta", "gamma", "vega", "rho", "dividend_rho", "theta", "strike_sensitivity") %in% names(call)))
})

test_that("inputs recycle and failed rows are kept on request", {
  x <- compute_european_option_value(c("call", "put"), 100, 105, 0.03, 0.01, c(0.25, -0.1), 1, on_error = "na")
  expect_equal(nrow(x), 2)
  expect_true(is.na(x$value[2]) && !is.na(x$error[2]))
  expect_error(compute_european_option_value("call", 100, 105, 0.03, 0.01, -0.1, 1), "volatility")
  expect_error(compute_european_option_value("straddle", 100, 105, 0.03, 0.01, 0.2, 1), "option type")
})

test_that("barrier and tree values behave", {
  a <- compute_barrier_option_value("call", c("down_in", "down_out"), 100, 105, 80, 0, 0.03, 0.01, 0.25, 1.5)
  expect_equal(sum(a$value), compute_european_option_value("call", 100, 105, 0.03, 0.01, 0.25, 1.5)$value, tolerance = 1e-10)
  expect_error(compute_barrier_option_value("call", "down_out", 70, 105, 80, 0, 0.03, 0.01, 0.25, 1.5), "touched")
  b <- compute_binomial_option_value("put", c("american", "european"), 100, 120, 0.03, 0.01, 0.25, 1.5, 100L)
  expect_gt(b$value[1], b$value[2])
})

test_that("the implied volatility recovers the input", {
  p <- compute_european_option_value("put", 100, 90, 0.02, 0.01, 0.31, 2)$value
  expect_equal(compute_implied_volatility("put", p, 100, 90, 0.02, 0.01, 2), 0.31, tolerance = 1e-8)
})

test_that("forward options match the equity formula", {
  d <- exp(-0.03 * 1.5)
  f <- 100 * exp(-0.01 * 1.5) / d
  expect_equal(compute_forward_option_value("black", "call", f, 105, 0.25, 1.5, d)$value, compute_european_option_value("call", 100, 105, 0.03, 0.01, 0.25, 1.5)$value, tolerance = 1e-11)
  expect_gt(compute_forward_option_value("bachelier", "put", 0.02, 0.025, 0.006, 2, 0.95)$value, 0)
})

test_that("Monte Carlo agrees with the analytic value and does not depend on the threads", {
  a <- compute_european_option_value("call", 100, 105, 0.03, 0.01, 0.25, 1.5)
  m1 <- compute_monte_carlo_european_value("call", 100, 105, 0.03, 0.01, 0.25, 1.5, paths = 2e5, seed = 11, threads = 1L)
  m4 <- compute_monte_carlo_european_value("call", 100, 105, 0.03, 0.01, 0.25, 1.5, paths = 2e5, seed = 11, threads = 4L)
  expect_identical(m1, m4)
  expect_lt(abs(m1$value - a$value), 5 * m1$standard_error)
  expect_lt(abs(m1$delta - a$delta), 0.01)
  expect_true(is.na(m1$gamma))
  s <- compute_monte_carlo_european_value("call", 100, 105, 0.03, 0.01, 0.25, 1.5, paths = 2e5, seed = 11, smoothing = 0.25)
  expect_lt(abs(s$gamma - a$gamma), 0.03 * a$gamma)
})

test_that("a missing flag is an error", {
  expect_error(compute_monte_carlo_european_value("call", 100, 105, 0.03, 0.01, 0.25, 1.5, paths = 100, seed = 1, antithetic = NA), "TRUE or FALSE")
})

test_that("American approximations recycle, sit near the tree and refuse what QuantLib refuses", {
  x <- compute_american_approximation_value(c("barone_adesi_whaley", "bjerksund_stensland"), "put", 100, 100, 0.05, 0, 0.2, 367 / 365)
  # LagrangeFinance II-4.5
  expect_equal(x$value, c(6.110018383735438, 5.9950587422795465), tolerance = 1e-13)
  tree <- compute_binomial_option_value("put", "american", 100, 100, 0.05, 0, 0.2, 367 / 365, steps = 2000L)$value
  expect_true(all(abs(x$value - tree) < 0.15))
  expect_true(all(c("delta", "gamma", "vega", "theta") %in% names(x)))
  expect_error(compute_american_approximation_value("barone_adesi_whaley", "put", 100, 100, -0.01, 0, 0.2, 1), "negative interest rates")
  expect_true(is.na(compute_american_approximation_value("bjerksund_stensland", "call", 100, 100, -0.01, 0, 0.3, 1, on_error = "na")$value))
  expect_error(compute_american_approximation_value("qd_plus", "put", 100, 100, 0.05, 0, 0.2, 1), "method must be")
})
