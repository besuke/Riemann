#' Curves given directly
#'
#' Yield curves built from rates or discount factors that you give, rather than bootstrapped from quotes.
#' They work wherever a bootstrapped curve does: discount factors, zero and forward rates, bonds, OIS swaps,
#' CDS. They have no quotes, so the functions of sensitivities to quotes refuse them.
#'
#' * `create_flat_curve()`: one rate (QuantLib `FlatForward`).
#' * `create_zero_curve()`: zero rates at dates, linear in the continuous zero rate between them (QuantLib
#'   `ZeroCurve`); the first date is the reference date. Rates in another compounding are converted to
#'   continuous rates at each date.
#' * `create_discount_curve()`: discount factors at dates, the first of which is 1 on the reference date
#'   (QuantLib `DiscountCurve`).
#' * `create_zero_spreaded_curve()`: another curve with a spread added to its zero rates in the given
#'   compounding (QuantLib `ZeroSpreadedTermStructure`).
#'
#' The reference date is fixed: it does not move with an evaluation date, unlike QuantLib curves built
#' with a number of settlement days. Pass the date you want.
#'
#' @param reference_date The curve's reference date.
#' @param rate,rates Annual rates.
#' @param dates Dates, increasing; the first is the reference date.
#' @param discounts Discount factors at `dates`.
#' @param day_count Day count of the curve's times.
#' @param compounding,frequency How `rate`, `rates` or `spread` compound: `"continuous"`, `"compounded"`
#'   with `frequency` per year, or `"simple"`.
#' @param interpolation `"log_linear"` or `"monotonic_log_cubic"` on the discount factors.
#' @param index The overnight index whose conventions the curve forecasts with, in OIS swaps and asset
#'   swaps: `"tonar"`, `"estr"`, `"sofr"` or `"sonia"`. It does not matter for discounting.
#' @param curve A curve from any of these functions or from [bootstrap_ois_curve()].
#' @param spread Spread added to the zero rates.
#' @return A curve, of class `riemann_ois_curve`.
#' @examples
#' flat <- create_flat_curve("2024-05-02", 0.04, "actual_360")
#' compute_discount(flat, as.Date("2029-05-02"))
#' zero <- create_zero_curve(as.Date("2022-12-08") + c(0, 365, 3650), c(0.015, 0.018, 0.025), "actual_360")
#' compute_zero_rate(create_zero_spreaded_curve(zero, 0.01), as.Date("2030-01-01"), "actual_360", "continuous")
#' @export
create_flat_curve <- function(reference_date, rate, day_count, compounding = "continuous", frequency = 1L, index = "tonar") {
  given(checked(.Call(wrap__create_flat_curve, as_serial(reference_date), as.numeric(rate), day_count, compounding,
                      as.integer(frequency), index))$value, "flat", index)
}

#' @rdname create_flat_curve
#' @export
create_zero_curve <- function(dates, rates, day_count, compounding = "continuous", frequency = 1L, index = "tonar") {
  given(checked(.Call(wrap__create_zero_curve, as_serial(dates), as.numeric(rates), day_count, compounding,
                      as.integer(frequency), index))$value, "zero-rate", index)
}

#' @rdname create_flat_curve
#' @export
create_discount_curve <- function(dates, discounts, day_count, interpolation = "log_linear", index = "tonar") {
  given(checked(.Call(wrap__create_discount_curve, as_serial(dates), as.numeric(discounts), day_count, interpolation,
                      index))$value, "discount-factor", index)
}

#' @rdname create_flat_curve
#' @export
create_zero_spreaded_curve <- function(curve, spread, compounding = "continuous", frequency = 1L) {
  if (!inherits(curve, "riemann_ois_curve")) stop("`curve` must be a curve.", call. = FALSE)
  given(checked(.Call(wrap__create_zero_spreaded_curve, curve$pointer, as.numeric(spread), compounding, as.integer(frequency)))$value,
        "zero-spreaded", curve$index)
}

given <- function(pointer, kind, index) {
  structure(list(pointer = pointer, index = index, kind = kind, interpolation = NULL, instruments = NULL), class = "riemann_ois_curve")
}
