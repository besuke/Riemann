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
#' rates), next to the closed form. The closed form's first derivatives with respect to every input come from
#' automatic differentiation in one pass.
#'
#' With a finite `target`, the note is a target redemption note: once the sum of the coupon rates times their accrual
#' fractions (fixed coupons included) reaches `target`, it pays that date's coupon (`target_coupon = "capped"` only up
#' to the target, `"full"` the whole coupon) and the redemption, and ends. With `make_whole`, a note that has not
#' reached the target by the last date also pays the shortfall then. Its value is a Monte Carlo on the three-factor
#' paths (no regression), next to the note without the target on the same paths.
#'
#' With a finite `knock_out_level`, the note also ends on the coupon dates `knock_out_dates` (all but the last by
#' default) where the FX rate is at or above the level, paying that date's coupon and the redemption. A date that
#' reaches the target and the level counts as the target's. No call with a target or knock-out, and no
#' derivatives of these notes (their cash flows jump where they end).
#'
#' With `call_dates`, the issuer may redeem the note at `call_price` on those coupon dates, right after paying their
#' coupons, and calls when the rest of the note is worth more than the call price. The rule comes from Longstaff and
#' Schwartz on paths from `training_seed` (the rest's value at the date regressed on 1, `S / fx_base`, its square and
#' the two rate factors), and the value from paths from `seed`. A suboptimal rule overstates the note, so the
#' estimate leans upward. The result adds the non-callable note and the hindsight value (each path called where it
#' is cheapest) on the same paths, and each call date's probability.
#'
#' The callable note's derivatives hold the issuer's rule fixed (the call is optimal, so a change of the rule moves
#' the value only to second order, up to the rule's error, which more `training_paths` reduce): pathwise for the spot
#' and the curves' nodes, and central differences of `bump` on the same paths for the other inputs (`10 * bump` for
#' the correlations, one-sided at the edge of a range). Each comes with its standard error.
#'
#' @inheritParams compute_prdc_value
#' @param fx_volatility Lognormal FX volatility.
#' @param domestic_rate,foreign_rate Hull-White `c(mean_reversion, volatility)` of each short rate.
#' @param correlation `c(domestic-foreign, domestic-FX, foreign-FX)` correlations of the Brownian motions.
#' @param paths Monte Carlo paths, or 0 for the closed form only.
#' @param steps_per_year Steps a year of the Monte Carlo.
#' @param call_dates Positions in `coupon_times` of the issuer's call dates, increasing, before the last; empty for
#'   no call. A call needs `paths > 0`.
#' @param call_price The amount paid on a call, like `redemption`.
#' @param training_seed Seed of the paths of the call rule's regressions.
#' @param training_paths Number of those paths; more make the rule, and the derivatives, more accurate.
#' @param target The target, in units of the notional; `Inf` for none.
#' @param target_coupon `"capped"` or `"full"`: the coupon of the date that reaches the target.
#' @param make_whole `TRUE` to pay the shortfall at the last date if the target is not reached.
#' @param knock_out_level FX level (domestic per foreign) of the knock-out; `Inf` for none.
#' @param knock_out_dates Positions in `coupon_times` where the knock-out is observed, before the last; `NULL` for all
#'   of them.
#' @param bump Step of the callable note's central differences. Paths that cross the fixed rule's boundary add a jump
#'   divided by the step, so a small step makes the estimate noisy.
#' @return A list with `summary`, a one-row tibble with `value`, `standard_error` (0 for the closed form) and
#'   `analytic` (the closed form without the call), and `coupons`, a tibble with each coupon's `time` and
#'   `effective_volatility`, `sensitivity`, a tibble of the closed form's derivatives (`parameter`, `d_npv`) with respect
#'   to the spot, the FX volatility, each rate's mean reversion and volatility and the three correlations, and
#'   `curve_sensitivity`, as in [compute_prdc_value()] (`d_npv` per unit of log discount factor, `npv_per_bp` for +1bp
#'   on the node's continuous zero rate). The derivatives are of the note without the call.
#'   With call dates, `summary` adds `noncallable_value` and `hindsight_value`, and `calls` is a
#'   tibble of the call dates' `call_probability`, `call_sensitivity` a tibble (`parameter`, `d_npv`,
#'   `standard_error`) of the callable note's derivatives, and `call_curve_sensitivity` its curve nodes, with
#'   `standard_error`. With a target or knock-out, `summary` adds `plain_value` (neither, same paths) and `expected_life`, and
#'   `endings` is a tibble of each coupon date's `target_probability` and `knock_out_probability`.
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
                                            paths = 0L, steps_per_year = 12L, seed = 1, call_dates = integer(), call_price = redemption,
                                            training_seed = seed + 1, training_paths = paths, bump = 1e-3,
                                            target = Inf, target_coupon = "capped", make_whole = FALSE, knock_out_level = Inf,
                                            knock_out_dates = NULL) {
  i <- tibble(coupon_times = as.numeric(coupon_times), accrual_fractions = as.numeric(accrual_fractions), fixed_rate = as.numeric(fixed_rate))
  x <- checked(.Call(wrap__compute_prdc_three_factor_value, as.numeric(spot), as.numeric(fx_volatility), domestic_curve$pointer,
                     foreign_curve$pointer, as.numeric(domestic_rate), as.numeric(foreign_rate), as.numeric(correlation),
                     i$coupon_times, i$accrual_fractions, i$fixed_rate, as.numeric(fx_base), as.numeric(leverage), as.numeric(subtraction),
                     as.numeric(floor), as.numeric(cap), as.numeric(notional), as.numeric(redemption), as.integer(paths),
                     as.integer(steps_per_year), as.numeric(seed), as.integer(call_dates) - 1L, as.numeric(call_price),
                     as.numeric(training_seed), as.integer(training_paths), as.numeric(bump), as.numeric(target),
                     match.arg(target_coupon, c("capped", "full")), as_flag(make_whole), as.numeric(knock_out_level),
                     if (is.null(knock_out_dates)) seq_len(max(length(coupon_times) - 1L, 0L)) - 1L else as.integer(knock_out_dates) - 1L))
  out <- list(summary = tibble(value = x$value, standard_error = x$standard_error, analytic = x$analytic),
              coupons = tibble(time = i$coupon_times, effective_volatility = x$effective_volatility),
              sensitivity = tibble(parameter = x$parameter, d_npv = x$d_parameter),
              curve_sensitivity = tibble(curve = x$curve, date = as_date(x$curve_date), time = x$curve_time, d_npv = x$d_curve,
                                         npv_per_bp = -x$d_curve * x$curve_time * 1e-4))
  if (length(call_dates) > 0L) {
    out$summary <- tibble::add_column(out$summary, noncallable_value = x$noncallable_value, hindsight_value = x$hindsight_value)
    out$calls <- tibble(call_date = as.integer(call_dates), time = i$coupon_times[call_dates], call_probability = x$call_probability)
    out$call_sensitivity <- tibble(parameter = x$parameter, d_npv = x$call_d_parameter, standard_error = x$call_parameter_error)
    out$call_curve_sensitivity <- tibble(curve = x$call_curve, date = as_date(x$call_curve_date), time = x$call_curve_time,
                                         d_npv = x$call_d_curve, standard_error = x$call_curve_error,
                                         npv_per_bp = -x$call_d_curve * x$call_curve_time * 1e-4)
  }
  if (is.finite(target) || isTRUE(make_whole) || is.finite(knock_out_level)) {
    out$summary <- tibble::add_column(out$summary, plain_value = x$plain_value, expected_life = x$expected_life)
    out$endings <- tibble(time = i$coupon_times, target_probability = x$target_probability, knock_out_probability = x$knock_out_probability)
  }
  out
}

#' Calibration of the three-factor PRDC model
#'
#' Calibrates the model of [compute_prdc_three_factor_value()]: each short rate's Hull-White mean reversion and
#' volatility to its own currency's swaptions (or bond options) on its own curve, by [calibrate_hull_white()], then
#' the FX volatility to FX option volatilities with those rates and the given correlations. Under each maturity's
#' domestic forward measure the FX forward is lognormal with the three-factor variance `V(T)`, so a European FX
#' option is Black's formula with `sqrt(V(T) / T)`; `V` is a quadratic in the FX volatility, so one quote is matched
#' exactly and several are fitted by least squares on the volatilities. The correlations are inputs, not fitted.
#'
#' @inheritParams compute_prdc_three_factor_value
#' @param domestic_swaptions,foreign_swaptions Swaptions of each currency, as `swaptions` of [calibrate_hull_white()].
#' @param fx_options A data frame with `maturity` (years) and `volatility` (Black) of FX options.
#' @param ... Passed to [calibrate_hull_white()] (starting point, `fix_mean_reversion`, ...).
#' @return A list with `domestic_rate` and `foreign_rate` (`c(mean_reversion, volatility)`, for
#'   [compute_prdc_three_factor_value()]), `fx_volatility`, the rate calibrations `domestic_fit` and `foreign_fit`, and
#'   `fx_fit`, a tibble of the FX quotes with the model's volatilities.
#' @export
calibrate_prdc_three_factor <- function(spot, domestic_curve, foreign_curve, domestic_swaptions, foreign_swaptions, correlation = c(0, 0, 0),
                                       fx_options, ...) {
  domestic <- calibrate_hull_white(domestic_curve, swaptions = domestic_swaptions, ...)
  foreign <- calibrate_hull_white(foreign_curve, swaptions = foreign_swaptions, ...)
  rates <- list(c(domestic$mean_reversion, domestic$volatility), c(foreign$mean_reversion, foreign$volatility))
  fx <- checked(.Call(wrap__calibrate_prdc_fx_volatility, as.numeric(spot), domestic_curve$pointer, foreign_curve$pointer, rates[[1]], rates[[2]],
                      as.numeric(correlation), as.numeric(fx_options$maturity), as.numeric(fx_options$volatility)))
  list(domestic_rate = rates[[1]], foreign_rate = rates[[2]], fx_volatility = fx$fx_volatility, domestic_fit = domestic, foreign_fit = foreign,
       fx_fit = tibble(maturity = as.numeric(fx_options$maturity), volatility = as.numeric(fx_options$volatility),
                       model_volatility = fx$model_volatilities))
}
