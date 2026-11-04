# Options. Calculations and sensitivities are in Rust; time is in years and rates are continuous.

option_table <- function(rows, inputs, on_error) {
  rows <- finish_rows(rows, on_error)
  values <- tibble::as_tibble(rows[setdiff(names(rows), "error")])
  tibble::as_tibble(c(as.list(inputs), as.list(values), list(error = rows$error)))
}

#' Options on an equity
#'
#' Prices with sensitivities by automatic differentiation, under Black-Scholes-Merton with a flat
#' continuous rate, dividend yield and volatility. `compute_barrier_option_value()` monitors the barrier
#' continuously. `compute_binomial_option_value()` is a Cox-Ross-Rubinstein tree; its value is piecewise linear in
#' the spot, so its delta and gamma come from the first steps of the tree, as in QuantLib, and the
#' other sensitivities are the derivatives of the tree.
#'
#' Inputs are recycled to a common length.
#'
#' @param option_type `"call"` or `"put"`.
#' @param spot,strike Positive numbers.
#' @param rate,dividend_yield Continuous rates.
#' @param volatility Positive lognormal volatility.
#' @param time Positive time to expiry in years.
#' @param barrier_type `"down_in"`, `"up_in"`, `"down_out"` or `"up_out"`.
#' @param barrier Barrier level. A spot past the barrier is an error.
#' @param rebate Paid at the knock-out or, for a knock-in, at expiry if not knocked in.
#' @param exercise `"american"` or `"european"`.
#' @param steps Steps of the tree, at least 2.
#' @param on_error `"stop"` or `"na"`; see [RiemannFinMath-package].
#' @return A tibble with the inputs and the columns `value`, `delta`, `gamma`, `vega`, `rho`,
#'   `dividend_rho`, `theta` (per year of calendar time), `strike_sensitivity` and `error`.
#' @examples
#' compute_european_option_value("call", 100, 105, 0.03, 0.01, 0.25, 1.5)
#' compute_barrier_option_value("call", "down_out", 100, 105, 80, 0, 0.03, 0.01, 0.25, 1.5)
#' compute_binomial_option_value("put", "american", 100, 110, 0.03, 0.01, 0.25, 1.5)
#' @export
compute_european_option_value <- function(option_type, spot, strike, rate, dividend_yield, volatility, time, on_error = "stop") {
  i <- tibble(option_type = as.character(option_type), spot = as.numeric(spot), strike = as.numeric(strike), rate = as.numeric(rate),
              dividend_yield = as.numeric(dividend_yield), volatility = as.numeric(volatility), time = as.numeric(time))
  option_table(.Call(wrap__compute_european_option_value, i$option_type, i$spot, i$strike, i$rate, i$dividend_yield, i$volatility, i$time), i, on_error)
}

#' @rdname compute_european_option_value
#' @export
compute_barrier_option_value <- function(option_type, barrier_type, spot, strike, barrier, rebate, rate, dividend_yield, volatility, time, on_error = "stop") {
  i <- tibble(option_type = as.character(option_type), barrier_type = as.character(barrier_type), spot = as.numeric(spot), strike = as.numeric(strike),
              barrier = as.numeric(barrier), rebate = as.numeric(rebate), rate = as.numeric(rate),
              dividend_yield = as.numeric(dividend_yield), volatility = as.numeric(volatility), time = as.numeric(time))
  option_table(.Call(wrap__compute_barrier_option_value, i$option_type, i$barrier_type, i$spot, i$strike, i$barrier, i$rebate, i$rate, i$dividend_yield, i$volatility, i$time), i, on_error)
}

#' @rdname compute_european_option_value
#' @export
compute_binomial_option_value <- function(option_type, exercise, spot, strike, rate, dividend_yield, volatility, time, steps = 200L, on_error = "stop") {
  i <- tibble(option_type = as.character(option_type), exercise = as.character(exercise), spot = as.numeric(spot), strike = as.numeric(strike), rate = as.numeric(rate),
              dividend_yield = as.numeric(dividend_yield), volatility = as.numeric(volatility), time = as.numeric(time), steps = as.integer(steps))
  option_table(.Call(wrap__compute_binomial_option_value, i$option_type, i$exercise, i$spot, i$strike, i$rate, i$dividend_yield, i$volatility, i$time, i$steps), i, on_error)
}

#' American options by closed-form approximations
#'
#' The Barone-Adesi–Whaley (1987) and Bjerksund–Stensland (1993) approximations, as in QuantLib's
#' `BaroneAdesiWhaleyApproximationEngine` and `BjerksundStenslandApproximationEngine`. The Greeks are the
#' derivatives of the approximation by automatic differentiation; QuantLib gives none for
#' Barone-Adesi–Whaley when early exercise can be optimal. Barone-Adesi–Whaley refuses negative rates.
#' Inputs are recycled to a common length.
#'
#' @param method `"barone_adesi_whaley"` or `"bjerksund_stensland"`.
#' @inheritParams compute_european_option_value
#' @return A tibble as from [compute_european_option_value()], with `method`.
#' @examples
#' compute_american_approximation_value(c("barone_adesi_whaley", "bjerksund_stensland"), "put", 36, 40, 0.06, 0, 0.2, 1)
#' @export
compute_american_approximation_value <- function(method, option_type, spot, strike, rate, dividend_yield, volatility, time, on_error = "stop") {
  i <- tibble(method = as.character(method), option_type = as.character(option_type), spot = as.numeric(spot), strike = as.numeric(strike),
              rate = as.numeric(rate), dividend_yield = as.numeric(dividend_yield), volatility = as.numeric(volatility), time = as.numeric(time))
  option_table(.Call(wrap__compute_american_approximation_value, i$method, i$option_type, i$spot, i$strike, i$rate, i$dividend_yield, i$volatility,
                     i$time), i, on_error)
}

#' Options on a forward
#'
#' Black (lognormal) and Bachelier (normal) formulas, with derivatives by automatic differentiation.
#'
#' @param model `"black"` or `"bachelier"`.
#' @param option_type `"call"` or `"put"`.
#' @param forward,strike Forward and strike. Black needs both positive.
#' @param volatility Lognormal for Black, absolute for Bachelier.
#' @param time Time to expiry in years.
#' @param discount Discount factor to payment.
#' @param on_error `"stop"` or `"na"`; see [RiemannFinMath-package].
#' @return A tibble with the inputs and the columns `value`, `delta_forward`, `gamma_forward`,
#'   `vega`, `d_time`, `d_discount`, `strike_sensitivity` and `error`.
#' @examples
#' compute_forward_option_value("black", "call", 100, 105, 0.25, 1.5, 0.96)
#' compute_forward_option_value("bachelier", "put", 0.02, 0.025, 0.006, 2, 0.95)
#' @export
compute_forward_option_value <- function(model, option_type, forward, strike, volatility, time, discount, on_error = "stop") {
  i <- tibble(model = as.character(model), option_type = as.character(option_type), forward = as.numeric(forward), strike = as.numeric(strike),
              volatility = as.numeric(volatility), time = as.numeric(time), discount = as.numeric(discount))
  option_table(.Call(wrap__compute_forward_option_value, i$model, i$option_type, i$forward, i$strike, i$volatility, i$time, i$discount), i, on_error)
}

#' Implied volatility of a European option
#'
#' Searches between 1e-7 and 4 with Brent's method.
#'
#' @inheritParams compute_european_option_value
#' @param price Option price.
#' @param accuracy Accuracy on the volatility.
#' @param max_evaluations Maximum evaluations of the price.
#' @return A numeric vector.
#' @examples
#' compute_implied_volatility("call", 13.7, 100, 105, 0.03, 0.01, 1.5)
#' @export
compute_implied_volatility <- function(option_type, price, spot, strike, rate, dividend_yield, time, accuracy = 1e-10, max_evaluations = 200L, on_error = "stop") {
  i <- tibble(option_type = as.character(option_type), price = as.numeric(price), spot = as.numeric(spot), strike = as.numeric(strike),
              rate = as.numeric(rate), dividend_yield = as.numeric(dividend_yield), time = as.numeric(time))
  finish_values(.Call(wrap__compute_implied_volatility, i$option_type, i$price, i$spot, i$strike, i$rate, i$dividend_yield, i$time,
                      as.numeric(accuracy), as.integer(max_evaluations)), on_error)
}

#' Monte Carlo of a European option
#'
#' Pathwise sensitivities by automatic differentiation on path vectors, in place of `torch`. The
#' result depends on the seed, not on the number of threads. The kink of the payoff has a pathwise
#' gamma of 0, so `gamma` is `NA` unless `smoothing` is positive, and then it is the gamma of the
#' smoothed payoff `(x + sqrt(x^2 + smoothing^2)) / 2`.
#'
#' @inheritParams compute_european_option_value
#' @param paths Number of paths; even with `antithetic`.
#' @param seed A whole number, used to compute each path's random number.
#' @param antithetic `TRUE` or `FALSE`. `NA` is an error.
#' @param smoothing Width of the smoothed payoff, or 0 for the plain payoff.
#' @param threads Worker threads; 0 means one per logical processor.
#' @return A one-row tibble with the columns `value`, `standard_error`, `delta`, `gamma`, `vega`,
#'   `rho`, `dividend_rho` and `theta`.
#' @examples
#' compute_monte_carlo_european_value("call", 100, 105, 0.03, 0.01, 0.25, 1.5, paths = 1e5, seed = 1)
#' @export
compute_monte_carlo_european_value <- function(option_type, spot, strike, rate, dividend_yield, volatility, time, paths, seed, antithetic = TRUE, smoothing = 0, threads = 0L) {
  tibble::as_tibble(checked(.Call(wrap__compute_monte_carlo_european_value, option_type, as.numeric(spot), as.numeric(strike), as.numeric(rate), as.numeric(dividend_yield),
                                  as.numeric(volatility), as.numeric(time), as.numeric(paths), as.numeric(seed), as_flag(antithetic), as.numeric(smoothing), as.integer(threads))))
}
