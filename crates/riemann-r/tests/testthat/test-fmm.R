# Backward-looking caplets (FMM). Synthetic inputs.

test_that("the forward joins the realized fixings and the curve", {
  expect_equal(compute_backward_forward_rate(1, 0.99, 0.985, 0.25), (0.99 / 0.985 - 1) / 0.25)
  fixings <- rep(0.02, 30)
  realized <- prod(1 + fixings / 365)
  expect_equal(compute_backward_forward_rate(realized, 1, 0.996, 0.25), (realized / 0.996 - 1) / 0.25)
  expect_error(compute_backward_forward_rate(0, 1, 0.99, 0.25), "positive")
})

test_that("the backward caplet reduces to the forward-looking one and decays through the period", {
  classical <- compute_caplet_value(0.02, 0.021, 0.3, 0.75, 0.25, 0.98)
  expect_equal(compute_backward_caplet_value(0.02, 0.021, 0.3, 0.75, 0.75 + 1e-12, 0.25, 0.98), classical, tolerance = 1e-10)
  expect_gt(compute_backward_caplet_value(0.02, 0.021, 0.3, 0.75, 1, 0.25, 0.98), classical)
  # during the period the value falls towards the intrinsic value as the period ends
  value_at <- purrr::map_dbl(c(0.2, 0.1, 0.01, 1e-9), \(left) compute_backward_caplet_value(0.025, 0.021, 0.3, left - 0.25, left, 0.25, 0.98))
  expect_true(all(diff(value_at) <= 1e-15))
  expect_lt(value_at[2], value_at[1])
  expect_equal(value_at[4], 0.25 * 0.98 * (0.025 - 0.021), tolerance = 1e-10)
  expect_equal(compute_backward_caplet_value(c(0.02, 0.025), 0.021, 0.006, 0.5, 0.75, 0.25, 0.98, model = "bachelier") > 0, c(TRUE, TRUE))
  expect_error(compute_backward_caplet_value(0.02, 0.021, 0.3, 0.5, 0.25, 0.25, 0.98), "period")
})
