# FX smiles quoted by delta and PRDC notes priced on them (docs/spec_prdc.md chapter 14).

fx_delta_types <- c("spot", "forward", "premium_adjusted_spot", "premium_adjusted_forward")
fx_atm_types <- c("spot", "forward", "delta_neutral", "put_call_50")

#' Strikes and deltas of FX options
#'
#' The strike of a delta, the delta of a strike, and the ATM strike of an option on an FX rate (domestic per
#' foreign), as QuantLib's `BlackDeltaCalculator`. The spot delta is `phi P_f N(phi d1)` and the forward delta
#' `phi N(phi d1)`; the premium-adjusted ones take the premium in foreign currency out, `phi P_f N(phi d2) K / F`
#' and `phi N(phi d2) K / F`, and are solved for the strike by Brent's method to 1e-10. The delta-neutral ATM
#' strike is `F exp(v / 2)`, or `F exp(-v / 2)` with a premium-adjusted delta (`v` the variance); `"put_call_50"`
#' needs the forward delta.
#'
#' @param delta Deltas: positive for calls, negative for puts.
#' @param strike Strikes.
#' @param option `"call"` or `"put"`.
#' @param delta_type `"spot"`, `"forward"`, `"premium_adjusted_spot"` or `"premium_adjusted_forward"`.
#' @param spot FX spot, domestic per foreign.
#' @param domestic_discount,foreign_discount Discount factors to the delivery.
#' @param std_dev Standard deviation `sigma sqrt(T)`.
#' @param atm_type `"spot"`, `"forward"`, `"delta_neutral"` or `"put_call_50"`.
#' @return A numeric vector.
#' @examples
#' compute_fx_strike_from_delta(c(-0.25, 0.25), c("put", "call"), "spot", 150, 0.99, 0.96, 0.12)
#' compute_fx_atm_strike("delta_neutral", "premium_adjusted_spot", 150, 0.99, 0.96, 0.12)
#' @export
compute_fx_strike_from_delta <- function(delta, option, delta_type, spot, domestic_discount, foreign_discount, std_dev) {
  convert_fx_delta("strike_from_delta", delta, option, delta_type, spot, domestic_discount, foreign_discount, std_dev, "delta_neutral")
}

#' @rdname compute_fx_strike_from_delta
#' @export
compute_fx_delta_from_strike <- function(strike, option, delta_type, spot, domestic_discount, foreign_discount, std_dev) {
  convert_fx_delta("delta_from_strike", strike, option, delta_type, spot, domestic_discount, foreign_discount, std_dev, "delta_neutral")
}

#' @rdname compute_fx_strike_from_delta
#' @export
compute_fx_atm_strike <- function(atm_type, delta_type, spot, domestic_discount, foreign_discount, std_dev) {
  convert_fx_delta("atm_strike", 0, "put", delta_type, spot, domestic_discount, foreign_discount, std_dev, atm_type)
}

convert_fx_delta <- function(kind, input, option, delta_type, spot, domestic_discount, foreign_discount, std_dev, atm_type) {
  rows <- tibble(input = as.numeric(input), option = option, delta_type = delta_type, spot = as.numeric(spot),
                 domestic_discount = as.numeric(domestic_discount), foreign_discount = as.numeric(foreign_discount),
                 std_dev = as.numeric(std_dev), atm_type = atm_type)
  purrr::pmap_dbl(rows, \(input, option, delta_type, spot, domestic_discount, foreign_discount, std_dev, atm_type) {
    checked(.Call(wrap__compute_fx_delta_conversion, kind, match.arg(option, c("call", "put")), match.arg(delta_type, fx_delta_types),
                  spot, domestic_discount, foreign_discount, std_dev, input, match.arg(atm_type, fx_atm_types)))$value
  })
}

#' FX volatility surface quoted by delta
#'
#' A surface of Black volatilities from volatilities at put and call deltas and ATM by expiry, as QuantLib's
#' `BlackVolatilitySurfaceDelta` (with the volatility flat in time after the last expiry). For a time `t`, each
#' delta's (and ATM's) variance `sigma^2 T` is interpolated linearly in time from 0 at time 0, each delta is turned
#' into its strike with its own volatility and the curves' discount factors to `t`, and the smile is interpolated in
#' strike. From `switch_time` on the long-term conventions apply (for example spot deltas up to a year and forward
#' deltas after).
#'
#' @param spot FX spot, domestic per foreign.
#' @param domestic_curve,foreign_curve Curves of the two currencies.
#' @param expiries Expiries in years, increasing.
#' @param put_deltas Put deltas (negative), for example `c(-0.1, -0.25)`.
#' @param call_deltas Call deltas, for example `c(0.25, 0.1)`.
#' @param volatilities A matrix with one row per expiry and one column per pillar: the put deltas, ATM (with
#'   `has_atm`), then the call deltas.
#' @param has_atm Whether the volatilities include ATM.
#' @param delta_type,atm_type,atm_delta_type The conventions of the deltas (see [compute_fx_strike_from_delta()]);
#'   `atm_delta_type` is that of the ATM strike.
#' @param interpolation `"linear"` or `"cubic_spline"` (natural) in strike.
#' @param flat_strike_extrapolation `TRUE` for the volatility of the lowest or highest strike beyond them.
#' @param switch_time Time from which `long_delta_type`, `long_atm_type` and `long_atm_delta_type` apply; `Inf`
#'   for none.
#' @param long_delta_type,long_atm_type,long_atm_delta_type The long-term conventions.
#' @return A list of class `riemann_fx_smile` for [compute_fx_smile_volatility()] and [compute_prdc_smile_value()].
#' @examples
#' jpy <- create_flat_curve("2026-11-02", 0.012, "actual_365_fixed", "continuous")
#' usd <- create_flat_curve("2026-11-02", 0.035, "actual_365_fixed", "continuous")
#' vols <- rbind(c(0.15, 0.13, 0.115, 0.118, 0.135), c(0.156, 0.134, 0.118, 0.121, 0.14))
#' smile <- create_fx_smile(150, jpy, usd, c(1, 3), c(-0.1, -0.25), c(0.25, 0.1), vols)
#' compute_fx_smile_volatility(smile, 2, c(130, 150, 170))
#' @export
create_fx_smile <- function(spot, domestic_curve, foreign_curve, expiries, put_deltas, call_deltas, volatilities, has_atm = TRUE,
                            delta_type = "spot", atm_type = "delta_neutral", atm_delta_type = delta_type, interpolation = "linear",
                            flat_strike_extrapolation = FALSE, switch_time = Inf, long_delta_type = "forward",
                            long_atm_type = "delta_neutral", long_atm_delta_type = long_delta_type) {
  volatilities <- as.matrix(volatilities)
  smile <- structure(list(
    spot = as.numeric(spot), domestic_curve = domestic_curve, foreign_curve = foreign_curve, expiries = as.numeric(expiries),
    put_deltas = as.numeric(put_deltas), call_deltas = as.numeric(call_deltas), has_atm = as_flag(has_atm),
    volatilities = volatilities,
    conventions = c(match.arg(delta_type, fx_delta_types), match.arg(atm_type, fx_atm_types), match.arg(atm_delta_type, fx_delta_types),
                    match.arg(long_delta_type, fx_delta_types), match.arg(long_atm_type, fx_atm_types),
                    match.arg(long_atm_delta_type, fx_delta_types)),
    switch_time = as.numeric(switch_time), interpolation = match.arg(interpolation, c("linear", "cubic_spline")),
    flat_strike_extrapolation = as_flag(flat_strike_extrapolation)
  ), class = "riemann_fx_smile")
  # checked once here, so that a bad surface fails where it is made
  fx_smile_call(smile, wrap__compute_fx_smile_values, numeric(), numeric(), smile$expiries[1])
  smile
}

fx_smile_call <- function(smile, routine, ...) {
  if (!inherits(smile, "riemann_fx_smile")) stop("`smile` must come from create_fx_smile().", call. = FALSE)
  checked(.Call(routine, smile$spot, smile$domestic_curve$pointer, smile$foreign_curve$pointer, smile$expiries, smile$put_deltas,
                smile$call_deltas, smile$has_atm, as.numeric(t(smile$volatilities)), smile$conventions, smile$switch_time,
                smile$interpolation, smile$flat_strike_extrapolation, ...))
}

#' Volatilities and strikes of an FX smile
#'
#' @param smile A smile of [create_fx_smile()].
#' @param time Times in years (recycled with `strike`).
#' @param strike Strikes.
#' @return `compute_fx_smile_volatility()`: the Black volatilities. `compute_fx_smile_strikes()`: a tibble of the
#'   smile's strikes at `time` (one time) and their volatilities, in increasing strike.
#' @export
compute_fx_smile_volatility <- function(smile, time, strike) {
  grid <- tibble(time = as.numeric(time), strike = as.numeric(strike))
  fx_smile_call(smile, wrap__compute_fx_smile_values, grid$time, grid$strike, Inf)$volatility
}

#' @rdname compute_fx_smile_volatility
#' @export
compute_fx_smile_strikes <- function(smile, time) {
  x <- fx_smile_call(smile, wrap__compute_fx_smile_values, numeric(), numeric(), as.numeric(time))
  tibble(strike = x$smile_strike, volatility = x$smile_volatility)
}

#' PRDC note on an FX smile
#'
#' The note of [compute_prdc_value()] priced by static replication on an FX smile: each coupon
#' `clamp(leverage * S / fx_base - subtraction, floor, cap)` is its floor plus `leverage / fx_base` times the call
#' spread between the FX levels of the floor and the cap, so it needs only those two calls, which the smile prices
#' by Black's formula at its volatilities there (forwards and discount factors from the smile's curves). This holds
#' for any model of the FX rate that reproduces the smile; calls, target redemptions and knock-outs depend on the
#' dynamics and are not valued here. The derivatives are central differences: the spot by 1e-5 relative, with the
#' surface moving with it (`fx_delta`, sticky delta) and without (`fx_delta_sticky_strike`), and each volatility of
#' the surface by 1e-5.
#'
#' @inheritParams compute_prdc_value
#' @param smile A smile of [create_fx_smile()].
#' @return A list with `summary` (`value`, `fx_delta`, `fx_delta_sticky_strike`, `fx_vega`), `coupons` (each
#'   coupon's `time`, `forward`, the strikes and volatilities of its floor and cap, its expected `rate` and its
#'   `value`) and `pillar_vega` (the derivative with respect to each volatility of the surface: `expiry`, `pillar`,
#'   `d_npv`).
#' @examples
#' jpy <- create_flat_curve("2026-11-02", 0.012, "actual_365_fixed", "continuous")
#' usd <- create_flat_curve("2026-11-02", 0.035, "actual_365_fixed", "continuous")
#' vols <- rbind(c(0.15, 0.13, 0.115, 0.118, 0.135), c(0.156, 0.134, 0.118, 0.121, 0.14))
#' smile <- create_fx_smile(150, jpy, usd, c(1, 3), c(-0.1, -0.25), c(0.25, 0.1), vols)
#' compute_prdc_smile_value(smile, 1:3, rep(1, 3), 150, 0.8, 0.78, 0, 0.08)$summary
#' @export
compute_prdc_smile_value <- function(smile, coupon_times, accrual_fractions, fx_base, leverage, subtraction, floor = 0, cap = Inf,
                                     notional = 1, redemption = notional, fixed_rate = NA_real_) {
  i <- tibble(coupon_times = as.numeric(coupon_times), accrual_fractions = as.numeric(accrual_fractions), fixed_rate = as.numeric(fixed_rate))
  x <- fx_smile_call(smile, wrap__compute_prdc_smile_value, i$coupon_times, i$accrual_fractions, i$fixed_rate, as.numeric(fx_base),
                     as.numeric(leverage), as.numeric(subtraction), as.numeric(floor), as.numeric(cap), as.numeric(notional),
                     as.numeric(redemption))
  pillars <- c(paste0("put_", abs(smile$put_deltas)), if (smile$has_atm) "atm", paste0("call_", smile$call_deltas))
  list(
    summary = tibble(value = x$value, fx_delta = x$fx_delta, fx_delta_sticky_strike = x$fx_delta_sticky_strike, fx_vega = x$fx_vega),
    coupons = tibble(time = x$time, forward = x$forward, floor_strike = x$floor_strike, floor_volatility = x$floor_volatility,
                     cap_strike = x$cap_strike, cap_volatility = x$cap_volatility, rate = x$rate, value = x$coupon_value),
    pillar_vega = tibble(expiry = rep(smile$expiries, each = length(pillars)), pillar = rep(pillars, times = length(smile$expiries)),
                         d_npv = x$pillar_vega)
  )
}
