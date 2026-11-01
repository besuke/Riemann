# OIS curves for TONAR, ESTR, SOFR and SONIA. Names are those of the Rust functions.

#' Instruments of an OIS curve
#'
#' Settlement days, payment frequency and payment lag have no defaults, because QuantLib's defaults
#' are not market conventions.
#'
#' @param tenor Tenors such as `"1W"`, `"6M"`, `"10Y"`. `"ON"` is an overnight deposit, which does
#'   not use the settlement days, frequency or lag.
#' @param rate Quotes as decimals.
#' @param settlement_days Business days to the start of each OIS.
#' @param payment_frequency Payments per year: 1, 2, 4 or 12.
#' @param payment_lag Business days from the end of a period to its payment.
#' @return A tibble, to pass to [build_ois_curve()].
#' @examples
#' ois_instruments(c("ON", "1Y", "5Y"), c(0.002, 0.004, 0.009), 2L, 1L, 0L)
#' @export
ois_instruments <- function(tenor, rate, settlement_days, payment_frequency, payment_lag) {
  tibble(tenor = as.character(tenor), rate = as.numeric(rate), settlement_days = as.integer(settlement_days),
         payment_frequency = as.integer(payment_frequency), payment_lag = as.integer(payment_lag))
}

#' Build an OIS discount curve
#'
#' Bootstraps a curve from overnight deposits and OIS quotes, following QuantLib's iterative
#' bootstrap.
#'
#' Check [quote_repricing()]. `accuracy` applies to discount factors, so a curve whose discount
#' factors come close to 0 can miss its quotes, in QuantLib as well.
#'
#' @param evaluation_date,reference_date Dates: `Date` or ISO strings.
#' @param index `"tonar"`, `"estr"`, `"sofr"` or `"sonia"`.
#' @param curve_day_count Day count of the curve's times; see [RiemannFinMath-package].
#' @param instruments A table from [ois_instruments()].
#' @param interpolation `"monotonic_log_cubic"`, the method of the specification, or `"log_linear"`.
#' @param accuracy Accuracy on discount factors.
#' @return A `riemann_ois_curve` value. It does not change.
#' @examples
#' instruments <- ois_instruments(
#'   c("ON", "1Y", "5Y", "10Y"), c(0.002, 0.004, 0.009, 0.014), 2L, 1L, 0L
#' )
#' curve <- build_ois_curve("2026-11-02", "2026-11-02", "sofr", "actual_365_fixed", instruments)
#' curve
#' @export
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

#' Allow a curve to extrapolate
#'
#' @param curve A curve from [build_ois_curve()].
#' @param extrapolate `TRUE` or `FALSE`. `NA` is an error.
#' @return A new curve; the original does not change.
#' @examples
#' instruments <- ois_instruments(c("ON", "1Y", "5Y"), c(0.002, 0.004, 0.009), 2L, 1L, 0L)
#' curve <- build_ois_curve("2026-11-02", "2026-11-02", "estr", "actual_365_fixed", instruments)
#' discount(with_extrapolation(curve), "2040-01-01")
#' @export
with_extrapolation <- function(curve, extrapolate = TRUE) {
  curve$pointer <- .Call(wrap__with_extrapolation, curve$pointer, as_flag(extrapolate))
  curve
}

#' Read a curve
#'
#' `discount()`, `zero_rate()` and `forward_rate()` return one number for each date.
#' `curve_values()` returns a table with the discount factor, the continuous zero rate and the
#' one-day simple forward rate.
#'
#' @param curve A curve from [build_ois_curve()].
#' @param date Dates: `Date` or ISO strings.
#' @param start_date,end_date Dates of the forward period. Equal dates give the rate over 0.0001
#'   years around the date, as in QuantLib.
#' @param day_count Day count of the rate; see [RiemannFinMath-package].
#' @param compounding `"simple"`, `"compounded"` or `"continuous"`.
#' @param frequency Compounding periods per year.
#' @param on_error `"stop"` or `"na"`; see [RiemannFinMath-package].
#' @return A numeric vector, or a tibble for `curve_values()`.
#' @examples
#' instruments <- ois_instruments(
#'   c("ON", "1Y", "5Y", "10Y"), c(0.002, 0.004, 0.009, 0.014), 2L, 1L, 0L
#' )
#' curve <- build_ois_curve("2026-11-02", "2026-11-02", "sonia", "actual_365_fixed", instruments)
#' discount(curve, "2031-11-03")
#' zero_rate(curve, "2031-11-03", "actual_365_fixed", "continuous")
#' forward_rate(curve, "2030-01-02", "2030-01-03", "actual_365_fixed", "simple")
#' curve_values(curve, as.Date("2026-11-02") + c(365L, 1825L), "actual_365_fixed")
#' @export
discount <- function(curve, date, on_error = "stop") {
  finish_values(.Call(wrap__discount, curve$pointer, as_serial(date)), on_error)
}

#' @rdname discount
#' @export
zero_rate <- function(curve, date, day_count, compounding, frequency = 1L, on_error = "stop") {
  finish_values(checked(.Call(wrap__zero_rate, curve$pointer, as_serial(date), day_count, compounding, as.integer(frequency))), on_error)
}

#' @rdname discount
#' @export
forward_rate <- function(curve, start_date, end_date, day_count, compounding, frequency = 1L, on_error = "stop") {
  inputs <- tibble(start = as_serial(start_date), end = as_serial(end_date))
  finish_values(checked(.Call(wrap__forward_rate, curve$pointer, inputs$start, inputs$end, day_count, compounding, as.integer(frequency))), on_error)
}

#' @rdname discount
#' @export
curve_values <- function(curve, date, day_count, on_error = "stop") {
  dates <- as_date(as_serial(date))
  tibble(
    date = dates,
    discount = discount(curve, dates, on_error),
    zero_continuous = zero_rate(curve, dates, day_count, "continuous", 1L, on_error),
    forward_1d_simple = forward_rate(curve, dates, dates + 1L, day_count, "simple", 1L, on_error)
  )
}

#' How a curve reprices its quotes, and its nodes
#'
#' A curve can agree with QuantLib and still miss a quote; `reproduction_error` shows how far.
#'
#' @param curve A curve from [build_ois_curve()].
#' @return `quote_repricing()` returns a tibble with the columns `tenor`, `rate`, `start_date`,
#'   `maturity_date`, `pillar_date`, `latest_relevant_date`, `used_in_bootstrap` (logical),
#'   `implied_rate` and `reproduction_error`. `discount_nodes()` returns a tibble with the columns
#'   `date` and `discount`.
#' @examples
#' instruments <- ois_instruments(c("ON", "1Y", "5Y"), c(0.002, 0.004, 0.009), 2L, 1L, 0L)
#' curve <- build_ois_curve("2026-11-02", "2026-11-02", "tonar", "actual_365_fixed", instruments)
#' quote_repricing(curve)
#' discount_nodes(curve)
#' @export
quote_repricing <- function(curve) {
  rows <- .Call(wrap__quote_repricing, curve$pointer)
  tibble(
    tenor = curve$instruments$tenor, rate = rows$rate, start_date = as_date(rows$start_date),
    maturity_date = as_date(rows$maturity_date), pillar_date = as_date(rows$pillar_date),
    latest_relevant_date = as_date(rows$latest_relevant_date), used_in_bootstrap = rows$used_in_bootstrap,
    implied_rate = rows$implied_rate, reproduction_error = abs(rows$implied_rate - rows$rate)
  )
}

#' @rdname quote_repricing
#' @export
discount_nodes <- function(curve) {
  rows <- .Call(wrap__discount_nodes, curve$pointer)
  tibble(date = as_date(rows$date), discount = rows$discount)
}
