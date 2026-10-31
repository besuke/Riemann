# OIS curves for TONAR, ESTR, SOFR and SONIA. Names are those of the Rust functions.

#' The instruments of a curve. A tenor of `ON` is an overnight deposit. Settlement days, payment
#' frequency and payment lag have no defaults, because QuantLib's defaults are not market
#' conventions.
ois_instruments <- function(tenor, rate, settlement_days, payment_frequency, payment_lag) {
  tibble(tenor = as.character(tenor), rate = as.numeric(rate), settlement_days = as.integer(settlement_days),
         payment_frequency = as.integer(payment_frequency), payment_lag = as.integer(payment_lag))
}

#' Builds an OIS curve. `index` is tonar, estr, sofr or sonia. `interpolation` is
#' monotonic_log_cubic, the method of the specification, or log_linear. `accuracy` applies to
#' discount factors. Check `quote_repricing()`: a curve whose discount factors come close to 0
#' can miss its quotes.
build_ois_curve <- function(evaluation_date, reference_date, index, curve_day_count, instruments,
                            interpolation = "monotonic_log_cubic", accuracy = 1e-12) {
  pointer <- checked(.Call(
    wrap__build_ois_curve, as_serial(evaluation_date), as_serial(reference_date), index, curve_day_count, instruments$tenor,
    instruments$rate, instruments$settlement_days, instruments$payment_frequency, instruments$payment_lag, interpolation,
    as.numeric(accuracy)
  ))$value
  structure(list(pointer = pointer, index = index, interpolation = interpolation, instruments = instruments),
            class = "riemann_ois_curve")
}

#' @export
print.riemann_ois_curve <- function(x, ...) {
  cat("<riemann_ois_curve>", x$index, x$interpolation, "with", nrow(x$instruments), "instruments\n")
  invisible(x)
}

#' The same curve with extrapolation allowed or not; the original does not change.
with_extrapolation <- function(curve, extrapolate = TRUE) {
  curve$pointer <- .Call(wrap__with_extrapolation, curve$pointer, as_flag(extrapolate))
  curve
}

#' Discount factors at each date.
discount <- function(curve, date, on_error = "stop") {
  finish_values(.Call(wrap__discount, curve$pointer, as_serial(date)), on_error)
}

#' Zero rates to each date.
zero_rate <- function(curve, date, day_count, compounding, frequency = 1L, on_error = "stop") {
  finish_values(checked(.Call(wrap__zero_rate, curve$pointer, as_serial(date), day_count, compounding, as.integer(frequency))), on_error)
}

#' Forward rates between dates. Equal dates give the rate over 0.0001 years around the date.
forward_rate <- function(curve, start_date, end_date, day_count, compounding, frequency = 1L, on_error = "stop") {
  inputs <- tibble(start = as_serial(start_date), end = as_serial(end_date))
  finish_values(checked(.Call(wrap__forward_rate, curve$pointer, inputs$start, inputs$end, day_count, compounding, as.integer(frequency))), on_error)
}

#' The dates of each instrument and how the curve reprices its quote.
quote_repricing <- function(curve) {
  rows <- .Call(wrap__quote_repricing, curve$pointer)
  tibble(
    tenor = curve$instruments$tenor, rate = rows$rate, start_date = as_date(rows$start_date),
    maturity_date = as_date(rows$maturity_date), pillar_date = as_date(rows$pillar_date),
    latest_relevant_date = as_date(rows$latest_relevant_date), used_in_bootstrap = rows$used_in_bootstrap,
    implied_rate = rows$implied_rate, reproduction_error = abs(rows$implied_rate - rows$rate)
  )
}

#' The nodes of the curve.
discount_nodes <- function(curve) {
  rows <- .Call(wrap__discount_nodes, curve$pointer)
  tibble(date = as_date(rows$date), discount = rows$discount)
}

#' Discount factor, continuous zero rate and one-day simple forward rate at each date. The rates use the curve's day count.
curve_values <- function(curve, date, day_count, on_error = "stop") {
  serials <- as_serial(date)
  dates <- as_date(serials)
  tibble(
    date = dates,
    discount = discount(curve, dates, on_error),
    zero_continuous = zero_rate(curve, dates, day_count, "continuous", 1L, on_error),
    forward_1d_simple = forward_rate(curve, dates, dates + 1L, day_count, "simple", 1L, on_error)
  )
}
