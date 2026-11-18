# Constrained portfolio optimization. Synthetic returns; the comparison with quadprog, Rglpk, the closed forms
# and GaussQuant on LagrangeFinance I-8 is crates/rmql/tests/golden_portfolio.rs.

set.seed(20261115)
n <- 5
r <- matrix(rnorm(80 * n), 80) %*% matrix(rnorm(n * n, sd = 0.02), n)
r <- sweep(r, 2, seq(0.002, 0.012, length.out = n), "+")
colnames(r) <- paste0("A", seq_len(n))

test_that("without bounds the minimum variance, tangency and mean-variance portfolios are the closed forms", {
  free <- create_portfolio_constraints(colnames(r), lower = -Inf, upper = Inf)
  m <- compute_return_moments(r)
  s_inv <- solve(m$covariance); one <- rep(1, n)
  expect_equal(compute_optimal_portfolio(r, free, "minimum_variance")$weights$weight, as.numeric(s_inv %*% one) / sum(s_inv), tolerance = 1e-12)
  tangency <- as.numeric(s_inv %*% (m$mean - 0.001)); tangency <- tangency / sum(tangency)
  expect_equal(compute_optimal_portfolio(r, free, "maximum_sharpe", risk_free_rate = 0.001)$weights$weight, tangency, tolerance = 1e-12)
  gamma <- 3
  a <- sum(s_inv); b <- sum(s_inv %*% m$mean)
  mv <- as.numeric(s_inv %*% one) / a + as.numeric(s_inv %*% m$mean - (b / a) * s_inv %*% one) / gamma
  expect_equal(compute_optimal_portfolio(r, free, "mean_variance", risk_aversion = gamma)$weights$weight, mv, tolerance = 1e-12)
  expect_equal(m$covariance, stats::cov(r), tolerance = 1e-15)
})

test_that("bounds hold, and a long data frame gives the same portfolio as the matrix", {
  box <- create_portfolio_constraints(colnames(r), lower = 0, upper = 0.3)
  long <- tibble::tibble(date = rep(seq_len(nrow(r)), n), asset = rep(colnames(r), each = nrow(r)), return = as.numeric(r))
  purrr::walk(c("minimum_variance", "maximum_return", "maximum_sharpe", "minimum_ES"), \(method) {
    p <- compute_optimal_portfolio(r, box, method)
    expect_equal(sum(p$weights$weight), 1, tolerance = 1e-12)
    expect_true(all(p$weights$weight >= -1e-12 & p$weights$weight <= 0.3 + 1e-12))
    expect_equal(compute_optimal_portfolio(long, box, method)$weights$weight, p$weights$weight, tolerance = 1e-14)
  })
  low <- compute_optimal_portfolio(r, box, "minimum_variance")$statistics$value[1]
  high <- compute_optimal_portfolio(r, box, "maximum_return")$statistics$value[1]
  targets <- seq(low, high, length.out = 5)[2:4]
  frontier <- compute_efficient_frontier(r, box, targets)
  expect_equal(frontier$expected_return, targets, tolerance = 1e-10)
  expect_true(all(diff(frontier$volatility) > 0))
})

test_that("the risk budget equalizes contributions, and every contribution adds up to its measure", {
  free <- create_portfolio_constraints(colnames(r), lower = -Inf, upper = Inf)
  w <- compute_optimal_portfolio(r, free, "risk_budget")$weights$weight
  rc <- w * as.numeric(stats::cov(r) %*% w)
  expect_equal(rc / sum(rc), rep(1 / n, n), tolerance = 1e-12)
  risk <- compute_portfolio_risk(r, stats::setNames(w, colnames(r)), confidence_level = 0.9)
  stats <- stats::setNames(risk$statistics$value, risk$statistics$metric)
  purrr::walk(c("volatility", "normal_var", "normal_es", "historical_es"), \(k) expect_equal(sum(risk$contributions[[k]]), stats[[k]], tolerance = 1e-14))
  expect_equal(stats[["historical_var"]], stats::quantile(-as.numeric(r %*% w), 0.9, type = 7, names = FALSE), tolerance = 1e-14)
})

test_that("inputs that cannot be solved are errors", {
  expect_error(create_portfolio_constraints(colnames(r), upper = 0.1), "sum to 1")
  box <- create_portfolio_constraints(colnames(r), upper = 0.3)
  expect_error(compute_optimal_portfolio(r, box, "target_return", target_return = 1), "target return")
  expect_error(compute_optimal_portfolio(r, box, "target_return"), "target_return")
  expect_error(compute_optimal_portfolio(r[, 1:3], box, "minimum_variance"), "no column")
  expect_error(compute_optimal_portfolio(r[1:3, ], box, "minimum_variance"), "positive definite")
})
