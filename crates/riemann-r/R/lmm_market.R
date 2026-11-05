# The lognormal forward (LIBOR) market model with joint paths and calibration. Calculations are in Rust.

#' Forward market model with joint paths
#'
#' The lognormal market model of term forward rates (LagrangeFinance II-7.4, extended): `times` are the
#' tenor dates `T_0 < ... < T_n` in years, and forward `i` (1 to n) covers `times[i]` to `times[i + 1]`, fixes at
#' the first and pays at the second. Each forward has a constant volatility, and their Brownian motions
#' are correlated. The paths are joint, under the spot measure (the numeraire rolls from one tenor date
#' to the next), with the drift that keeps every discounted bond a martingale, in log-Euler steps.
#'
#' * `create_lmm()` collects the model. Give `correlation`, or `beta` for `exp(-beta |T_i - T_j|)` on the fixing
#'   times (`create_exponential_correlation()`).
#' * `generate_lmm_paths()` returns the paths: `state[p, k, i]` is forward `i` at `times[k]` (`NA` once fixed),
#'   and `numeraire[p, k]` the numeraire at `times[k]`.
#' * `compute_lmm_swaption_value()` and `compute_lmm_caplet_value()` value payer swaptions and caplets on the
#'   paths, with their standard errors, next to Rebonato's approximation and Black's formula.
#' * `compute_rebonato_swaption_volatility()` gives Rebonato's approximation of the Black volatility of the
#'   swaption exercised at `times[start]` into the swap to `times[end]`.
#' * `calibrate_lmm_correlation()` fits `beta` to swaption volatilities by least squares on Rebonato's
#'   volatilities, the forwards' volatilities fixed. With a constant volatility per forward, the volatility of
#'   forward `i` is the Black volatility of its caplet, so the caplets calibrate the volatilities directly.
#'
#' The rates are generic term forwards: no IBOR index, curve or fixings.
#'
#' @param times Tenor dates in years from today, increasing, from 0 on.
#' @param forwards Forward rates today, one per period.
#' @param volatilities Lognormal volatilities, one per forward.
#' @param correlation A correlation matrix of the forwards, or `NULL` with `beta`.
#' @param beta Decay of the exponential correlation.
#' @param discount_start The discount factor to `times[1]`.
#' @param model A model from `create_lmm()`.
#' @param paths,steps_per_period,seed Number of paths, log-Euler steps in each period, and the seed.
#' @param start,end Positions in `times`: exercise at `times[start]`, swap to `times[end]`.
#' @param forward Position of the forward of a caplet (1 to n).
#' @param strike Strike rates.
#' @param volatility Black volatilities of the swaptions to fit.
#' @param beta_max Upper end of the search for `beta`.
#' @return `create_lmm()` a `riemann_lmm`. `create_exponential_correlation()` a matrix. `generate_lmm_paths()` a list
#'   with the array `state` and the matrix `numeraire`. The valuations a tibble. `compute_rebonato_swaption_volatility()`
#'   a numeric vector. `calibrate_lmm_correlation()` a list with `beta`, `root_mean_square_error`, a tibble `fitted`
#'   and the calibrated `model`.
#' @examples
#' times <- seq(1, 6, by = 0.5)
#' model <- create_lmm(times, seq(0.02, 0.029, length.out = 10), rep(0.22, 10), beta = 0.3, discount_start = exp(-0.02))
#' compute_lmm_swaption_value(model, start = 3, end = 7, strike = 0.023, paths = 2000L)
#' fit <- calibrate_lmm_correlation(model, start = c(3, 5), end = c(7, 11), volatility = c(0.2, 0.19))
#' fit$beta
#' @export
create_lmm <- function(times, forwards, volatilities, correlation = NULL, beta = NULL, discount_start = 1) {
  times <- as.numeric(times)
  n <- length(times) - 1L
  if (is.null(correlation)) {
    if (is.null(beta)) stop("Give `correlation` or `beta`.", call. = FALSE)
    correlation <- create_exponential_correlation(times[seq_len(n)], beta)
  }
  structure(list(times = times, forwards = as.numeric(forwards), volatilities = as.numeric(volatilities),
                 correlation = matrix(as.numeric(correlation), n, n), discount_start = as.numeric(discount_start)), class = "riemann_lmm")
}

#' @export
print.riemann_lmm <- function(x, ...) {
  cat("<riemann_lmm>", length(x$forwards), "forwards from", x$times[1], "to", x$times[length(x$times)], "years\n")
  invisible(x)
}

#' @rdname create_lmm
#' @export
create_exponential_correlation <- function(times, beta) {
  n <- length(times)
  matrix(checked(.Call(wrap__create_exponential_correlation, as.numeric(times), as.numeric(beta)))$value, n, n, byrow = TRUE)
}

lmm_args <- function(model) {
  if (!inherits(model, "riemann_lmm")) stop("`model` must come from create_lmm().", call. = FALSE)
  list(model$times, model$forwards, model$volatilities, as.numeric(t(model$correlation)), model$discount_start)
}

#' @rdname create_lmm
#' @export
generate_lmm_paths <- function(model, paths, steps_per_period = 4L, seed = 1) {
  a <- lmm_args(model)
  x <- checked(.Call(wrap__generate_lmm_paths, a[[1]], a[[2]], a[[3]], a[[4]], a[[5]], as.integer(paths), as.integer(steps_per_period), as.numeric(seed)))
  n <- length(model$forwards)
  list(state = array(x$state, c(paths, n + 1L, n)), numeraire = matrix(x$numeraire, paths, n + 1L))
}

#' @rdname create_lmm
#' @export
compute_rebonato_swaption_volatility <- function(model, start, end) {
  a <- lmm_args(model)
  i <- tibble(start = as.integer(start), end = as.integer(end))
  checked(.Call(wrap__compute_rebonato_swaption_volatility, a[[1]], a[[2]], a[[3]], a[[4]], a[[5]], i$start - 1L, i$end - 1L,
                rep(0, nrow(i))))$volatility
}

#' @rdname create_lmm
#' @export
compute_lmm_swaption_value <- function(model, start, end, strike, paths = 2000L, steps_per_period = 4L, seed = 1) {
  a <- lmm_args(model)
  i <- tibble(start = as.integer(start), end = as.integer(end), strike = as.numeric(strike))
  mc <- checked(.Call(wrap__compute_lmm_monte_carlo, a[[1]], a[[2]], a[[3]], a[[4]], a[[5]], rep("swaption", nrow(i)), i$start - 1L, i$end - 1L, i$strike,
                      as.integer(paths), as.integer(steps_per_period), as.numeric(seed)))
  approx <- checked(.Call(wrap__compute_rebonato_swaption_volatility, a[[1]], a[[2]], a[[3]], a[[4]], a[[5]], i$start - 1L, i$end - 1L, i$strike))
  tibble::add_column(i, value = mc$value, standard_error = mc$standard_error, rebonato_volatility = approx$volatility,
                     rebonato_value = approx$black_value)
}

#' @rdname create_lmm
#' @export
compute_lmm_caplet_value <- function(model, forward, strike, paths = 2000L, steps_per_period = 4L, seed = 1) {
  a <- lmm_args(model)
  i <- tibble(forward = as.integer(forward), strike = as.numeric(strike))
  mc <- checked(.Call(wrap__compute_lmm_monte_carlo, a[[1]], a[[2]], a[[3]], a[[4]], a[[5]], rep("caplet", nrow(i)), i$forward - 1L, i$forward, i$strike,
                      as.integer(paths), as.integer(steps_per_period), as.numeric(seed)))
  n <- length(model$forwards)
  discount <- model$discount_start / cumprod(1 + diff(model$times) * model$forwards)
  black <- purrr::pmap_dbl(i, \(forward, strike) {
    compute_caplet_value(model$forwards[forward], strike, model$volatilities[forward], model$times[forward],
                         model$times[forward + 1L] - model$times[forward], discount[forward])
  })
  tibble::add_column(i, value = mc$value, standard_error = mc$standard_error, black_value = black)
}

#' @rdname create_lmm
#' @export
calibrate_lmm_correlation <- function(model, start, end, volatility, beta_max = 5) {
  a <- lmm_args(model)
  i <- tibble(start = as.integer(start), end = as.integer(end), volatility = as.numeric(volatility))
  x <- checked(.Call(wrap__calibrate_lmm_correlation, a[[1]], a[[2]], a[[3]], a[[5]], i$start - 1L, i$end - 1L, i$volatility, as.numeric(beta_max)))
  list(beta = x$beta, root_mean_square_error = x$root_mean_square_error, fitted = tibble::add_column(i, fitted_volatility = x$fitted),
       model = create_lmm(model$times, model$forwards, model$volatilities, beta = x$beta, discount_start = model$discount_start))
}
