#' Sobol points and Black-Scholes paths
#'
#' `generate_sobol_sequence()` returns the points of QuantLib's default Sobol sequence (direction
#' integers of Jaeckel, Gray code), from its first point, 0.5 in every dimension. There is no seed, no
#' skipping and no scrambling, and the dimension is at most 32. `generate_black_scholes_paths()` turns
#' them, or pseudo-random numbers, into paths with exact steps; with `sequence = "sobol"` dimension `i`
#' drives step `i`, unless `brownian_bridge = TRUE`. The points of an unscrambled Sobol sequence are not
#' independent samples.
#'
#' @param dimension Number of dimensions, from 1 to 32.
#' @param points Number of points.
#' @param spot,rate,dividend_yield,volatility,time Black-Scholes inputs; rates are continuous, `time` in years.
#' @param steps Number of equal steps; at most 32 for a Sobol sequence.
#' @param paths Number of paths.
#' @param sequence `"sobol"` or `"pseudo"`.
#' @param brownian_bridge Build Sobol paths by the Brownian bridge.
#' @param seed For `"pseudo"`: a whole number that selects the random numbers. Not used for `"sobol"`.
#' @return `generate_sobol_sequence()` a matrix with one row per point. `generate_black_scholes_paths()` a
#'   tibble with `path_id`, `step`, `time` and `price`.
#' @examples
#' generate_sobol_sequence(2, 4)
#' generate_black_scholes_paths(100, 0.05, 0, 0.2, 1, steps = 4, paths = 3)
#' @export
generate_sobol_sequence <- function(dimension, points) {
  values <- checked(.Call(wrap__generate_sobol_sequence, as.integer(dimension), as.numeric(points)))$values
  matrix(values, ncol = as.integer(dimension), byrow = TRUE)
}

#' @rdname generate_sobol_sequence
#' @export
generate_black_scholes_paths <- function(spot, rate, dividend_yield, volatility, time, steps, paths, sequence = "sobol",
                                         brownian_bridge = FALSE, seed = 1) {
  prices <- checked(.Call(wrap__generate_black_scholes_paths, as.numeric(spot), as.numeric(rate), as.numeric(dividend_yield),
                          as.numeric(volatility), as.numeric(time), as.integer(steps), as.numeric(paths), as.character(sequence),
                          as_flag(brownian_bridge), as.numeric(seed)))$prices
  steps <- as.integer(steps)
  tibble::tibble(
    path_id = rep(seq_len(length(prices) %/% (steps + 1L)), each = steps + 1L),
    step = rep(0:steps, length(prices) %/% (steps + 1L)),
    time = step * as.numeric(time) / steps,
    price = prices
  )
}

#' Longstaff-Schwartz value of an American or Bermudan option
#'
#' Regresses the discounted cash flow already chosen on powers of `S / K`, using the paths in the money,
#' and exercises where the immediate value exceeds the fitted continuation value. Exercise is allowed at
#' every step from 1, or at `exercise_steps` and maturity. The choices that QuantLib and the legacy text
#' make differently are arguments:
#'
#' * LagrangeFinance II-4.6: `basis_degree = 2`, `include_payoff = FALSE`, `calibration = "in_sample"`,
#'   `few_in_the_money = "hold"`.
#' * QuantLib `MCLDAmericanEngine`: `include_payoff = TRUE`, `calibration = "overlapping"` with
#'   `calibration_paths` its `nCalibrationSamples`, `few_in_the_money = "exercise"`, `sequence = "sobol"`.
#'
#' The value from `"in_sample"` is biased upward. `"separate"` regresses on `calibration_paths` paths and
#' values on the paths after them; with `"pseudo"` the two sets are independent, but with `"sobol"` they
#' come from one deterministic sequence. The standard error is `sd / sqrt(paths)`; for a Sobol sequence it
#' is not a confidence interval.
#'
#' @inheritParams generate_sobol_sequence
#' @param option_type `"call"` or `"put"`.
#' @param strike Strike.
#' @param basis_degree Highest power of `S / K`; 0 is a constant.
#' @param include_payoff Add the payoff to the regression, as QuantLib does.
#' @param calibration `"in_sample"`, `"overlapping"` or `"separate"`.
#' @param calibration_paths Number of regression paths for `"overlapping"` and `"separate"`.
#' @param few_in_the_money At a date with fewer paths in the money than basis functions: `"hold"` or `"exercise"`.
#' @param exercise_steps `NULL` for every step, or the steps that allow exercise besides maturity.
#' @return A one-row tibble with `value`, `standard_error`, `european_value` (the same paths without early
#'   exercise), `exercise_probability`, `unexercised_paths`, `dates_with_too_few_paths`, and the list columns
#'   `exercise` (paths stopping at each exercise step) and `regressions` (one row per date, from the last
#'   back, with the coefficients in the order 1, x, x^2, ..., payoff, and the highest spot exercised for a
#'   put or the lowest for a call).
#' @examples
#' compute_longstaff_schwartz_value("put", 100, 100, 0.05, 0, 0.2, 1, steps = 12, paths = 2048)
#' @export
compute_longstaff_schwartz_value <- function(option_type, spot, strike, rate, dividend_yield, volatility, time, steps, paths,
                                             sequence = "sobol", brownian_bridge = FALSE, seed = 1, basis_degree = 2L,
                                             include_payoff = FALSE, calibration = "in_sample", calibration_paths = paths,
                                             few_in_the_money = "hold", exercise_steps = NULL) {
  r <- checked(.Call(
    wrap__compute_longstaff_schwartz_value, as.character(option_type), as.numeric(spot), as.numeric(strike), as.numeric(rate),
    as.numeric(dividend_yield), as.numeric(volatility), as.numeric(time), as.integer(steps), as.numeric(paths), as.character(sequence),
    as_flag(brownian_bridge), as.numeric(seed), as.integer(basis_degree), as_flag(include_payoff), as.character(calibration),
    as.numeric(calibration_paths), as.character(few_in_the_money), as.integer(if (is.null(exercise_steps)) integer() else exercise_steps)
  ))
  coefficients <- lapply(seq_along(r$coefficient_counts), function(i) if (r$coefficient_counts[i] == 0L) NULL else
    r$coefficients[sum(r$coefficient_counts[seq_len(i - 1L)]) + seq_len(r$coefficient_counts[i])])
  tibble::tibble(
    value = r$value, standard_error = r$standard_error, european_value = r$european_value,
    exercise_probability = r$exercise_probability, unexercised_paths = r$unexercised_paths,
    dates_with_too_few_paths = r$dates_with_too_few_paths,
    exercise = list(tibble::tibble(step = r$exercise_step, time = r$exercise_time, paths = r$exercise_paths)),
    regressions = list(tibble::tibble(
      step = r$step, time = r$time, in_the_money_paths = r$in_the_money_paths, exercised_paths = r$exercised_paths,
      mean_immediate_value = r$mean_immediate_value, mean_realized_continuation = r$mean_realized_continuation,
      exercise_boundary = r$exercise_boundary, too_few_paths = r$too_few_paths, coefficients = coefficients
    ))
  )
}
