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
  expect_true(all(abs(caplets$value - caplets$formula_value) < 4 * caplets$standard_error))
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

test_that("backward-looking rates (FMM) keep bonds martingales and match the FMM caplet", {
  fmm <- create_lmm(times, model$forwards, model$volatilities, beta = 0.3, discount_start = model$discount_start, backward_looking = TRUE)
  p <- generate_lmm_paths(fmm, paths = 2000L, steps_per_period = 8L, seed = 5)
  expect_true(all(is.finite(p$state[, 2, 1])))
  bonds <- fmm$discount_start / cumprod(1 + diff(times) * fmm$forwards)
  discounted <- 1 / p$numeraire[, 11]
  expect_lt(abs(mean(discounted) - bonds[10]), 4 * sd(discounted) / sqrt(2000) + 1e-12)
  caplets <- compute_lmm_caplet_value(fmm, forward = c(1, 5, 10), strike = fmm$forwards[c(1, 5, 10)], paths = 2000L, steps_per_period = 8L)
  expect_true(all(abs(caplets$value - caplets$formula_value) < 4 * caplets$standard_error))
  forward_looking <- compute_lmm_caplet_value(model, forward = 5, strike = model$forwards[5], paths = 2000L, steps_per_period = 8L)
  expect_gt(caplets$formula_value[2], forward_looking$formula_value)
})

test_that("a period under way moves only its unfixed part", {
  times <- -1 / 12 + 0.25 * (0:8)
  realized <- 1.0017
  forwards <- c(compute_backward_forward_rate(realized, 1, 0.9965, 0.25), 0.02 + 0.001 * (1:7))
  fmm <- create_lmm(times, forwards, rep(0.3, 8), beta = 0.3, discount_start = realized, backward_looking = TRUE)
  caplets <- compute_lmm_caplet_value(fmm, forward = c(1, 4), strike = forwards[c(1, 4)] + 0.0005, paths = 2000L, steps_per_period = 8L)
  expect_equal(caplets$formula_value[1], compute_backward_caplet_value(forwards[1], forwards[1] + 0.0005, 0.3, -1 / 12, 1 / 6, 0.25, 0.9965))
  expect_true(all(abs(caplets$value - caplets$formula_value) < 4 * caplets$standard_error))
  expect_error(generate_lmm_paths(create_lmm(times, forwards, rep(0.3, 8), beta = 0.3, discount_start = realized), 10), "started before today")
})

test_that("normal rates keep bonds martingales and follow Bachelier's formulas", {
  normal <- create_lmm(times, model$forwards, rep(0.006, 10), beta = 0.3, discount_start = model$discount_start, model = "bachelier")
  caplets <- compute_lmm_caplet_value(normal, forward = c(1, 5, 10), strike = normal$forwards[c(1, 5, 10)], paths = 2000L, steps_per_period = 8L)
  expect_equal(caplets$formula_value[2], compute_caplet_value(normal$forwards[5], normal$forwards[5], 0.006, times[5], 0.5,
                                                                normal$discount_start / prod(1 + 0.5 * normal$forwards[1:5]), model = "bachelier"))
  expect_true(all(abs(caplets$value - caplets$formula_value) < 4 * caplets$standard_error))
  s <- compute_lmm_swaption_value(normal, start = c(3, 5), end = c(7, 11), strike = c(0.023, 0.026), paths = 2000L, steps_per_period = 8L)
  expect_true(all(abs(s$value - s$rebonato_value) < 4 * s$standard_error + 0.02 * s$rebonato_value))
  expect_lt(max(s$rebonato_volatility), 0.01)
  fmm <- create_lmm(times, model$forwards, rep(0.006, 10), beta = 0.3, discount_start = model$discount_start, backward_looking = TRUE, model = "bachelier")
  b <- compute_lmm_caplet_value(fmm, forward = 5, strike = fmm$forwards[5], paths = 2000L, steps_per_period = 8L)
  expect_lt(abs(b$value - b$formula_value), 4 * b$standard_error)
  expect_error(create_lmm(times, model$forwards, rep(0.006, 10), beta = 0.3, model = "sabr"), "should be one of")
})

test_that("swaptions exercised between tenor dates and during a backward-looking period", {
  fmm <- create_lmm(times, model$forwards, model$volatilities, beta = 0.3, discount_start = model$discount_start, backward_looking = TRUE)
  expiry <- c(NA, times[5] + 0.25, times[5] - 0.125)
  s <- compute_lmm_swaption_value(fmm, start = 5, end = 10, strike = 0.025, expiry = expiry, paths = 2000L, steps_per_period = 8L)
  expect_true(all(abs(s$value - s$rebonato_value) < 4 * s$standard_error + 0.02 * s$rebonato_value))
  expect_equal(compute_rebonato_swaption_volatility(fmm, 5, 10, expiry = times[5]), compute_rebonato_swaption_volatility(fmm, 5, 10))
  # no arbitrage: a strike of 0 values the floating leg
  bonds <- fmm$discount_start / cumprod(1 + diff(times) * fmm$forwards)
  leg <- compute_lmm_swaption_value(fmm, start = 5, end = 10, strike = 0, expiry = times[5] + 0.25, paths = 2000L, steps_per_period = 8L)
  expect_lt(abs(leg$value - (bonds[4] - bonds[9])), 4 * leg$standard_error + 1e-12)
  expect_error(compute_lmm_swaption_value(fmm, 5, 10, 0.025, expiry = times[6] + 0.1, paths = 10L), "after the end")
})

test_that("complete periods before the one under way are fixed and left out", {
  times <- -0.6 + 0.25 * (0:10)
  forwards <- c(0.0185, 0.019, 0.02 + 0.001 * (2:9))
  full <- create_lmm(times, forwards, rep(0.25, 10), beta = 0.3, discount_start = 1.0012, backward_looking = TRUE)
  trimmed <- create_lmm(times[-(1:2)], forwards[-(1:2)], rep(0.25, 8), beta = 0.3, discount_start = 1.0012, backward_looking = TRUE)
  expect_equal(compute_rebonato_swaption_volatility(full, 5, 10), compute_rebonato_swaption_volatility(trimmed, 3, 8))
  caplets <- compute_lmm_caplet_value(full, forward = c(3, 6), strike = forwards[c(3, 6)] + 0.0005, paths = 2000L, steps_per_period = 8L)
  same <- compute_lmm_caplet_value(trimmed, forward = c(1, 4), strike = forwards[c(3, 6)] + 0.0005, paths = 2000L, steps_per_period = 8L)
  expect_equal(caplets$formula_value, same$formula_value)
  expect_true(all(abs(caplets$value - caplets$formula_value) < 4 * caplets$standard_error))
  p <- generate_lmm_paths(full, paths = 50L)
  expect_true(all(p$state[, 2, 1] == 0.0185 & p$state[, 3, 2] == 0.019))
  expect_error(compute_lmm_caplet_value(full, forward = 2, strike = 0.02, paths = 10L), "ended before today")
})

test_that("volatilities that change over time", {
  n <- 10
  flat <- create_lmm(times, model$forwards, matrix(rep(model$volatilities, n + 1), n), beta = 0.3, discount_start = model$discount_start)
  expect_equal(compute_rebonato_swaption_volatility(flat, 3, 7), compute_rebonato_swaption_volatility(model, 3, 7), tolerance = 1e-13)
  starts <- c(0, times[-length(times)])
  grid <- outer(times[1:n], (starts + times) / 2, \(fixing, middle) 0.12 + 0.2 * exp(-pmax(fixing - middle, 0)))
  moving <- create_lmm(times, model$forwards, grid, beta = 0.3, discount_start = model$discount_start, backward_looking = TRUE)
  caplets <- compute_lmm_caplet_value(moving, forward = c(1, 5, 10), strike = moving$forwards[c(1, 5, 10)], paths = 2000L, steps_per_period = 8L)
  expect_true(all(abs(caplets$value - caplets$formula_value) < 4 * caplets$standard_error))
  s <- compute_lmm_swaption_value(moving, start = c(3, 5), end = c(7, 11), strike = c(0.023, 0.026), paths = 2000L, steps_per_period = 8L)
  expect_true(all(abs(s$value - s$rebonato_value) < 4 * s$standard_error + 0.02 * s$rebonato_value))
  expect_error(create_lmm(times, model$forwards, matrix(0.2, n, n), beta = 0.3), "must be 10 by 11")
})
