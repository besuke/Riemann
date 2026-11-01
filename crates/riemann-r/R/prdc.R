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
#' @param domestic_curve,foreign_curve Curves from [build_ois_curve()], on the same date.
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
#' jpy_quotes <- ois_instruments(tenors, c(0.01, 0.012, 0.014, 0.016), 2L, 1L, 0L)
#' usd_quotes <- ois_instruments(tenors, c(0.04, 0.038, 0.036, 0.034), 2L, 1L, 0L)
#' jpy <- build_ois_curve("2026-11-02", "2026-11-02", "tonar", "actual_365_fixed", jpy_quotes)
#' usd <- build_ois_curve("2026-11-02", "2026-11-02", "sofr", "actual_365_fixed", usd_quotes)
#' prdc_value(150, 0.12, jpy, usd, 1:5, rep(1, 5), 150, 0.8, 0.78, 0, 0.08, notional = 1e8)$summary
#' @export
prdc_value <- function(spot, volatility, domestic_curve, foreign_curve, coupon_times, accrual_fractions, fx_base, leverage, subtraction, floor = 0, cap = Inf,
                       notional = 1, redemption = notional, fixed_rate = NA_real_, paths = 0, seed = 1, antithetic = TRUE, threads = 0L) {
  i <- tibble(coupon_times = as.numeric(coupon_times), accrual_fractions = as.numeric(accrual_fractions), fixed_rate = as.numeric(fixed_rate))
  x <- checked(.Call(wrap__prdc_value, as.numeric(spot), as.numeric(volatility), domestic_curve$pointer, foreign_curve$pointer, i$coupon_times, i$accrual_fractions, i$fixed_rate,
                     as.numeric(fx_base), as.numeric(leverage), as.numeric(subtraction), as.numeric(floor), as.numeric(cap), as.numeric(notional), as.numeric(redemption),
                     as.numeric(paths), as.numeric(seed), as_flag(antithetic), as.integer(threads)))
  list(
    summary = tibble(npv = x$npv, standard_error = x$standard_error, fx_delta = x$fx_delta, fx_vega = x$fx_vega),
    curve_sensitivity = tibble(curve = x$curve, date = as_date(x$date), time = x$time, d_npv = x$d_npv, npv_per_bp = -x$d_npv * x$time * 1e-4)
  )
}
