# Asset swaps and futures basis. Calculations are in Rust.

#' OIS asset swap
#'
#' A par-par asset swap of a fixed-rate bullet bond: pay the bond, receive the overnight leg plus a
#' spread, and exchange the difference between the dirty price and par at the start. The curve
#' forecasts and discounts, and its index is the overnight index.
#'
#' @param bond A bond from [fixed_rate_bond()], redeemed at 100.
#' @param clean_price Clean prices per 100; recycled with `spread_bp`.
#' @param curve A curve from [build_ois_curve()].
#' @param evaluation_date One date; the swap starts on the bond's settlement date.
#' @param payment_frequency Payments per year of the overnight leg.
#' @param payment_lag Business days from the end of a period to its payment.
#' @param spread_bp Contract spread on the overnight leg, in basis points.
#' @param on_error `"stop"` or `"na"`; see [RiemannFinMath-package].
#' @return A tibble with `clean_price`, `spread_bp`, `accrued`, `dirty_price`, `fair_spread_bp`,
#'   `spread_annuity`, `npv`, `bond_leg_pv`, `overnight_leg_pv`, `principal_pv`, `upfront_pv`,
#'   `fair_clean_price`, `effective_date`, `maturity_date` and `error`.
#' @examples
#' instruments <- ois_instruments(c("1Y", "5Y", "10Y"), c(0.006, 0.011, 0.015), 2L, 1L, 0L)
#' curve <- build_ois_curve("2026-10-06", "2026-10-06", "tonar", "actual_365_fixed", instruments)
#' bond <- fixed_rate_bond(
#'   settlement_days = 2L, face_amount = 100,
#'   effective_date = "2026-09-30", termination_date = "2033-09-30",
#'   tenor = "6M", calendar = "japan",
#'   convention = "unadjusted", termination_convention = "unadjusted", rule = "backward",
#'   end_of_month = FALSE, coupon_rate = 0.015, day_count = "actual_365_fixed",
#'   payment_convention = "following", redemption = 100
#' )
#' ois_asset_swap(bond, c(99, 102), curve, "2026-10-06")
#' asset_swap_quote_sensitivity(bond, 102, curve, "2026-10-06")
#' @export
ois_asset_swap <- function(bond, clean_price, curve, evaluation_date, payment_frequency = 2L, payment_lag = 0L, spread_bp = 0, on_error = "stop") {
  i <- tibble(clean_price = as.numeric(clean_price), spread_bp = as.numeric(spread_bp))
  rows <- finish_rows(.Call(wrap__ois_asset_swap, bond$pointer, curve$pointer, i$clean_price, i$spread_bp / 1e4, as.integer(payment_frequency),
                            as.integer(payment_lag), as_serial(evaluation_date)), on_error)
  tibble(clean_price = i$clean_price, spread_bp = i$spread_bp, accrued = rows$accrued, dirty_price = rows$dirty_price, fair_spread_bp = rows$fair_spread * 1e4,
         spread_annuity = rows$spread_annuity, npv = rows$npv, bond_leg_pv = rows$bond_leg_pv, overnight_leg_pv = rows$overnight_leg_pv,
         principal_pv = rows$principal_pv, upfront_pv = rows$upfront_pv, fair_clean_price = rows$fair_clean_price,
         effective_date = as_date(rows$effective_date), maturity_date = as_date(rows$maturity_date), error = rows$error)
}

#' @rdname ois_asset_swap
#' @return `asset_swap_quote_sensitivity()` returns a tibble with `tenor`, `rate`, `pillar_date` and
#'   `spread_bp_per_quote_bp`: the change of the fair spread in basis points for +1bp on the quote,
#'   with the curve following.
#' @export
asset_swap_quote_sensitivity <- function(bond, clean_price, curve, evaluation_date, payment_frequency = 2L, payment_lag = 0L) {
  rows <- checked(.Call(wrap__asset_swap_quote_sensitivity, bond$pointer, curve$pointer, as.numeric(clean_price), as.integer(payment_frequency),
                        as.integer(payment_lag), as_serial(evaluation_date)))
  tibble(tenor = curve$instruments$tenor[rows$instrument], rate = rows$rate, pillar_date = as_date(rows$pillar_date), spread_bp_per_quote_bp = rows$d_fair_spread)
}

#' Yen-converted OIS asset swap with one basis quote
#'
#' A foreign bond swapped into yen with a yen-collateralised, constant-notional cross-currency swap.
#' One constant spread on the foreign discounting is calibrated so that the maturity swap with the
#' given basis is at par. This is a simplified model: no collateral choice, no basis term structure,
#' no notional resets, and no credit, funding or cost adjustments.
#'
#' @inheritParams ois_asset_swap
#' @param bond A bond in the foreign currency, from [fixed_rate_bond()].
#' @param foreign_curve,yen_curve Curves from [build_ois_curve()].
#' @param fx_spot Yen per foreign unit for exchange on the effective date.
#' @param basis_bp Cross-currency basis on the yen leg, in basis points.
#' @param spread_bp Contract spread on the yen leg, in basis points.
#' @param foreign_payment_frequency,yen_payment_frequency Payments per year.
#' @param foreign_payment_lag,yen_payment_lag Payment lags in business days.
#' @return A tibble with the inputs and `native_asw_bp`, `yen_asw_bp`, `simple_sum_bp`, `npv_yen`,
#'   `hedged_bond_pv_yen`, `yen_base_leg_pv`, `upfront_pv_yen`, `spread_annuity_yen`,
#'   `calibrated_discount_spread_bp` and `error`.
#' @export
yen_asset_swap <- function(bond, clean_price, foreign_curve, yen_curve, fx_spot, basis_bp, evaluation_date, spread_bp = 0,
                           foreign_payment_frequency = 2L, yen_payment_frequency = 2L, foreign_payment_lag = 0L, yen_payment_lag = 0L, on_error = "stop") {
  i <- tibble(clean_price = as.numeric(clean_price), fx_spot = as.numeric(fx_spot), basis_bp = as.numeric(basis_bp), spread_bp = as.numeric(spread_bp))
  rows <- finish_rows(.Call(wrap__yen_asset_swap, bond$pointer, foreign_curve$pointer, yen_curve$pointer, i$clean_price, i$fx_spot, i$basis_bp / 1e4, i$spread_bp / 1e4,
                            as.integer(foreign_payment_frequency), as.integer(yen_payment_frequency), as.integer(foreign_payment_lag), as.integer(yen_payment_lag),
                            as_serial(evaluation_date)), on_error)
  tibble(clean_price = i$clean_price, fx_spot = i$fx_spot, basis_bp = i$basis_bp, spread_bp = i$spread_bp, native_asw_bp = rows$native_fair_spread * 1e4,
         yen_asw_bp = rows$yen_fair_spread * 1e4, simple_sum_bp = rows$simple_sum * 1e4, npv_yen = rows$npv, hedged_bond_pv_yen = rows$hedged_bond_pv,
         yen_base_leg_pv = rows$yen_base_leg_pv, upfront_pv_yen = rows$upfront_pv, spread_annuity_yen = rows$spread_annuity,
         calibrated_discount_spread_bp = rows$calibrated_discount_spread * 1e4, error = rows$error)
}

#' Basis of bonds against a bond future
#'
#' Gross basis, carry, net basis and implied repo. `future_basket()` applies this to the deliverable
#' bonds under the same inputs and marks the smallest net basis as the cheapest to deliver. The
#' conversion factors are inputs: they are set by the exchange.
#'
#' @param bond,bonds A bond, or a list of bonds, from [fixed_rate_bond()].
#' @param clean_price Clean prices per 100, one per bond.
#' @param futures_price The futures price.
#' @param conversion_factor Conversion factors, one per bond.
#' @param funding_rate Simple rate on Actual/365 from valuation to delivery.
#' @param valuation_date,delivery_date Dates.
#' @return A one-row tibble for `future_basis()`, with `accrued_start`, `accrued_end`,
#'   `coupon_income`, `futures_invoice`, `gross_basis`, `funding_cost`, `carry`, `net_basis` and
#'   `implied_repo`. `future_basket()` has one row per bond, with `clean_price`, `conversion_factor`
#'   and the logical `ctd`.
#' @export
future_basis <- function(bond, clean_price, futures_price, conversion_factor, funding_rate, valuation_date, delivery_date) {
  tibble::as_tibble(checked(.Call(wrap__future_basis, bond$pointer, as.numeric(clean_price), as.numeric(futures_price), as.numeric(conversion_factor),
                                  as.numeric(funding_rate), as_serial(valuation_date), as_serial(delivery_date))))
}

#' @rdname future_basis
#' @export
future_basket <- function(bonds, clean_price, futures_price, conversion_factor, funding_rate, valuation_date, delivery_date) {
  if (length(bonds) != length(clean_price) || length(bonds) != length(conversion_factor)) {
    stop("`bonds`, `clean_price` and `conversion_factor` must have the same length.", call. = FALSE)
  }
  rows <- Map(function(bond, price, factor) {
    future_basis(bond, price, futures_price, factor, funding_rate, valuation_date, delivery_date)
  }, bonds, clean_price, conversion_factor)
  columns <- lapply(names(rows[[1]]), function(name) vapply(rows, function(r) r[[name]], 0))
  names(columns) <- names(rows[[1]])
  out <- tibble::as_tibble(c(list(clean_price = as.numeric(clean_price), conversion_factor = as.numeric(conversion_factor)), columns))
  out$ctd <- out$net_basis == min(out$net_basis)
  out
}
