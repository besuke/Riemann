# Sobol points, paths and the Longstaff-Schwartz value through the public functions. Synthetic inputs.

test_that("the Sobol sequence starts at the middle, has no seed and stops at 32 dimensions", {
  p <- generate_sobol_sequence(3, 4)
  expect_equal(dim(p), c(4L, 3L))
  expect_true(all(p[1, ] == 0.5))
  expect_true(all(p > 0 & p < 1))
  expect_error(generate_sobol_sequence(33, 1), "1 to 32")
})

test_that("paths have the legacy layout and the first Sobol path is the drift alone", {
  paths <- generate_black_scholes_paths(100, 0.05, 0, 0.2, 1, steps = 4, paths = 3)
  expect_named(paths, c("path_id", "step", "time", "price"))
  expect_equal(nrow(paths), 15L)
  first <- paths$price[paths$path_id == 1L]
  expect_equal(first, 100 * exp((0.05 - 0.02) * (0:4) / 4), tolerance = 1e-14)
  expect_error(generate_black_scholes_paths(100, 0.05, 0, 0.2, 1, 4, 3, sequence = "pseudo", brownian_bridge = TRUE), "only for the Sobol")
})

test_that("the legacy conditions give the legacy value and counts", {
  r <- compute_longstaff_schwartz_value("put", 100, 100, 0.05, 0, 0.2, 367 / 365, steps = 12, paths = 2048)
  expect_lt(abs(r$value - 6.2924853437827819), 1e-12)
  expect_lt(abs(r$standard_error - 0.16653057537317539), 1e-12)
  ex <- r$exercise[[1]]
  expect_equal(ex$paths[ex$step %in% 2:11], c(28L, 52L, 57L, 109L, 107L, 100L, 87L, 38L, 78L, 96L))
  expect_equal(sum(ex$paths) + r$unexercised_paths, 2048L)
  expect_equal(r$dates_with_too_few_paths, 0L)
  reg <- r$regressions[[1]]
  expect_equal(lengths(reg$coefficients), rep(3L, 11L))
})

test_that("only maturity is the European option, and the settings are checked", {
  r <- compute_longstaff_schwartz_value("put", 100, 100, 0.05, 0, 0.2, 1, steps = 12, paths = 4096, exercise_steps = 12L)
  expect_lt(abs(r$value - r$european_value), 1e-12)
  separate <- compute_longstaff_schwartz_value("put", 100, 100, 0.05, 0, 0.2, 1, steps = 12, paths = 4096, sequence = "pseudo",
                                               calibration = "separate", calibration_paths = 4096)
  expect_gt(separate$value, separate$european_value)
  expect_error(compute_longstaff_schwartz_value("put", 100, 100, 0.05, 0, 0.2, 1, 12, 64, calibration = "other"), "in_sample")
  expect_error(compute_longstaff_schwartz_value("put", 100, 100, 0.05, 0, 0.2, 1, 12, 64, few_in_the_money = "other"), "hold or exercise")
})
