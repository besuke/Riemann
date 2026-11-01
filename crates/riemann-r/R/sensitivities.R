# Sensitivities by automatic differentiation. The calculations are in Rust.

#' Yield sensitivity of a bond
#'
#' The dirty price and its first and second derivatives with respect to the yield, by automatic
#' differentiation. For compounded and continuous yields they equal `-duration * price` and
#' `convexity * price` with the modified duration. For simple yields, the price compounds period by
#' period, so its derivatives differ from QuantLib's duration formula, which uses the accumulated
#' time.
#'
#' @inheritParams bond_values
#' @return A tibble with the columns `rate`, `settlement_date`, `dirty_price`, `d_price_d_yield`,
#'   `d2_price_d_yield2` and `error`.
#' @examples
#' bond <- fixed_rate_bond(
#'   settlement_days = 1L, face_amount = 100,
#'   effective_date = "2020-03-20", termination_date = "2030-03-20",
#'   tenor = "6M", calendar = "japan",
#'   convention = "following", termination_convention = "following", rule = "backward",
#'   end_of_month = FALSE, coupon_rate = 0.015, day_count = "act_act_isma",
#'   payment_convention = "following", redemption = 100
#' )
#' yield_sensitivity(bond, 0.02, "act_act_isma", "compounded", 2L, "2026-11-04")
#' @export
yield_sensitivity <- function(bond, rate, day_count, compounding, frequency, settlement_date, on_error = "stop") {
  inputs <- tibble(rate = as.numeric(rate), settlement_date = as_serial(settlement_date))
  rows <- finish_rows(checked(.Call(
    wrap__yield_sensitivity, bond$pointer, inputs$rate, day_count, compounding, as.integer(frequency), inputs$settlement_date
  )), on_error)
  tibble(rate = inputs$rate, settlement_date = as_date(inputs$settlement_date), dirty_price = rows$dirty_price,
         d_price_d_yield = rows$d_price_d_yield, d2_price_d_yield2 = rows$d2_price_d_yield2, error = rows$error)
}

#' Curve sensitivities of a bond
#'
#' Bucket sensitivities by automatic differentiation.
#'
#' `curve_node_sensitivity()` moves one node of the curve at a time, with the interpolation between
#' nodes following. `quote_sensitivity()` moves one input quote at a time, and the curve follows as
#' if it were rebuilt: the derivatives of the nodes with respect to the quotes come from the solved
#' curve, not from differentiating the root finder.
#'
#' A curve that misses its quotes (see [quote_repricing()]) has unreliable derivatives with respect
#' to those quotes.
#'
#' @param bond A bond from [fixed_rate_bond()].
#' @param curve A curve from [build_ois_curve()].
#' @param evaluation_date One date.
#' @return A tibble. `npv` is in currency at the curve's reference date, and `dirty_price` is per
#'   100 at settlement; both repeat the value of the bond on the curve. `npv_per_bp` and
#'   `dirty_price_per_bp` are first-order changes for +1bp, on the node's continuous zero rate or on
#'   the quote. `curve_node_sensitivity()` has the columns `date`, `time`, `npv`, `dirty_price`,
#'   `npv_per_bp` and `dirty_price_per_bp`. `quote_sensitivity()` has `tenor`, `rate`, `pillar_date`
#'   and the same value columns.
#' @examples
#' instruments <- ois_instruments(
#'   c("ON", "1Y", "5Y", "10Y"), c(0.002, 0.004, 0.009, 0.014), 2L, 1L, 0L
#' )
#' curve <- build_ois_curve("2026-11-02", "2026-11-02", "tonar", "actual_365_fixed", instruments)
#' bond <- fixed_rate_bond(
#'   settlement_days = 1L, face_amount = 100,
#'   effective_date = "2025-03-20", termination_date = "2033-03-20",
#'   tenor = "6M", calendar = "japan",
#'   convention = "following", termination_convention = "following", rule = "backward",
#'   end_of_month = FALSE, coupon_rate = 0.015, day_count = "actual_365_fixed",
#'   payment_convention = "following", redemption = 100
#' )
#' curve_node_sensitivity(bond, curve, "2026-11-02")
#' quote_sensitivity(bond, curve, "2026-11-02")
#' @export
curve_node_sensitivity <- function(bond, curve, evaluation_date) {
  rows <- checked(.Call(wrap__curve_node_sensitivity, bond$pointer, curve$pointer, as_serial(evaluation_date)))
  tibble(date = as_date(rows$date), time = rows$time, npv = rows$npv, dirty_price = rows$dirty_price,
         npv_per_bp = -rows$d_npv * rows$time * 1e-4, dirty_price_per_bp = -rows$d_dirty_price * rows$time * 1e-4)
}

#' @rdname curve_node_sensitivity
#' @export
quote_sensitivity <- function(bond, curve, evaluation_date) {
  rows <- checked(.Call(wrap__quote_sensitivity, bond$pointer, curve$pointer, as_serial(evaluation_date)))
  tibble(tenor = curve$instruments$tenor[rows$instrument], rate = rows$rate, pillar_date = as_date(rows$pillar_date),
         npv = rows$npv, dirty_price = rows$dirty_price, npv_per_bp = rows$d_npv * 1e-4,
         dirty_price_per_bp = rows$d_dirty_price * 1e-4)
}
