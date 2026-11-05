# The forward market model with joint paths. Synthetic inputs.

times <- seq(1, 6, by = 0.5)
model <- create_lmm(times, seq(0.02, 0.029, length.out = 10), seq(0.25, 0.205, length.out = 10), beta = 0.3, discount_start = exp(-0.02))

test_that("discounted bonds are martingales and caplets follow Black's formula", {
  p <- generate_lmm_paths(model, paths = 2000L, seed = 3)
  expect_equal(dim(p$state), c(2000L, 11L, 10L))
  bonds <- model$discount_start / cumprod(1 + diff(times) * model$forwards)
  purrr::walk(c(2L, 6L, 11L), \(k) {
    discounted <- 1 / p$numeraire[, k]
    expect_lt(abs(mean(discounted) - c(model$discount_start, bonds)[k]), 4 * sd(discounted) / sqrt(2000) + 1e-12)
  })
  caplets <- compute_lmm_caplet_value(model, forward = c(1, 5, 10), strike = model$forwards[c(1, 5, 10)], paths = 2000L)
  expect_true(all(abs(caplets$value - caplets$black_value) < 4 * caplets$standard_error))
})

test_that("Rebonato's swaption is near the simulated one, and the calibration recovers beta", {
  s <- compute_lmm_swaption_value(model, start = c(3, 5), end = c(7, 11), strike = c(0.023, 0.026), paths = 2000L)
  expect_true(all(abs(s$value - s$rebonato_value) < 4 * s$standard_error + 0.02 * s$rebonato_value))
  quotes <- compute_rebonato_swaption_volatility(model, start = c(3, 5, 2, 7), end = c(7, 11, 9, 11))
  other <- create_lmm(times, model$forwards, model$volatilities, beta = 1.2, discount_start = model$discount_start)
  fit <- calibrate_lmm_correlation(other, start = c(3, 5, 2, 7), end = c(7, 11, 9, 11), volatility = quotes)
  expect_equal(fit$beta, 0.3, tolerance = 1e-6)
  expect_lt(fit$root_mean_square_error, 1e-9)
  expect_s3_class(fit$model, "riemann_lmm")
  expect_error(compute_rebonato_swaption_volatility(model, 5, 5), "T_a to T_b")
  expect_error(create_lmm(times, model$forwards, model$volatilities), "correlation")
})
