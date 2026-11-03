# Pass-through MBS through the public functions. Synthetic inputs.

pool <- function(...) create_mbs_pass_through(100, 0.05, 0.045, refinancing_cost_rate = 0.005, ...)

test_that("the pool is checked once and prints", {
  expect_s3_class(pool(), "riemann_mbs")
  expect_error(create_mbs_pass_through(100, 0.05, 0.06), "at most the mortgage rate")
  expect_error(pool(amortization = "other"), "fixed_payment or reamortized")
  expect_output(print(pool()), "riemann_mbs")
})

test_that("cash flows repay the principal, and prepayment follows the refinancing rate", {
  for (amortization in c("fixed_payment", "reamortized")) {
    cf <- compute_mbs_cashflows(pool(amortization = amortization), c(rep(0.05, 24), rep(0.035, 336)))
    expect_equal(nrow(cf), 360L)
    expect_lt(abs(sum(cf$scheduled_principal + cf$prepaid_principal) - 100), 1e-9)
    expect_lt(abs(tail(cf$end_balance, 1)), 1e-9)
  }
  life <- function(f) compute_mbs_path_values(pool(), matrix(0.04, 1, 360), matrix(f, 1, 360))$weighted_average_life
  expect_lt(life(0.035), life(0.05))
  expect_lt(life(0.05), life(0.065))
  expect_error(compute_mbs_cashflows(pool(), rep(0.05, 12)), "1 or 360")
})

test_that("the spread reprices the mean of the path values", {
  r <- generate_hull_white_monthly_rates(40, 360, 0.02, 0.15, 0.012, seed = 3)
  f <- pmax(r + 0.03, 0)
  z <- compute_mbs_oas(pool(), 110, r, f)
  expect_lt(abs(mean(compute_mbs_path_values(pool(), r, f, oas = z)$price) - 110), 1e-9)
  expect_error(compute_mbs_path_values(pool(), r, f[, 1:12]), "same dimensions")
  expect_error(generate_hull_white_monthly_rates(3, 12, 0.02, 0.15, 0.01), "even")
})
