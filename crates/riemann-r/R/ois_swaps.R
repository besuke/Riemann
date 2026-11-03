# Overnight indexed swaps. Calculations are in Rust; fixings are an argument, and no state is kept.

#' Overnight indexed swap
#'
#' A fixed leg against a leg that compounds an overnight index daily, as QuantLib's `MakeOIS` with an
#' effective and a termination date. Both schedules run backward on the index's calendar and are
#' modified following, and end of month when the effective date is. Fixed payments adjust modified
#' following and overnight payments following.
#'
#' There is no lookback, lockout, observation shift or arithmetic averaging. The spread is added to
#' the compounded rate, and it does not compound daily.
#'
#' @param side `"payer"` pays the fixed leg, and `"receiver"` receives it.
#' @param nominal The notional.
#' @param effective_date,termination_date Dates: `Date` or ISO strings.
#' @param index `"tonar"`, `"estr"`, `"sofr"` or `"sonia"`.
#' @param fixed_rate Fixed rate as a decimal.
#' @param spread Spread on the overnight leg as a decimal.
#' @param fixed_frequency,overnight_frequency Payments per year: 1, 2, 4 or 12. The defaults are
#'   QuantLib's and not market conventions.
#' @param payment_lag Business days from the end of a period to its payment.
#' @param fixed_day_count Day count of the fixed leg, or `NULL` for the index's.
#' @return A `riemann_ois_swap` value, which does not change.
#' @examples
#' swap <- create_ois_swap("payer", 1e7, "2026-11-04", "2031-11-04", "tonar", 0.012)
#' swap
#' @export
create_ois_swap <- function(side, nominal, effective_date, termination_date, index, fixed_rate, spread = 0,
                     fixed_frequency = 1L, overnight_frequency = 1L, payment_lag = 0L, fixed_day_count = NULL) {
  pointer <- checked(.Call(
    wrap__create_ois_swap, side, as.numeric(nominal), as_serial(effective_date), as_serial(termination_date), index, as.numeric(fixed_rate),
    if (is.null(fixed_day_count)) "" else fixed_day_count, as.numeric(spread), as.integer(fixed_frequency), as.integer(overnight_frequency),
    as.integer(payment_lag)
  ))$value
  structure(list(pointer = pointer, side = side, index = index, fixed_rate = fixed_rate,
                 termination_date = as_date(as_serial(termination_date))), class = "riemann_ois_swap")
}

#' @export
print.riemann_ois_swap <- function(x, ...) {
  cat("<riemann_ois_swap>", x$side, x$index, "fixed", x$fixed_rate, "maturing", format(x$termination_date), "\n")
  invisible(x)
}

# Fixings as a table, or nothing.
fixing_columns <- function(fixings) {
  if (is.null(fixings)) return(list(dates = integer(), rates = numeric()))
  missing_columns <- setdiff(c("date", "rate"), names(fixings))
  if (length(missing_columns)) stop("`fixings` lacks the columns: ", paste(missing_columns, collapse = ", "), call. = FALSE)
  if (anyNA(fixings$rate)) stop("`fixings` has a missing rate.", call. = FALSE)
  if (anyDuplicated(as_serial(fixings$date))) stop("`fixings` has a date twice.", call. = FALSE)
  list(dates = as_serial(fixings$date), rates = as.numeric(fixings$rate))
}

#' Value of an overnight indexed swap
#'
#' Each coupon compounds the published fixings before the evaluation date, today's fixing if it is
#' given, and the ratio of the curve's discount factors after that. A missing fixing before the
#' evaluation date is an error, unless the coupon has been paid. The value is at the discount curve's
#' reference date, and flows paid on or before that date have occurred. A swap whose flows have all
#' been paid by the evaluation date is worth 0, as in QuantLib.
#'
#' @param swap A swap from [create_ois_swap()].
#' @param curve A curve from [bootstrap_ois_curve()] of the swap's index, which forecasts the overnight rates.
#' @param evaluation_date One date.
#' @param fixings `NULL`, or a data frame with the columns `date` and `rate`. Rates are decimals.
#' @param discount_curve A curve that discounts. The default is `curve`.
#' @param coupon The position of the coupon in the overnight leg.
#' @param on_error `"stop"` or `"na"`; see [RiemannFinMath-package].
#' @return `compute_ois_swap_value()` returns a one-row tibble with `npv`, `fixed_leg_npv`, `overnight_leg_npv`,
#'   `fixed_leg_bps`, `overnight_leg_bps` (the change for 1bp, with the leg's sign), `fair_rate` and
#'   `fair_spread`. These two are `NA` when no coupon of that leg is left to pay.
#'   `compute_ois_swap_cashflows()` returns a tibble with `leg`, `payment_date`, `accrual_start`, `accrual_end`,
#'   `amount` (unsigned) and `error`. `compute_ois_swap_daily_rates()` returns a tibble with `fixing_date`,
#'   `next_date`, `days`, `day_fraction`, `fixing`, `forward_rate` and `applied_rate`.
#'   `compute_ois_swap_quote_sensitivity()` returns a tibble with `tenor`, `rate`, `pillar_date` and `npv_per_bp`,
#'   the change of the value for +1bp on the quote with the curve following; the curve both forecasts
#'   and discounts.
#' @examples
#' instruments <- create_ois_instruments(c("ON", "1Y", "5Y", "10Y"), c(0.002, 0.004, 0.009, 0.014), 2L, 1L, 0L)
#' curve <- bootstrap_ois_curve("2026-11-02", "2026-11-02", "tonar", "actual_365_fixed", instruments)
#' swap <- create_ois_swap("payer", 1e7, "2026-11-04", "2031-11-04", "tonar", 0.012)
#' compute_ois_swap_value(swap, curve, "2026-11-02")
#' compute_ois_swap_cashflows(swap, curve, "2026-11-02")
#' head(compute_ois_swap_daily_rates(swap, curve, "2026-11-02"))
#' compute_ois_swap_quote_sensitivity(swap, curve, "2026-11-02")
#' started <- tibble::tibble(date = as.Date(c("2026-11-04", "2026-11-05")), rate = c(0.002, 0.0021))
#' compute_ois_swap_value(swap, curve, "2026-11-06", started)
#' @export
compute_ois_swap_value <- function(swap, curve, evaluation_date, fixings = NULL, discount_curve = curve) {
  f <- fixing_columns(fixings)
  tibble::as_tibble(checked(.Call(wrap__compute_ois_swap_value, swap$pointer, curve$pointer, discount_curve$pointer, as_serial(evaluation_date), f$dates, f$rates)))
}

#' @rdname compute_ois_swap_value
#' @export
compute_ois_swap_cashflows <- function(swap, curve, evaluation_date, fixings = NULL, on_error = "stop") {
  f <- fixing_columns(fixings)
  rows <- finish_rows(checked(.Call(wrap__compute_ois_swap_cashflows, swap$pointer, curve$pointer, as_serial(evaluation_date), f$dates, f$rates)), on_error)
  tibble(leg = rows$leg, payment_date = as_date(rows$payment_date), accrual_start = as_date(rows$accrual_start),
         accrual_end = as_date(rows$accrual_end), amount = rows$amount, error = rows$error)
}

#' @rdname compute_ois_swap_value
#' @export
compute_ois_swap_daily_rates <- function(swap, curve, evaluation_date, fixings = NULL, coupon = 1L) {
  f <- fixing_columns(fixings)
  rows <- checked(.Call(wrap__compute_ois_swap_daily_rates, swap$pointer, curve$pointer, as.integer(coupon), as_serial(evaluation_date), f$dates, f$rates))
  fixing_date <- as_date(rows$fixing_date)
  next_date <- as_date(rows$next_date)
  tibble(fixing_date = fixing_date, next_date = next_date, days = as.integer(next_date - fixing_date), day_fraction = rows$day_fraction,
         fixing = rows$fixing, forward_rate = rows$forward_rate, applied_rate = rows$applied_rate)
}

#' @rdname compute_ois_swap_value
#' @export
compute_ois_swap_quote_sensitivity <- function(swap, curve, evaluation_date, fixings = NULL) {
  f <- fixing_columns(fixings)
  rows <- checked(.Call(wrap__compute_ois_swap_quote_sensitivity, swap$pointer, curve$pointer, as_serial(evaluation_date), f$dates, f$rates))
  tibble(tenor = curve$instruments$tenor[rows$instrument], rate = rows$rate, pillar_date = as_date(rows$pillar_date), npv_per_bp = rows$d_npv * 1e-4)
}
