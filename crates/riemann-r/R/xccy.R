# Curves of a currency collateralised in another, from FX swaps and cross-currency basis swaps.
# Calculations are in Rust.

#' Quotes for a cross-currency curve
#'
#' A table of FX swaps and cross-currency basis swaps for [bootstrap_xccy_curve()].
#'
#' @param type `"fx_swap"` or `"basis_swap"`.
#' @param tenor Periods such as `"3M"` or `"5Y"`.
#' @param quote For FX swaps, the forward points: forward minus spot, in quote currency per unit of base
#'   currency. For basis swaps, the basis as a decimal.
#' @param fixing_days Business days from the evaluation date to the start.
#' @param payment_frequency Payments per year of the basis swaps' overnight legs: 1, 2, 4 or 12.
#' @param basis_on_base_leg `TRUE` when the basis is paid on the base currency's leg.
#' @param resetting `"none"` for constant notionals, `"base"` or `"quote"` for the leg whose notional resets
#'   to the FX forward at each period (mark-to-market).
#' @return A tibble, one row per quote.
#' @examples
#' create_xccy_instruments(c("fx_swap", "basis_swap", "basis_swap"), c("6M", "2Y", "5Y"), c(-3.2, -0.0042, -0.0050))
#' @export
create_xccy_instruments <- function(type, tenor, quote, fixing_days = 2L, payment_frequency = 4L, basis_on_base_leg = FALSE, resetting = "none") {
  tibble(type = as.character(type), tenor = as.character(tenor), quote = as.numeric(quote), fixing_days = as.integer(fixing_days),
         payment_frequency = as.integer(payment_frequency), basis_on_base_leg = as.logical(basis_on_base_leg), resetting = as.character(resetting))
}

#' Discount curve of a currency collateralised in another
#'
#' Bootstraps the discount curve of the currency that is not the collateral from FX swaps and
#' cross-currency basis swaps with overnight-indexed legs, log-linear in the discount factors, as
#' QuantLib's `FxSwapRateHelper`, `ConstNotionalCrossCurrencyBasisSwapRateHelper` and
#' `MtMCrossCurrencyBasisSwapRateHelper` with `PiecewiseLogLinearDiscount`.
#'
#' The pair is base/quote: the FX rate is units of the quote currency per unit of the base currency.
#' Each currency's overnight index forecasts from its curve (`base_curve`, `quote_curve`), and
#' `collateral_curve` discounts the collateral currency. For a foreign bond swapped into yen with yen
#' collateral, the result is the foreign currency's discount curve under yen collateral, for
#' [compute_yen_asset_swap_on_basis_curve()].
#'
#' @param evaluation_date The evaluation date, also the curve's reference date.
#' @param base_curve,quote_curve Curves of the two overnight indices, from [bootstrap_ois_curve()] or the
#'   functions of [create_flat_curve()].
#' @param collateral_curve The discount curve of the collateral currency; by default the curve of that
#'   currency's index.
#' @param base_is_collateral `TRUE` when the base currency is the collateral.
#' @param fx_spot Units of quote currency per unit of base currency, for the FX swaps.
#' @param instruments A table from [create_xccy_instruments()].
#' @param calendar `NULL` for the joint calendar of the two indices, or a calendar name.
#' @param convention,end_of_month Business-day convention and end-of-month rule of the instruments.
#' @param day_count Day count of the curve's times.
#' @param accuracy Accuracy of each search on the discount factors.
#' @return A curve, of class `riemann_ois_curve`, with `repricing`: the instruments with their pillar
#'   dates and the quotes the curve implies.
#' @examples
#' today <- "2026-11-04"
#' sofr <- create_flat_curve(today, 0.042, "actual_365_fixed", index = "sofr")
#' tonar <- create_flat_curve(today, 0.005, "actual_365_fixed", index = "tonar")
#' quotes <- create_xccy_instruments(c("fx_swap", "basis_swap", "basis_swap"), c("6M", "2Y", "5Y"), c(-3.2, -0.0042, -0.0050))
#' usd_in_jpy <- bootstrap_xccy_curve(today, sofr, tonar, base_is_collateral = FALSE, fx_spot = 150, instruments = quotes)
#' usd_in_jpy$repricing
#' @export
bootstrap_xccy_curve <- function(evaluation_date, base_curve, quote_curve, base_is_collateral, fx_spot, instruments, collateral_curve = NULL,
                                 calendar = NULL, convention = "modified_following", end_of_month = FALSE, day_count = "actual_365_fixed",
                                 accuracy = 1e-12) {
  purrr::walk(list(base_curve, quote_curve), \(curve) {
    if (!inherits(curve, "riemann_ois_curve")) stop("`base_curve` and `quote_curve` must be curves.", call. = FALSE)
  })
  if (is.null(collateral_curve)) collateral_curve <- if (as_flag(base_is_collateral)) base_curve else quote_curve
  x <- checked(.Call(wrap__bootstrap_xccy_curve, as_serial(evaluation_date), base_curve$pointer, quote_curve$pointer, collateral_curve$pointer,
                     as_flag(base_is_collateral), as.numeric(fx_spot), instruments$type, instruments$tenor, instruments$quote, instruments$fixing_days,
                     instruments$payment_frequency, as.integer(instruments$basis_on_base_leg), instruments$resetting,
                     if (is.null(calendar)) "" else calendar, convention, as_flag(end_of_month), day_count, as.numeric(accuracy)))
  index <- if (as_flag(base_is_collateral)) quote_curve$index else base_curve$index
  curve <- given(x$value, "cross-currency", index)
  curve$repricing <- tibble::add_column(instruments, pillar_date = as_date(x$pillar_date), implied_quote = x$implied_quote)
  curve
}

#' Yen-converted OIS asset swap on a basis term structure
#'
#' As [compute_yen_asset_swap()], with the FX forward implied by a basis term structure instead of one
#' constant spread: `basis_curve` discounts the foreign currency under yen collateral, from
#' [bootstrap_xccy_curve()], and the forward is `S [Pb(t)/Pb(t0)] / [Pj(t)/Pj(t0)]`. Notionals are constant;
#' there is no collateral choice and no credit, funding or cost adjustment.
#'
#' @inheritParams compute_yen_asset_swap
#' @param basis_curve The foreign currency's discount curve under yen collateral.
#' @return A tibble with the inputs and `native_asw_bp`, `yen_asw_bp`, `npv_yen`, `hedged_bond_pv_yen`,
#'   `yen_base_leg_pv`, `upfront_pv_yen`, `spread_annuity_yen` and `error`.
#' @export
compute_yen_asset_swap_on_basis_curve <- function(bond, clean_price, foreign_curve, yen_curve, basis_curve, fx_spot, evaluation_date, spread_bp = 0,
                                                  foreign_payment_frequency = 2L, yen_payment_frequency = 2L, foreign_payment_lag = 0L,
                                                  yen_payment_lag = 0L, on_error = "stop") {
  i <- tibble(clean_price = as.numeric(clean_price), fx_spot = as.numeric(fx_spot), spread_bp = as.numeric(spread_bp))
  rows <- finish_rows(.Call(wrap__compute_yen_asset_swap_on_basis_curve, bond$pointer, foreign_curve$pointer, yen_curve$pointer, basis_curve$pointer,
                            i$clean_price, i$fx_spot, i$spread_bp / 1e4, as.integer(foreign_payment_frequency), as.integer(yen_payment_frequency),
                            as.integer(foreign_payment_lag), as.integer(yen_payment_lag), as_serial(evaluation_date)), on_error)
  tibble(clean_price = i$clean_price, fx_spot = i$fx_spot, spread_bp = i$spread_bp, native_asw_bp = rows$native_fair_spread * 1e4,
         yen_asw_bp = rows$yen_fair_spread * 1e4, npv_yen = rows$npv, hedged_bond_pv_yen = rows$hedged_bond_pv, yen_base_leg_pv = rows$yen_base_leg_pv,
         upfront_pv_yen = rows$upfront_pv, spread_annuity_yen = rows$spread_annuity, error = rows$error)
}
