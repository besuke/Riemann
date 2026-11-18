#' Market VaR, expected shortfall and backtests
#'
#' VaR and expected shortfall (ES) of a loss series (losses positive, `L_t = -R_t`), by the variance-covariance
#' (Gaussian), historical and Monte Carlo methods, and the backtest of a VaR (LagrangeFinance I-9).
#'
#' `compute_loss_risk()` gives, for each confidence level, the Gaussian VaR and ES (`mean + qnorm(alpha) sd` and
#' `mean + dnorm(qnorm(alpha)) / (1 - alpha) sd` of the losses, sample standard deviation), the historical VaR
#' as R's `quantile(type = 7)` (the default) and `type = 1`, and two historical ES: `tail_mean_es`, the mean of
#' the losses at or above the type-7 VaR, and `historical_es`, Rockafellar and Uryasev's mean of the largest
#' `(1 - alpha) T` losses with the last counted in part.
#'
#' `compute_monte_carlo_risk()` simulates normal returns `N(mean, covariance)` of the assets and the losses
#' `-w'R` of the portfolio. The random numbers depend on the seed, scenario and asset only, so the result does
#' not depend on `threads`; it is not R's random stream. Standard errors come from `batches` batch means.
#'
#' `compute_var_backtest()` counts the exceptions `L_t > VaR_t` and gives Kupiec's proportion-of-failures test,
#' Christoffersen's independence test and their sum, the conditional coverage test (likelihood ratios and
#' chi-squared p-values with 1, 1 and 2 degrees of freedom). `compute_rolling_var()` gives `VaR_t` from the
#' `window` losses before `t`, for `t = window + 1, ..., T`. `compute_traffic_light()` gives the Basel zone:
#' green while the binomial probability of at most that many exceptions is below 95%, yellow while below
#' 99.99%, red otherwise (with 250 observations at 99%: green up to 4, yellow 5 to 9, red from 10).
#'
#' @param losses Losses, positive numbers for losses.
#' @param confidence_level Confidence level(s), strictly between 0 and 1.
#' @param weights,mean,covariance Portfolio weights and the mean and covariance of the assets' returns.
#' @param scenarios,seed,batches,threads The simulation; `threads = 0` uses every logical processor.
#' @param var The VaR to test: one number, or one per loss.
#' @param window Number of past losses for each VaR.
#' @param method `"historical"` (type-7 quantile) or `"gaussian"`.
#' @param exceptions,observations Exceptions and the number of observations.
#' @return `compute_loss_risk()`, `compute_traffic_light()` tibbles with one row per confidence level or
#'   number of exceptions. `compute_monte_carlo_risk()` a one-row tibble. `compute_var_backtest()` a one-row
#'   tibble. `compute_rolling_var()` a tibble with `t`, `loss`, `var` and `exception`.
#' @examples
#' set.seed(1)
#' losses <- -rnorm(250, 0.0005, 0.01)
#' compute_loss_risk(losses, c(0.95, 0.99))
#' compute_monte_carlo_risk(1, 0.0005, matrix(1e-4), confidence_level = 0.99, scenarios = 20000)
#' rolling <- compute_rolling_var(losses, 100, 0.99)
#' compute_var_backtest(rolling$loss, rolling$var, 0.99)
#' compute_traffic_light(0:12)
#' @export
compute_loss_risk <- function(losses, confidence_level = 0.95) {
  r <- checked(.Call(wrap__compute_loss_risk, as.numeric(losses), as.numeric(confidence_level)))
  tibble::as_tibble(r)
}

#' @rdname compute_loss_risk
#' @export
compute_monte_carlo_risk <- function(weights, mean, covariance, confidence_level = 0.95, scenarios = 100000, seed = 1, batches = 20L, threads = 0L) {
  covariance <- as.matrix(covariance)
  r <- checked(.Call(wrap__compute_monte_carlo_risk, as.numeric(weights), as.numeric(mean), as.numeric(t(covariance)),
                     as.numeric(confidence_level), as.numeric(scenarios), as.numeric(seed), as.integer(batches), as.integer(threads)))
  tibble::tibble(confidence_level = as.numeric(confidence_level), var = r$var, es = r$es, var_standard_error = r$var_standard_error,
                 es_standard_error = r$es_standard_error, scenarios = r$scenarios)
}

#' @rdname compute_loss_risk
#' @export
compute_var_backtest <- function(losses, var, confidence_level = 0.95) {
  tibble::as_tibble(checked(.Call(wrap__compute_var_backtest, as.numeric(losses), as.numeric(var), as.numeric(confidence_level))))
}

#' @rdname compute_loss_risk
#' @export
compute_rolling_var <- function(losses, window, confidence_level = 0.95, method = c("historical", "gaussian")) {
  method <- match.arg(method)
  losses <- as.numeric(losses)
  v <- checked(.Call(wrap__compute_rolling_var, losses, as.integer(window), as.numeric(confidence_level), method))$var
  t <- seq_along(v) + as.integer(window)
  tibble::tibble(t = t, loss = losses[t], var = v, exception = losses[t] > v)
}

#' @rdname compute_loss_risk
#' @export
compute_traffic_light <- function(exceptions, observations = 250L, confidence_level = 0.99) {
  r <- checked(.Call(wrap__compute_traffic_light, as.integer(exceptions), as.integer(observations), as.numeric(confidence_level)))
  tibble::tibble(exceptions = as.integer(exceptions), cumulative_probability = r$cumulative_probability, zone = r$zone)
}
