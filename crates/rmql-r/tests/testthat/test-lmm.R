# Caplets and correlated shocks through the public functions. Synthetic inputs.

test_that("the caplet is the scaled Black formula, and the intrinsic value without volatility", {
  df <- exp(-0.05)
  v <- compute_caplet_value(0.025, 0.0275, 0.24, 2, 0.5, df)
  expect_equal(v, QuantLib::blackFormula("Call", 0.0275, 0.025, 0.24 * sqrt(2), 0.5 * df), tolerance = 1e-14)
  expect_equal(compute_caplet_value(0.03, 0.0275, 0, 2, 0.5, df, notional = 100), 100 * 0.5 * df * 0.0025, tolerance = 1e-14)
  expect_gt(compute_caplet_value(-0.001, 0, 0.006, 2, 0.5, df, model = "bachelier"), 0)
  expect_error(compute_caplet_value(-0.001, 0, 0.006, 2, 0.5, df), "positive forward")
  expect_warning(values <- compute_caplet_value(c(0.025, -0.001), 0.0275, 0.24, 2, 0.5, df, on_error = "na"), "1 rows failed")
  expect_true(is.na(values[2]) && !is.na(values[1]))
})

test_that("the simulated caplet uses the normals it is given", {
  set.seed(1)
  z <- rnorm(20000)
  r <- compute_caplet_from_normals(z, 0.025, 0.0275, 0.24, 2, 0.5, exp(-0.05))
  payoff <- 0.5 * exp(-0.05) * pmax(0.025 * exp(-0.24^2 * 2 / 2 + 0.24 * sqrt(2) * z) - 0.0275, 0)
  expect_equal(r$value, mean(payoff), tolerance = 1e-14)
  expect_equal(r$standard_error, sd(payoff) / sqrt(20000), tolerance = 1e-12)
  expect_equal(r$paths, 20000L)
})

test_that("correlated normals are R's Z %*% chol(C), and bad matrices are errors", {
  c3 <- matrix(c(1, 0.8, 0.5, 0.8, 1, 0.7, 0.5, 0.7, 1), 3)
  set.seed(2)
  z <- matrix(rnorm(300), ncol = 3)
  expect_equal(compute_correlated_normals(z, c3), z %*% chol(c3), tolerance = 1e-14)
  own <- generate_correlated_normals(50000, c3, seed = 4)
  expect_lt(max(abs(cor(own) - c3)), 0.02)
  expect_error(compute_correlated_normals(z, matrix(c(1, 0.9, -0.9, 0.9, 1, 0.9, -0.9, 0.9, 1), 3)), "positive definite")
  expect_error(compute_correlated_normals(z[, 1:2], c3), "one column per row")
})
