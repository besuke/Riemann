# Options through the R package against the QuantLib golden values and Monte Carlo. Synthetic inputs.

verify_options <- function() {
  read <- function(name) readr::read_csv(file.path("..", "..", "tests", "golden", name), comment = "#", show_col_types = FALSE)
  rel <- function(a, b) abs(a - b) / (1 + abs(b))
  e <- read("option_european.csv")
  ad <- RiemannFinMath::european_option(e$kind, e$spot, e$strike, e$rate, e$dividend, e$volatility, e$days / 365)
  measures <- c("value", "delta", "gamma", "vega", "rho", "dividend_rho", "theta", "strike_sensitivity")
  european <- purrr::map_dfr(measures, \(m) tibble(target = paste("European", m), cases = nrow(e), max_rel_diff = max(rel(ad[[m]], e[[m]]))))

  b <- read("option_barrier.csv")
  ba <- RiemannFinMath::barrier_option(b$kind, b$barrier_type, b$spot, b$strike, b$barrier, b$rebate, b$rate, b$dividend, b$volatility, b$days / 365)
  f <- read("option_forward.csv")
  fa <- RiemannFinMath::forward_option(f$model, f$kind, f$forward, f$strike, f$volatility, f$time, f$discount)

  t <- read("option_tree.csv")
  time <- t$days / 365
  short <- t$exercise == "american" & (time / t$steps) * t$steps < time
  ta <- RiemannFinMath::binomial_option(t$kind, t$exercise, t$spot, t$strike, t$rate, t$dividend, t$volatility, time, t$steps)
  values <- bind_rows(
    european,
    tibble(target = "Barrier value", cases = nrow(b), max_rel_diff = max(rel(ba$value, b$value))),
    tibble(target = "Black and Bachelier value", cases = nrow(f), max_rel_diff = max(rel(fa$value, f$value))),
    tibble(target = "Tree value", cases = sum(!short), max_rel_diff = max(rel(ta$value[!short], t$value[!short]))),
    tibble(target = "Tree delta", cases = sum(!short), max_rel_diff = max(rel(ta$delta[!short], t$delta[!short]))),
    tibble(target = "Tree gamma", cases = sum(!short), max_rel_diff = max(rel(ta$gamma[!short], t$gamma[!short])))
  ) |> mutate(limit = if_else(startsWith(target, "Tree"), 1e-9, 1e-11), pass = max_rel_diff < limit)
  grid_fault <- tibble(american_cases = sum(t$exercise == "american"), short_grid = sum(short),
                       riemann_higher = sum(ta$value[short] > t$value[short]), max_diff = max(abs(ta$value[short] - t$value[short])))

  i <- read("option_implied.csv")
  implied <- RiemannFinMath::implied_volatility(i$kind, i$price, i$spot, i$strike, i$rate, i$dividend, i$days / 365)
  implied_result <- tibble(cases = nrow(i), max_diff_quantlib = max(abs(implied - i$implied)), max_diff_true = max(abs(implied - i$true_volatility)))

  args <- list("call", 100, 105, 0.03, 0.01, 0.25, 1.5)
  analytic <- do.call(RiemannFinMath::european_option, args)
  mc <- function(threads, smoothing = 0) do.call(RiemannFinMath::monte_carlo_european, c(args, list(paths = 2e5, seed = 20261101, smoothing = smoothing, threads = threads)))
  m1 <- mc(1L)
  m4 <- mc(4L)
  m_smooth <- mc(0L, 0.25)
  monte_carlo <- tibble(
    measure = c("value", "delta", "vega", "rho", "dividend_rho", "theta", "gamma (smoothing 0.25)"),
    analytic = c(analytic$value, analytic$delta, analytic$vega, analytic$rho, analytic$dividend_rho, analytic$theta, analytic$gamma),
    monte_carlo = c(m1$value, m1$delta, m1$vega, m1$rho, m1$dividend_rho, m1$theta, m_smooth$gamma)
  ) |> mutate(rel_diff = abs(monte_carlo - analytic) / abs(analytic))
  list(values = values, grid_fault = grid_fault, implied = implied_result, monte_carlo = monte_carlo,
       standard_error = m1$standard_error, value_within = abs(m1$value - analytic$value) < 5 * m1$standard_error,
       threads_identical = identical(m1, m4))
}
