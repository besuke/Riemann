# The one-factor Gaussian credit portfolio. Small synthetic portfolios; the comparison with LagrangeFinance I-10 is
# crates/rmql/tests/golden_credit_risk.rs.

test_that("without correlation the distribution is the convolution of independent defaults", {
  pd <- c(0.01, 0.03, 0.05)
  d <- compute_credit_loss_distribution(pd, ead = c(100, 200, 100), lgd = 0.5, rho = 0, unit = 50)
  # losses 50, 100, 50: the number of units is 1 * D1 + 2 * D2 + 1 * D3
  p <- 1; for (i in 1:3) { m <- c(1, 2, 1)[i]; p <- c(p, rep(0, m)) * (1 - pd[i]) + c(rep(0, m), p) * pd[i] }
  expect_equal(d$probability, p, tolerance = 1e-14)
  expect_equal(attr(d, "mean"), sum(pd * c(100, 200, 100) * 0.5), tolerance = 1e-14)
})

test_that("the distribution, the simulation and the ASRF limit agree", {
  pd <- seq(0.005, 0.05, length.out = 40)
  d <- compute_credit_loss_distribution(pd, 100, 0.6, 0.3, unit = 60)
  expect_equal(sum(d$probability), 1, tolerance = 1e-13)
  r <- compute_credit_risk(d, c(0.99, 0.999))
  losses <- simulate_credit_losses(pd, 100, 0.6, 0.3, scenarios = 40000, seed = 11)
  expect_identical(losses, simulate_credit_losses(pd, 100, 0.6, 0.3, scenarios = 40000, seed = 11, threads = 1))
  expect_lt(abs(mean(losses) - attr(d, "mean")), 4 * sd(losses) / sqrt(40000))
  expect_lte(abs(compute_loss_risk(losses, 0.99)$historical_var_type1 - r$var[1]), 60)
  expect_true(all(r$es >= r$var))
  # the ASRF quantile is the large-portfolio limit: of the same order, and below the finite portfolio's tail here
  asrf <- compute_asrf_var(pd, 100, 0.6, 0.3, c(0.99, 0.999))
  expect_true(all(asrf < r$var + 120))
  expect_equal(compute_conditional_pd(0.02, 0.2, 0), pnorm(qnorm(0.02) / sqrt(0.8)), tolerance = 1e-13)
})

test_that("Basel IRB capital follows the formula, and bad inputs are errors", {
  k <- compute_irb_capital(0.01, 0.45, 2.5)
  f <- (1 - exp(-0.5)) / (1 - exp(-50)); r <- 0.12 * f + 0.24 * (1 - f)
  expect_equal(k$correlation, r, tolerance = 1e-15)
  b <- (0.11852 - 0.05478 * log(0.01))^2  # at M = 2.5 only the denominator of the maturity adjustment remains
  expect_equal(k$capital, (0.45 * pnorm((qnorm(0.01) + sqrt(r) * qnorm(0.999)) / sqrt(1 - r)) - 0.0045) / (1 - 1.5 * b), tolerance = 1e-12)
  expect_error(compute_credit_loss_distribution(0.01, 100, 0.6, 0.2, unit = 7), "whole number")
  expect_error(compute_irb_capital(0, 0.45), "PD")
})
