# Hull-White and SABR. Calculations are in Rust. Times are in years from the curve's reference date.

#' Hull-White model on a curve
#'
#' The one-factor model `dr = (theta(t) - a r) dt + sigma dW`, fitted to `curve`. These are model values:
#' the parameters must come from calibration, and a match with another library checks the
#' implementation, not the model.
#'
#' `hull_white_swaption()` uses Jamshidian's decomposition, and its derivatives with respect to the
#' parameters come from automatic differentiation. `hull_white_bermudan_swaption()` and
#' `hull_white_callable_bond()` use a backward induction on a state grid, where each step is an exact
#' conditional expectation, integrated numerically.
#'
#' The kink at an exercise boundary limits Gauss-Hermite quadrature, so the Bermudan default is the
#' trapezoid rule. Set `spacing` to compare. The callable bond's default, `nodes = 24`, repeats
#' LagrangeFinance II-8.
#'
#' @param option_type `"call"` or `"put"`.
#' @param strike Strike on a bond worth 1 at maturity.
#' @param maturity Option expiry, in years.
#' @param bond_maturity Maturity of the zero-coupon bond.
#' @param payer `TRUE` to pay the fixed leg, `FALSE` to receive it. `NA` is an error.
#' @param payment_times,coupons Payment times of the fixed leg and its coupons, without the notional.
#' @param nominal Notional.
#' @param exercise_times Exercise times, among the period starts.
#' @param mean_reversion,volatility The parameters `a` (positive on the grid) and `sigma`.
#' @param curve A curve from [build_ois_curve()].
#' @param points,limit Number of states and half-width of the grid of the state.
#' @param nodes Gauss-Hermite nodes, used when `spacing` is 0.
#' @param spacing Spacing of the trapezoid rule in standard deviations, or 0 for Gauss-Hermite.
#' @param time,coupon,principal,call_price Dates of a callable bond: payment times, coupons,
#'   principal paid, and the call price after the coupon, or `NA` where there is no call.
#' @param on_error `"stop"` or `"na"`; see [RiemannFinMath-package].
#' @return `hull_white_bond_option()` returns a numeric vector, `hull_white_swaption()` a one-row
#'   tibble with `value`, `d_mean_reversion` and `d_volatility`, `hull_white_bermudan_swaption()` a
#'   number, and `hull_white_callable_bond()` a one-row tibble with `bullet`, `callable` and
#'   `issuer_call_value`.
#' @examples
#' instruments <- ois_instruments(c("1Y", "5Y", "10Y", "20Y"), c(0.01, 0.015, 0.02, 0.022), 2L, 1L, 0L)
#' curve <- build_ois_curve("2026-11-02", "2026-11-02", "tonar", "actual_365_fixed", instruments)
#' hull_white_bond_option("put", 0.95, 2, 5, 0.1, 0.01, curve)
#' hull_white_swaption(TRUE, 2, 3:7, rep(0.02, 5), 0.1, 0.01, curve)
#' hull_white_bermudan_swaption(TRUE, 2, 3:7, rep(0.02, 5), 2:6, 0.1, 0.01, curve)
#' hull_white_callable_bond(
#'   time = (1:10) / 2, coupon = 1.5, principal = c(rep(0, 9), 100),
#'   call_price = c(NA, rep(100, 8), NA), 0.15, 0.012, curve
#' )
#' @export
hull_white_bond_option <- function(option_type, strike, maturity, bond_maturity, mean_reversion, volatility, curve, on_error = "stop") {
  i <- tibble(option_type = as.character(option_type), strike = as.numeric(strike), maturity = as.numeric(maturity), bond_maturity = as.numeric(bond_maturity))
  finish_values(.Call(wrap__hull_white_bond_option, i$option_type, i$strike, i$maturity, i$bond_maturity, as.numeric(mean_reversion), as.numeric(volatility), curve$pointer), on_error)
}

#' @rdname hull_white_bond_option
#' @export
hull_white_swaption <- function(payer, maturity, payment_times, coupons, mean_reversion, volatility, curve, nominal = 1) {
  tibble::as_tibble(checked(.Call(wrap__hull_white_swaption, as_flag(payer), as.numeric(maturity), as.numeric(payment_times), as.numeric(coupons),
                                  as.numeric(nominal), as.numeric(mean_reversion), as.numeric(volatility), curve$pointer)))
}

#' @rdname hull_white_bond_option
#' @export
hull_white_bermudan_swaption <- function(payer, maturity, payment_times, coupons, exercise_times, mean_reversion, volatility, curve, nominal = 1,
                                         points = 801L, limit = 0.25, nodes = 24L, spacing = 0.05) {
  checked(.Call(wrap__hull_white_bermudan_swaption, as_flag(payer), as.numeric(maturity), as.numeric(payment_times), as.numeric(coupons), as.numeric(nominal),
                as.numeric(exercise_times), as.numeric(mean_reversion), as.numeric(volatility), curve$pointer, as.integer(points), as.numeric(limit),
                as.integer(nodes), as.numeric(spacing)))$value
}

#' @rdname hull_white_bond_option
#' @export
hull_white_callable_bond <- function(time, coupon, principal, call_price, mean_reversion, volatility, curve, points = 321L, limit = 0.13, nodes = 24L, spacing = 0) {
  i <- tibble(time = as.numeric(time), coupon = as.numeric(coupon), principal = as.numeric(principal), call_price = as.numeric(call_price))
  tibble::as_tibble(checked(.Call(wrap__hull_white_callable_bond, i$time, i$coupon, i$principal, i$call_price, as.numeric(mean_reversion), as.numeric(volatility),
                                  curve$pointer, as.integer(points), as.numeric(limit), as.integer(nodes), as.numeric(spacing))))
}

#' SABR volatility and calibration
#'
#' The expansions of Hagan et al. Forward and strike must be positive; the shifted form is not
#' implemented. `calibrate_sabr()` fits alpha, nu and rho with beta fixed, by Levenberg-Marquardt
#' with exact derivatives, and repeats the same result for the same inputs.
#'
#' @param kind `"lognormal"` or `"normal"`.
#' @param strike,forward Positive rates.
#' @param expiry Time to expiry in years.
#' @param alpha,beta,nu,rho The parameters.
#' @param volatility Volatilities at `strike`, to fit.
#' @param on_error `"stop"` or `"na"`; see [RiemannFinMath-package].
#' @return `sabr_volatility()` returns a numeric vector. `calibrate_sabr()` returns a one-row tibble
#'   with `alpha`, `beta`, `nu`, `rho`, `root_mean_square_error` and `iterations`.
#' @examples
#' k <- c(0.02, 0.025, 0.03, 0.035, 0.04)
#' v <- sabr_volatility("normal", k, 0.03, 2, 0.03, 0.5, 0.4, -0.3)
#' calibrate_sabr("normal", k, v, 0.03, 2, 0.5)
#' @export
sabr_volatility <- function(kind, strike, forward, expiry, alpha, beta, nu, rho, on_error = "stop") {
  i <- tibble(strike = as.numeric(strike), forward = as.numeric(forward), expiry = as.numeric(expiry), alpha = as.numeric(alpha),
              beta = as.numeric(beta), nu = as.numeric(nu), rho = as.numeric(rho))
  finish_values(checked(.Call(wrap__sabr_volatility, kind, i$strike, i$forward, i$expiry, i$alpha, i$beta, i$nu, i$rho)), on_error)
}

#' @rdname sabr_volatility
#' @export
calibrate_sabr <- function(kind, strike, volatility, forward, expiry, beta) {
  tibble::as_tibble(checked(.Call(wrap__calibrate_sabr, kind, as.numeric(strike), as.numeric(volatility), as.numeric(forward), as.numeric(expiry), as.numeric(beta))))
}
