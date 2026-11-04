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
#'   compounding (QuantLib `ZeroSpreadedTermStructure`). With `dates`, the spreads are given on those
#'   dates, linear in time between them and flat outside (QuantLib `PiecewiseZeroSpreadedTermStructure`);
#'   the curve then ends at the earlier of the base curve's last date and the last spread date.
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
#' @param spread Spread added to the zero rates: one number, or one per date of `spread_dates`.
#' @param spread_dates `NULL`, or increasing dates for a spread that changes with time.
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
create_zero_spreaded_curve <- function(curve, spread, compounding = "continuous", frequency = 1L, spread_dates = NULL) {
  if (!inherits(curve, "riemann_ois_curve")) stop("`curve` must be a curve.", call. = FALSE)
  dates <- if (is.null(spread_dates)) integer() else as_serial(spread_dates)
  given(checked(.Call(wrap__create_zero_spreaded_curve, curve$pointer, as.numeric(spread), compounding, as.integer(frequency), dates))$value,
        "zero-spreaded", curve$index)
}

#' Curve bootstrapped from bond prices
#'
#' Fits a curve to the prices of fixed-rate bonds, one node at the last payment date of each bond, from
#' the shortest (QuantLib `FixedRateBondHelper` and `PiecewiseYieldCurve`). Each bond's settlement
#' date is counted from `evaluation_date` with the bond's own settlement days and calendar, and the
#' curve prices it as QuantLib's `DiscountingBondEngine` does.
#'
#' @param evaluation_date The trade date (QuantLib's evaluation date).
#' @param bonds A list of bonds from [create_fixed_rate_bond()].
#' @param prices Prices per 100 of notional, one per bond.
#' @param price_type `"clean"` or `"dirty"`.
#' @param day_count Day count of the curve's times.
#' @param interpolation `"linear_zero"`: continuous zero rates linear in time (QuantLib
#'   `PiecewiseLinearZero`); or `"log_linear_discount"`: discount factors log-linear in time (QuantLib
#'   `PiecewiseYieldCurve<Discount, LogLinear>`).
#' @param reference_date The curve's reference date; by default `evaluation_date`.
#' @param accuracy Accuracy of each search, on zero rates or discount factors.
#' @param index As in [create_flat_curve()]; it does not matter for discounting.
#' @return A curve, of class `riemann_ois_curve`, with `repricing`: a tibble of each bond's price, pillar
#'   date and price on the curve. Sensitivities to the bond prices are not available.
#' @examples
#' bond <- function(years, coupon) create_fixed_rate_bond(1L, 100, "2022-03-17", as.Date("2022-03-17") + 365.25 * years,
#'   "6M", "us_government_bond", "unadjusted", "unadjusted", "backward", FALSE, coupon, "thirty_360_bond_basis", "following", 100)
#' curve <- bootstrap_bond_curve("2022-03-17", list(bond(1, 0.013), bond(2, 0.0194), bond(5, 0.0217)), c(100, 100, 100))
#' curve$repricing
#' @export
bootstrap_bond_curve <- function(evaluation_date, bonds, prices, price_type = "clean", day_count = "actual_360",
                                 interpolation = "linear_zero", reference_date = evaluation_date, accuracy = 1e-12, index = "tonar") {
  if (!is.list(bonds) || !all(purrr::map_lgl(bonds, \(b) inherits(b, "riemann_bond")))) {
    stop("`bonds` must be a list of bonds from create_fixed_rate_bond().", call. = FALSE)
  }
  x <- checked(.Call(wrap__bootstrap_bond_curve, as_serial(evaluation_date), as_serial(reference_date), purrr::map(bonds, "pointer"),
                     as.numeric(prices), price_type, day_count, interpolation, as.numeric(accuracy), index))
  curve <- given(x$value, paste("bond-bootstrapped", interpolation), index)
  curve$repricing <- tibble(price = as.numeric(prices), pillar_date = as_date(x$pillar_date), implied_price = x$implied_price)
  curve
}

given <- function(pointer, kind, index) {
  structure(list(pointer = pointer, index = index, kind = kind, interpolation = NULL, instruments = NULL), class = "riemann_ois_curve")
}
