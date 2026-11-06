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
#'   times (`create_exponential_correlation()`). With `factors = m`, the model has `m` factors: its correlation is
#'   the reduction to rank `m` of `create_reduced_correlation()`.
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
#' The rates are generic term forwards: no IBOR index, curve or fixings. With `backward_looking = TRUE` they are
#' backward-looking compounded rates, as overnight rates compounded over each period (the generalized forward
#' market model, FMM, of Lyashenko and Mercurio): rate `i` keeps moving through its period, its volatility
#' falling linearly to 0 at the period end, where it is known. The caplet's `black_value` is then the FMM
#' formula of [compute_backward_caplet_value()], and `state[p, k, i]` also holds rate `k - 1` at its end. The caplets'
#' `formula_value` is Black's or Bachelier's formula, by `model`.
#'
#' @param times Tenor dates in years from today, increasing, from 0 on.
#' @param forwards Forward rates today, one per period.
#' @param volatilities Volatilities, one per forward (lognormal for `"black"`, in rate units for `"bachelier"`), or a matrix
#'   with a row per forward and `n + 1` columns for volatilities that change over time: column `k` holds on
#'   `(times[k - 1], times[k]]`, column 1 up to `times[1]`. A rate uses its row while it moves.
#' @param correlation A correlation matrix of the forwards, or `NULL` with `beta`.
#' @param beta Decay of the exponential correlation.
#' @param factors `NULL` for as many factors as forwards, or the number of factors `m`: the correlation is then replaced
#'   by its reduction to rank `m` (`create_reduced_correlation()`, QuantLib's `rankReducedSqrt`), which the paths, the
#'   drifts and Rebonato's approximation all use, and the paths draw `m` normals per step.
#' @param discount_start The discount factor to `times[1]`. When the first period of backward-looking rates started before
#'   today (`times[1] < 0`), the compounding factor of its fixings so far instead; `forwards[1]` is then the forward of
#'   the whole period from [compute_backward_forward_rate()], and only the rest of the period is random. Earlier
#'   periods that ended on or before today are complete: give their fixed rates in `forwards`; they are not
#'   simulated, and the discount factors and the numeraire start from the period under way. Caplets and swaptions
#'   on complete periods are refused. [compute_backward_period_state()] gives that period's `discount_start` and
#'   forward from its daily fixings.
#' @param backward_looking `FALSE` for forward-looking term rates, `TRUE` for backward-looking compounded rates.
#' @param model `"black"` for lognormal rates, or `"bachelier"` for normal rates, whose volatilities are in rate units
#'   and which may turn negative. Rebonato's volatility and the formula values follow the model.
#' @param model A model from `create_lmm()`.
#' @param paths,steps_per_period,seed Number of paths, log-Euler steps in each period, and the seed.
#' @param start,end Positions in `times`: the swap runs from `times[start]` to `times[end]`.
#' @param expiry `NULL` to exercise at `times[start]`, or exercise times in years, after 0 and not after the end of
#'   the swap's first period (`times[start + 1]`): between tenor dates, or during the first period of
#'   backward-looking rates. The Monte Carlo then keeps the rates at that time and defers the payoff to the next
#'   tenor date; Rebonato's approximation integrates the decay of each backward-looking rate up to `expiry`.
#' @param forward Position of the forward of a caplet (1 to n).
#' @param strike Strike rates.
#' @param volatility Black volatilities of the swaptions to fit.
#' @param beta_max Upper end of the search for `beta`.
#' @return `create_lmm()` a `riemann_lmm`. `create_exponential_correlation()` and `create_reduced_correlation()` a matrix.
#'   `generate_lmm_paths()` a list
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
create_lmm <- function(times, forwards, volatilities, correlation = NULL, beta = NULL, discount_start = 1, backward_looking = FALSE, model = "black",
                       factors = NULL) {
  times <- as.numeric(times)
  n <- length(times) - 1L
  if (is.null(correlation)) {
    if (is.null(beta)) stop("Give `correlation` or `beta`.", call. = FALSE)
    correlation <- create_exponential_correlation(times[seq_len(n)], beta)
  }
  if (!is.null(factors)) correlation <- create_reduced_correlation(correlation, factors)
  grid <- NULL
  if (is.matrix(volatilities)) {
    if (!identical(dim(volatilities), c(n, n + 1L))) stop(sprintf("A volatility matrix must be %d by %d.", n, n + 1L), call. = FALSE)
    grid <- as.numeric(t(volatilities))
    volatilities <- rowMeans(volatilities)
  }
  structure(list(times = times, forwards = as.numeric(forwards), volatilities = as.numeric(volatilities), volatility_grid = grid,
                 correlation = matrix(as.numeric(correlation), n, n), discount_start = as.numeric(discount_start),
                 backward_looking = as_flag(backward_looking), model = match.arg(model, c("black", "bachelier")),
                 factors = if (is.null(factors)) NULL else as.integer(factors)), class = "riemann_lmm")
}

#' @export
print.riemann_lmm <- function(x, ...) {
  cat("<riemann_lmm>", length(x$forwards), if (isTRUE(x$backward_looking)) "backward-looking" else "forward-looking", x$model %||% "black", "rates from",
      x$times[1], "to", x$times[length(x$times)], "years\n")
  invisible(x)
}

#' @rdname create_lmm
#' @export
create_exponential_correlation <- function(times, beta) {
  n <- length(times)
  matrix(checked(.Call(wrap__create_exponential_correlation, as.numeric(times), as.numeric(beta)))$value, n, n, byrow = TRUE)
}

#' @rdname create_lmm
#' @export
create_reduced_correlation <- function(correlation, factors) {
  correlation <- as.matrix(correlation)
  n <- nrow(correlation)
  matrix(checked(.Call(wrap__create_reduced_correlation, as.numeric(t(correlation)), as.integer(n), as.integer(factors)))$value, n, n, byrow = TRUE)
}

lmm_args <- function(model) {
  if (!inherits(model, "riemann_lmm")) stop("`model` must come from create_lmm().", call. = FALSE)
  list(model$times, model$forwards, model$volatilities, as.numeric(t(model$correlation)), model$discount_start, isTRUE(model$backward_looking),
       if (is.null(model$model)) "black" else model$model, if (is.null(model$volatility_grid)) numeric() else model$volatility_grid,
       if (is.null(model$factors)) 0L else model$factors)
}

#' @rdname create_lmm
#' @export
generate_lmm_paths <- function(model, paths, steps_per_period = 4L, seed = 1) {
  a <- lmm_args(model)
  x <- checked(.Call(wrap__generate_lmm_paths, a[[1]], a[[2]], a[[3]], a[[4]], a[[5]], a[[6]], a[[7]], a[[8]], a[[9]], as.integer(paths), as.integer(steps_per_period), as.numeric(seed)))
  n <- length(model$forwards)
  list(state = array(x$state, c(paths, n + 1L, n)), numeraire = matrix(x$numeraire, paths, n + 1L))
}

#' @rdname create_lmm
#' @export
compute_rebonato_swaption_volatility <- function(model, start, end, expiry = NULL) {
  a <- lmm_args(model)
  i <- tibble(start = as.integer(start), end = as.integer(end), expiry = if (is.null(expiry)) NA_real_ else as.numeric(expiry))
  checked(.Call(wrap__compute_rebonato_swaption_volatility, a[[1]], a[[2]], a[[3]], a[[4]], a[[5]], a[[6]], a[[7]], a[[8]], a[[9]], i$start - 1L, i$end - 1L,
                rep(0, nrow(i)), i$expiry))$volatility
}

#' @rdname create_lmm
#' @export
compute_lmm_swaption_value <- function(model, start, end, strike, expiry = NULL, paths = 2000L, steps_per_period = 4L, seed = 1) {
  a <- lmm_args(model)
  i <- tibble(start = as.integer(start), end = as.integer(end), strike = as.numeric(strike),
              expiry = if (is.null(expiry)) NA_real_ else as.numeric(expiry))
  mc <- checked(.Call(wrap__compute_lmm_monte_carlo, a[[1]], a[[2]], a[[3]], a[[4]], a[[5]], a[[6]], a[[7]], a[[8]], a[[9]], rep("swaption", nrow(i)), i$start - 1L, i$end - 1L, i$strike, i$expiry,
                      as.integer(paths), as.integer(steps_per_period), as.numeric(seed)))
  approx <- checked(.Call(wrap__compute_rebonato_swaption_volatility, a[[1]], a[[2]], a[[3]], a[[4]], a[[5]], a[[6]], a[[7]], a[[8]], a[[9]], i$start - 1L, i$end - 1L, i$strike, i$expiry))
  tibble::add_column(i, value = mc$value, standard_error = mc$standard_error, rebonato_volatility = approx$volatility,
                     rebonato_value = approx$black_value)
}

#' @rdname create_lmm
#' @export
compute_lmm_caplet_value <- function(model, forward, strike, paths = 2000L, steps_per_period = 4L, seed = 1) {
  a <- lmm_args(model)
  i <- tibble(forward = as.integer(forward), strike = as.numeric(strike))
  mc <- checked(.Call(wrap__compute_lmm_monte_carlo, a[[1]], a[[2]], a[[3]], a[[4]], a[[5]], a[[6]], a[[7]], a[[8]], a[[9]], rep("caplet", nrow(i)), i$forward - 1L, i$forward, i$strike, rep(NA_real_, nrow(i)),
                      as.integer(paths), as.integer(steps_per_period), as.numeric(seed)))
  # the formula with each rate's total variance to its fixing, as the paths define it
  tibble::add_column(i, value = mc$value, standard_error = mc$standard_error, formula_value = mc$formula)
}

#' @rdname create_lmm
#' @export
calibrate_lmm_correlation <- function(model, start, end, volatility, beta_max = 5) {
  a <- lmm_args(model)
  i <- tibble(start = as.integer(start), end = as.integer(end), volatility = as.numeric(volatility))
  x <- checked(.Call(wrap__calibrate_lmm_correlation, a[[1]], a[[2]], a[[3]], a[[5]], a[[7]], a[[8]], a[[9]], i$start - 1L, i$end - 1L, i$volatility, as.numeric(beta_max)))
  list(beta = x$beta, root_mean_square_error = x$root_mean_square_error, fitted = tibble::add_column(i, fitted_volatility = x$fitted),
       model = create_lmm(model$times, model$forwards,
                          if (is.null(model$volatility_grid)) model$volatilities else matrix(model$volatility_grid, length(model$forwards), byrow = TRUE),
                          beta = x$beta, discount_start = model$discount_start,
                          backward_looking = isTRUE(model$backward_looking), model = a[[7]], factors = model$factors))
}
