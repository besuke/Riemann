# Market VaR, ES and backtests. Synthetic losses; the comparison with LagrangeFinance I-9 is
# crates/rmql/tests/golden_market_risk.rs.

set.seed(20261115)
losses <- -rnorm(300, 0.0004, 0.012)

test_that("historical and Gaussian VaR and ES follow R's definitions", {
  r <- compute_loss_risk(losses, c(0.95, 0.99))
  expect_equal(r$historical_var, unname(quantile(losses, c(0.95, 0.99), type = 7)), tolerance = 1e-14)
  expect_equal(r$historical_var_type1, unname(quantile(losses, c(0.95, 0.99), type = 1)), tolerance = 1e-14)
  expect_equal(r$gaussian_var, mean(losses) + qnorm(c(0.95, 0.99)) * sd(losses), tolerance = 1e-13)
  expect_equal(r$tail_mean_es[1], mean(losses[losses >= r$historical_var[1]]), tolerance = 1e-14)
  # 300 * 0.05 = 15 whole losses: the Rockafellar-Uryasev ES is their mean
  expect_equal(r$historical_es[1], mean(sort(losses, decreasing = TRUE)[1:15]), tolerance = 1e-14)
})

test_that("the Monte Carlo VaR agrees with the Gaussian value and does not depend on the threads", {
  s <- matrix(c(4, 1, 1, 9), 2) * 1e-4
  w <- c(0.6, 0.4); mu <- c(0.001, 0.002)
  a <- compute_monte_carlo_risk(w, mu, s, 0.99, scenarios = 40000, seed = 3, threads = 1)
  b <- compute_monte_carlo_risk(w, mu, s, 0.99, scenarios = 40000, seed = 3, threads = 4)
  expect_identical(a, b)
  sd_p <- sqrt(sum(w * (s %*% w)))
  expect_lt(abs(a$var - (-sum(w * mu) + qnorm(0.99) * sd_p)), 4 * a$var_standard_error)
})

test_that("backtests count exceptions and test them", {
  rolling <- compute_rolling_var(losses, 100, 0.95)
  expect_equal(nrow(rolling), 200)
  expect_equal(rolling$var[1], unname(quantile(losses[1:100], 0.95, type = 7)), tolerance = 1e-14)
  b <- compute_var_backtest(rolling$loss, rolling$var, 0.95)
  x <- sum(rolling$exception); n <- 200; p <- 0.05
  lr <- -2 * ((n - x) * log(1 - p) + x * log(p)) + 2 * ((n - x) * log(1 - x / n) + x * log(x / n))
  expect_equal(b$exceptions, x)
  expect_equal(b$kupiec_lr, lr, tolerance = 1e-12)
  expect_equal(b$kupiec_p_value, pchisq(lr, 1, lower.tail = FALSE), tolerance = 1e-12)
  expect_equal(compute_traffic_light(c(4, 5, 9, 10))$zone, c("green", "yellow", "yellow", "red"))
  expect_error(compute_var_backtest(losses, c(1, 2), 0.95), "one per loss")
  expect_error(compute_loss_risk(losses, 1), "between 0 and 1")
})
