# Options through the public functions, against QuantLib where the package is present. Synthetic inputs.

test_that("a European value has the columns and agrees with parity", {
  call <- european_option("call", 100, 105, 0.03, 0.01, 0.25, 1.5)
  put <- european_option("put", 100, 105, 0.03, 0.01, 0.25, 1.5)
  expect_equal(call$value - put$value, 100 * exp(-0.01 * 1.5) - 105 * exp(-0.03 * 1.5), tolerance = 1e-12)
  expect_true(all(c("delta", "gamma", "vega", "rho", "dividend_rho", "theta", "strike_sensitivity") %in% names(call)))
})

test_that("inputs recycle and failed rows are kept on request", {
  x <- european_option(c("call", "put"), 100, 105, 0.03, 0.01, c(0.25, -0.1), 1, on_error = "na")
  expect_equal(nrow(x), 2)
  expect_true(is.na(x$value[2]) && !is.na(x$error[2]))
  expect_error(european_option("call", 100, 105, 0.03, 0.01, -0.1, 1), "volatility")
  expect_error(european_option("straddle", 100, 105, 0.03, 0.01, 0.2, 1), "option type")
})

test_that("barrier and tree values behave", {
  a <- barrier_option("call", c("down_in", "down_out"), 100, 105, 80, 0, 0.03, 0.01, 0.25, 1.5)
  expect_equal(sum(a$value), european_option("call", 100, 105, 0.03, 0.01, 0.25, 1.5)$value, tolerance = 1e-10)
  expect_error(barrier_option("call", "down_out", 70, 105, 80, 0, 0.03, 0.01, 0.25, 1.5), "touched")
  b <- binomial_option("put", c("american", "european"), 100, 120, 0.03, 0.01, 0.25, 1.5, 100L)
  expect_gt(b$value[1], b$value[2])
})

test_that("the implied volatility recovers the input", {
  p <- european_option("put", 100, 90, 0.02, 0.01, 0.31, 2)$value
  expect_equal(implied_volatility("put", p, 100, 90, 0.02, 0.01, 2), 0.31, tolerance = 1e-8)
})

test_that("forward options match the equity formula", {
  d <- exp(-0.03 * 1.5)
  f <- 100 * exp(-0.01 * 1.5) / d
  expect_equal(forward_option("black", "call", f, 105, 0.25, 1.5, d)$value, european_option("call", 100, 105, 0.03, 0.01, 0.25, 1.5)$value, tolerance = 1e-11)
  expect_gt(forward_option("bachelier", "put", 0.02, 0.025, 0.006, 2, 0.95)$value, 0)
})

test_that("Monte Carlo agrees with the analytic value and does not depend on the threads", {
  a <- european_option("call", 100, 105, 0.03, 0.01, 0.25, 1.5)
  m1 <- monte_carlo_european("call", 100, 105, 0.03, 0.01, 0.25, 1.5, paths = 2e5, seed = 11, threads = 1L)
  m4 <- monte_carlo_european("call", 100, 105, 0.03, 0.01, 0.25, 1.5, paths = 2e5, seed = 11, threads = 4L)
  expect_identical(m1, m4)
  expect_lt(abs(m1$value - a$value), 5 * m1$standard_error)
  expect_lt(abs(m1$delta - a$delta), 0.01)
  expect_true(is.na(m1$gamma))
  s <- monte_carlo_european("call", 100, 105, 0.03, 0.01, 0.25, 1.5, paths = 2e5, seed = 11, smoothing = 0.25)
  expect_lt(abs(s$gamma - a$gamma), 0.03 * a$gamma)
})

test_that("a missing flag is an error", {
  expect_error(monte_carlo_european("call", 100, 105, 0.03, 0.01, 0.25, 1.5, paths = 100, seed = 1, antithetic = NA), "TRUE or FALSE")
})
