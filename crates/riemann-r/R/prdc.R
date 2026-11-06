# Power-reverse dual-currency coupons. Calculations are in Rust.

#' PRDC note
#'
#' A note paying `clamp(leverage * S / fx_base - subtraction, floor, cap)` on its notional, with the
#' model of LagrangeFinance II-9 and III-7: a lognormal FX rate with one volatility, on deterministic
#' domestic and foreign curves. Rates are not stochastic, and there is no call, target redemption or
#' knock-out. This is a model value, not a market price.
#'
#' With `paths = 0` the value is the closed form, each coupon being a floor plus a call spread on the
#' FX rate. With paths it is a Monte Carlo on the coupon dates, whose result depends on the seed and
#' not on the threads. Both give derivatives by automatic differentiation, in place of `torch`.
#'
#' @param spot Domestic per foreign.
#' @param volatility Lognormal FX volatility.
#' @param domestic_curve,foreign_curve Curves from [bootstrap_ois_curve()], on the same date.
#' @param coupon_times Coupon dates in years from the curves' reference date.
#' @param accrual_fractions Year fractions of the coupons.
#' @param fx_base,leverage,subtraction,floor,cap The coupon's terms. `cap` may be `Inf`.
#' @param notional,redemption The notional, and the amount paid on the last coupon date.
#' @param fixed_rate Fixed coupon rates, with `NA` where the formula applies.
#' @param paths Monte Carlo paths, or 0 for the closed form.
#' @param seed A whole number for the random numbers.
#' @param antithetic `TRUE` or `FALSE`. `NA` is an error.
#' @param threads Worker threads; 0 means one per logical processor.
#' @return A list with `summary`, a one-row tibble with `npv`, `standard_error` (0 for the closed form),
#'   `fx_delta` and `fx_vega`, and `curve_sensitivity`, a tibble with `curve`, `date`, `time`, `d_npv`
#'   per unit of log discount factor, and `npv_per_bp` for +1bp on the node's continuous zero rate.
#' @examples
#' tenors <- c("1Y", "3Y", "5Y", "7Y")
#' jpy_quotes <- create_ois_instruments(tenors, c(0.01, 0.012, 0.014, 0.016), 2L, 1L, 0L)
#' usd_quotes <- create_ois_instruments(tenors, c(0.04, 0.038, 0.036, 0.034), 2L, 1L, 0L)
#' jpy <- bootstrap_ois_curve("2026-11-02", "2026-11-02", "tonar", "actual_365_fixed", jpy_quotes)
#' usd <- bootstrap_ois_curve("2026-11-02", "2026-11-02", "sofr", "actual_365_fixed", usd_quotes)
#' compute_prdc_value(150, 0.12, jpy, usd, 1:5, rep(1, 5), 150, 0.8, 0.78, 0, 0.08, notional = 1e8)$summary
#' @export
compute_prdc_value <- function(spot, volatility, domestic_curve, foreign_curve, coupon_times, accrual_fractions, fx_base, leverage, subtraction, floor = 0, cap = Inf,
                       notional = 1, redemption = notional, fixed_rate = NA_real_, paths = 0, seed = 1, antithetic = TRUE, threads = 0L) {
  i <- tibble(coupon_times = as.numeric(coupon_times), accrual_fractions = as.numeric(accrual_fractions), fixed_rate = as.numeric(fixed_rate))
  x <- checked(.Call(wrap__compute_prdc_value, as.numeric(spot), as.numeric(volatility), domestic_curve$pointer, foreign_curve$pointer, i$coupon_times, i$accrual_fractions, i$fixed_rate,
                     as.numeric(fx_base), as.numeric(leverage), as.numeric(subtraction), as.numeric(floor), as.numeric(cap), as.numeric(notional), as.numeric(redemption),
                     as.numeric(paths), as.numeric(seed), as_flag(antithetic), as.integer(threads)))
  list(
    summary = tibble(npv = x$npv, standard_error = x$standard_error, fx_delta = x$fx_delta, fx_vega = x$fx_vega),
    curve_sensitivity = tibble(curve = x$curve, date = as_date(x$date), time = x$time, d_npv = x$d_npv, npv_per_bp = -x$d_npv * x$time * 1e-4)
  )
}

#' PRDC note with stochastic rates
#'
#' The note of [compute_prdc_value()] in a three-factor model under the domestic risk-neutral measure: Hull-White
#' domestic and foreign short rates fitting their curves, and a lognormal FX rate,
#' `dS / S = (r_d - r_f) dt + fx_volatility dW_X`, the foreign rate carrying the quanto drift
#' `-rho_fX sigma_f fx_volatility`. Each coupon depends only on the FX rate on its date, and under the domestic
#' forward measure of that date the FX forward is lognormal with the variance of
#' `fx_volatility dW_X - sigma_f B_f dW_f + sigma_d B_d dW_d`, so the closed form is the call spread of
#' [compute_prdc_value()] with each coupon's `effective_volatility`. With `paths > 0` the value is a Monte Carlo of
#' the three factors (exact Ornstein-Uhlenbeck steps of at most `1 / steps_per_year` years, trapezoid integrals of the
#' rates), next to the closed form. No call, target redemption or knock-out; no sensitivities.
#'
#' @inheritParams compute_prdc_value
#' @param fx_volatility Lognormal FX volatility.
#' @param domestic_rate,foreign_rate Hull-White `c(mean_reversion, volatility)` of each short rate.
#' @param correlation `c(domestic-foreign, domestic-FX, foreign-FX)` correlations of the Brownian motions.
#' @param paths Monte Carlo paths, or 0 for the closed form only.
#' @param steps_per_year Steps a year of the Monte Carlo.
#' @return A list with `summary`, a one-row tibble with `value`, `standard_error` (0 for the closed form) and
#'   `analytic`, and `coupons`, a tibble with each coupon's `time` and `effective_volatility`.
#' @examples
#' tenors <- c("1Y", "3Y", "5Y", "7Y")
#' jpy_quotes <- create_ois_instruments(tenors, c(0.01, 0.012, 0.014, 0.016), 2L, 1L, 0L)
#' usd_quotes <- create_ois_instruments(tenors, c(0.04, 0.038, 0.036, 0.034), 2L, 1L, 0L)
#' jpy <- bootstrap_ois_curve("2026-11-02", "2026-11-02", "tonar", "actual_365_fixed", jpy_quotes)
#' usd <- bootstrap_ois_curve("2026-11-02", "2026-11-02", "sofr", "actual_365_fixed", usd_quotes)
#' compute_prdc_three_factor_value(150, 0.11, jpy, usd, c(0.03, 0.01), c(0.05, 0.012), c(0.3, -0.2, 0.4),
#'                                 1:5, rep(1, 5), 150, 0.8, 0.78, 0, 0.08, notional = 1e8)$summary
#' @export
compute_prdc_three_factor_value <- function(spot, fx_volatility, domestic_curve, foreign_curve, domestic_rate, foreign_rate,
                                            correlation = c(0, 0, 0), coupon_times, accrual_fractions, fx_base, leverage, subtraction,
                                            floor = 0, cap = Inf, notional = 1, redemption = notional, fixed_rate = NA_real_,
                                            paths = 0L, steps_per_year = 12L, seed = 1) {
  i <- tibble(coupon_times = as.numeric(coupon_times), accrual_fractions = as.numeric(accrual_fractions), fixed_rate = as.numeric(fixed_rate))
  x <- checked(.Call(wrap__compute_prdc_three_factor_value, as.numeric(spot), as.numeric(fx_volatility), domestic_curve$pointer,
                     foreign_curve$pointer, as.numeric(domestic_rate), as.numeric(foreign_rate), as.numeric(correlation),
                     i$coupon_times, i$accrual_fractions, i$fixed_rate, as.numeric(fx_base), as.numeric(leverage), as.numeric(subtraction),
                     as.numeric(floor), as.numeric(cap), as.numeric(notional), as.numeric(redemption), as.integer(paths),
                     as.integer(steps_per_year), as.numeric(seed)))
  list(summary = tibble(value = x$value, standard_error = x$standard_error, analytic = x$analytic),
       coupons = tibble(time = i$coupon_times, effective_volatility = x$effective_volatility))
}
