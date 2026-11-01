# Options. Calculations and sensitivities are in Rust; time is in years and rates are continuous.

option_table <- function(rows, inputs, on_error) {
  rows <- finish_rows(rows, on_error)
  values <- tibble::as_tibble(rows[setdiff(names(rows), "error")])
  tibble::as_tibble(c(as.list(inputs), as.list(values), list(error = rows$error)))
}

#' Options on an equity
#'
#' Prices with sensitivities by automatic differentiation, under Black-Scholes-Merton with a flat
#' continuous rate, dividend yield and volatility. `barrier_option()` monitors the barrier
#' continuously. `binomial_option()` is a Cox-Ross-Rubinstein tree; its value is piecewise linear in
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
#' european_option("call", 100, 105, 0.03, 0.01, 0.25, 1.5)
#' barrier_option("call", "down_out", 100, 105, 80, 0, 0.03, 0.01, 0.25, 1.5)
#' binomial_option("put", "american", 100, 110, 0.03, 0.01, 0.25, 1.5)
#' @export
european_option <- function(option_type, spot, strike, rate, dividend_yield, volatility, time, on_error = "stop") {
  i <- tibble(option_type = as.character(option_type), spot = as.numeric(spot), strike = as.numeric(strike), rate = as.numeric(rate),
              dividend_yield = as.numeric(dividend_yield), volatility = as.numeric(volatility), time = as.numeric(time))
  option_table(.Call(wrap__european_option, i$option_type, i$spot, i$strike, i$rate, i$dividend_yield, i$volatility, i$time), i, on_error)
}

#' @rdname european_option
#' @export
barrier_option <- function(option_type, barrier_type, spot, strike, barrier, rebate, rate, dividend_yield, volatility, time, on_error = "stop") {
  i <- tibble(option_type = as.character(option_type), barrier_type = as.character(barrier_type), spot = as.numeric(spot), strike = as.numeric(strike),
              barrier = as.numeric(barrier), rebate = as.numeric(rebate), rate = as.numeric(rate),
              dividend_yield = as.numeric(dividend_yield), volatility = as.numeric(volatility), time = as.numeric(time))
  option_table(.Call(wrap__barrier_option, i$option_type, i$barrier_type, i$spot, i$strike, i$barrier, i$rebate, i$rate, i$dividend_yield, i$volatility, i$time), i, on_error)
}

#' @rdname european_option
#' @export
binomial_option <- function(option_type, exercise, spot, strike, rate, dividend_yield, volatility, time, steps = 200L, on_error = "stop") {
  i <- tibble(option_type = as.character(option_type), exercise = as.character(exercise), spot = as.numeric(spot), strike = as.numeric(strike), rate = as.numeric(rate),
              dividend_yield = as.numeric(dividend_yield), volatility = as.numeric(volatility), time = as.numeric(time), steps = as.integer(steps))
  option_table(.Call(wrap__binomial_option, i$option_type, i$exercise, i$spot, i$strike, i$rate, i$dividend_yield, i$volatility, i$time, i$steps), i, on_error)
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
#' forward_option("black", "call", 100, 105, 0.25, 1.5, 0.96)
#' forward_option("bachelier", "put", 0.02, 0.025, 0.006, 2, 0.95)
#' @export
forward_option <- function(model, option_type, forward, strike, volatility, time, discount, on_error = "stop") {
  i <- tibble(model = as.character(model), option_type = as.character(option_type), forward = as.numeric(forward), strike = as.numeric(strike),
              volatility = as.numeric(volatility), time = as.numeric(time), discount = as.numeric(discount))
  option_table(.Call(wrap__forward_option, i$model, i$option_type, i$forward, i$strike, i$volatility, i$time, i$discount), i, on_error)
}

#' Implied volatility of a European option
#'
#' Searches between 1e-7 and 4 with Brent's method.
#'
#' @inheritParams european_option
#' @param price Option price.
#' @param accuracy Accuracy on the volatility.
#' @param max_evaluations Maximum evaluations of the price.
#' @return A numeric vector.
#' @examples
#' implied_volatility("call", 13.7, 100, 105, 0.03, 0.01, 1.5)
#' @export
implied_volatility <- function(option_type, price, spot, strike, rate, dividend_yield, time, accuracy = 1e-10, max_evaluations = 200L, on_error = "stop") {
  i <- tibble(option_type = as.character(option_type), price = as.numeric(price), spot = as.numeric(spot), strike = as.numeric(strike),
              rate = as.numeric(rate), dividend_yield = as.numeric(dividend_yield), time = as.numeric(time))
  finish_values(.Call(wrap__implied_volatility, i$option_type, i$price, i$spot, i$strike, i$rate, i$dividend_yield, i$time,
                      as.numeric(accuracy), as.integer(max_evaluations)), on_error)
}

#' Monte Carlo of a European option
#'
#' Pathwise sensitivities by automatic differentiation on path vectors, in place of `torch`. The
#' result depends on the seed, not on the number of threads. The kink of the payoff has a pathwise
#' gamma of 0, so `gamma` is `NA` unless `smoothing` is positive, and then it is the gamma of the
#' smoothed payoff `(x + sqrt(x^2 + smoothing^2)) / 2`.
#'
#' @inheritParams european_option
#' @param paths Number of paths; even with `antithetic`.
#' @param seed A whole number, used to compute each path's random number.
#' @param antithetic `TRUE` or `FALSE`. `NA` is an error.
#' @param smoothing Width of the smoothed payoff, or 0 for the plain payoff.
#' @param threads Worker threads; 0 means one per logical processor.
#' @return A one-row tibble with the columns `value`, `standard_error`, `delta`, `gamma`, `vega`,
#'   `rho`, `dividend_rho` and `theta`.
#' @examples
#' monte_carlo_european("call", 100, 105, 0.03, 0.01, 0.25, 1.5, paths = 1e5, seed = 1)
#' @export
monte_carlo_european <- function(option_type, spot, strike, rate, dividend_yield, volatility, time, paths, seed, antithetic = TRUE, smoothing = 0, threads = 0L) {
  tibble::as_tibble(checked(.Call(wrap__monte_carlo_european, option_type, as.numeric(spot), as.numeric(strike), as.numeric(rate), as.numeric(dividend_yield),
                                  as.numeric(volatility), as.numeric(time), as.numeric(paths), as.numeric(seed), as_flag(antithetic), as.numeric(smoothing), as.integer(threads))))
}
