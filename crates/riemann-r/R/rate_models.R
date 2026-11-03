# Hull-White and SABR. Calculations are in Rust. Times are in years from the curve's reference date.

#' Hull-White model on a curve
#'
#' The one-factor model `dr = (theta(t) - a r) dt + sigma dW`, fitted to `curve`. These are model values:
#' the parameters must come from calibration, and a match with another library checks the
#' implementation, not the model.
#'
#' `compute_hull_white_swaption_value()` uses Jamshidian's decomposition, and its derivatives with respect to the
#' parameters come from automatic differentiation. `compute_hull_white_bermudan_swaption_value()` and
#' `compute_hull_white_callable_bond_value()` use a backward induction on a state grid, where each step is an exact
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
#' @param curve A curve from [bootstrap_ois_curve()].
#' @param points,limit Number of states and half-width of the grid of the state.
#' @param nodes Gauss-Hermite nodes, used when `spacing` is 0.
#' @param spacing Spacing of the trapezoid rule in standard deviations, or 0 for Gauss-Hermite.
#' @param time,coupon,principal,call_price Dates of a callable bond: payment times, coupons,
#'   principal paid, and the call price after the coupon, or `NA` where there is no call.
#' @param on_error `"stop"` or `"na"`; see [RiemannFinMath-package].
#' @return `compute_hull_white_bond_option_value()` returns a numeric vector, `compute_hull_white_swaption_value()` a one-row
#'   tibble with `value`, `d_mean_reversion` and `d_volatility`, `compute_hull_white_bermudan_swaption_value()` a
#'   number, and `compute_hull_white_callable_bond_value()` a one-row tibble with `bullet`, `callable` and
#'   `issuer_call_value`.
#' @examples
#' instruments <- create_ois_instruments(c("1Y", "5Y", "10Y", "20Y"), c(0.01, 0.015, 0.02, 0.022), 2L, 1L, 0L)
#' curve <- bootstrap_ois_curve("2026-11-02", "2026-11-02", "tonar", "actual_365_fixed", instruments)
#' compute_hull_white_bond_option_value("put", 0.95, 2, 5, 0.1, 0.01, curve)
#' compute_hull_white_swaption_value(TRUE, 2, 3:7, rep(0.02, 5), 0.1, 0.01, curve)
#' compute_hull_white_bermudan_swaption_value(TRUE, 2, 3:7, rep(0.02, 5), 2:6, 0.1, 0.01, curve)
#' compute_hull_white_callable_bond_value(
#'   time = (1:10) / 2, coupon = 1.5, principal = c(rep(0, 9), 100),
#'   call_price = c(NA, rep(100, 8), NA), 0.15, 0.012, curve
#' )
#' @export
compute_hull_white_bond_option_value <- function(option_type, strike, maturity, bond_maturity, mean_reversion, volatility, curve, on_error = "stop") {
  i <- tibble(option_type = as.character(option_type), strike = as.numeric(strike), maturity = as.numeric(maturity), bond_maturity = as.numeric(bond_maturity))
  finish_values(.Call(wrap__compute_hull_white_bond_option_value, i$option_type, i$strike, i$maturity, i$bond_maturity, as.numeric(mean_reversion), as.numeric(volatility), curve$pointer), on_error)
}

#' @rdname compute_hull_white_bond_option_value
#' @export
compute_hull_white_swaption_value <- function(payer, maturity, payment_times, coupons, mean_reversion, volatility, curve, nominal = 1) {
  tibble::as_tibble(checked(.Call(wrap__compute_hull_white_swaption_value, as_flag(payer), as.numeric(maturity), as.numeric(payment_times), as.numeric(coupons),
                                  as.numeric(nominal), as.numeric(mean_reversion), as.numeric(volatility), curve$pointer)))
}

#' @rdname compute_hull_white_bond_option_value
#' @export
compute_hull_white_bermudan_swaption_value <- function(payer, maturity, payment_times, coupons, exercise_times, mean_reversion, volatility, curve, nominal = 1,
                                         points = 801L, limit = 0.25, nodes = 24L, spacing = 0.05) {
  checked(.Call(wrap__compute_hull_white_bermudan_swaption_value, as_flag(payer), as.numeric(maturity), as.numeric(payment_times), as.numeric(coupons), as.numeric(nominal),
                as.numeric(exercise_times), as.numeric(mean_reversion), as.numeric(volatility), curve$pointer, as.integer(points), as.numeric(limit),
                as.integer(nodes), as.numeric(spacing)))$value
}

#' @rdname compute_hull_white_bond_option_value
#' @export
compute_hull_white_callable_bond_value <- function(time, coupon, principal, call_price, mean_reversion, volatility, curve, points = 321L, limit = 0.13, nodes = 24L, spacing = 0) {
  i <- tibble(time = as.numeric(time), coupon = as.numeric(coupon), principal = as.numeric(principal), call_price = as.numeric(call_price))
  tibble::as_tibble(checked(.Call(wrap__compute_hull_white_callable_bond_value, i$time, i$coupon, i$principal, i$call_price, as.numeric(mean_reversion), as.numeric(volatility),
                                  curve$pointer, as.integer(points), as.numeric(limit), as.integer(nodes), as.numeric(spacing))))
}

#' Calibrate the Hull-White model to market prices
#'
#' Fits the mean reversion and the volatility to market prices of options on zero-coupon bonds and
#' European swaptions, or the volatility alone with `fix_mean_reversion = TRUE`. It minimizes
#' `sum(weight * (model - price)^2)` by Levenberg-Marquardt on `log(a)` and `log(sigma)`, with exact
#' derivatives, and repeats the same result for the same inputs. The result is the fit of the model to
#' these prices, not a statement that the model is right.
#'
#' This is not the method of QuantLib or of the legacy text, and `docs/spec_hull_white.md` lists the
#' differences. The default objective of QuantLib is the relative price error, which
#' `weight = 1 / price^2` reproduces. A mean reversion that the data pushes to zero stops at about
#' `1.49e-8` and sets `at_lower_limit`: a logarithm cannot reach zero. To fit with a mean reversion of
#' exactly zero, use `initial_mean_reversion = 0` with `fix_mean_reversion = TRUE`.
#'
#' @param curve A curve, as for [compute_hull_white_bond_option_value()].
#' @param bond_options A data frame with `option_type`, `strike`, `maturity`, `bond_maturity`, `price` and
#'   optionally `weight`, or `NULL`. Maturities are in years from the date of the curve.
#' @param swaptions A data frame with `payer`, `maturity`, the list columns `payment_times` and `coupons`
#'   (coupon amounts, without the notional), `price`, and optionally `nominal` (1) and `weight` (1), or `NULL`.
#' @param initial_mean_reversion,initial_volatility The starting point; positive. A fixed mean
#'   reversion may be zero.
#' @param fix_mean_reversion Hold the mean reversion at `initial_mean_reversion`.
#' @param max_iterations The largest number of iterations. Reaching it is reported in `stop_reason`, not an
#'   error, so check that column.
#' @return A one-row tibble with `mean_reversion`, `volatility`, `objective`, `root_mean_square_error`,
#'   `at_lower_limit`, `iterations`, `stop_reason` (`"cost_change"`, `"small_step"`, `"gradient"`,
#'   `"no_further_reduction"` or `"max_iterations"`) and the list column `repricing`: a tibble of `instrument`,
#'   `market_price`, `model_price` and `difference`, bond options first.
#' @examples
#' instruments <- create_ois_instruments(c("1Y", "5Y", "10Y", "20Y"), c(0.01, 0.015, 0.02, 0.022), 2L, 1L, 0L)
#' curve <- bootstrap_ois_curve("2026-11-02", "2026-11-02", "tonar", "actual_365_fixed", instruments)
#' options <- tibble::tibble(
#'   option_type = "call", strike = c(0.98, 0.95, 0.9), maturity = c(0.5, 1, 2), bond_maturity = c(2, 3, 5)
#' )
#' options$price <- compute_hull_white_bond_option_value(
#'   options$option_type, options$strike, options$maturity, options$bond_maturity, 0.08, 0.012, curve
#' )
#' calibrate_hull_white(curve, bond_options = options)
#' @export
calibrate_hull_white <- function(curve, bond_options = NULL, swaptions = NULL, initial_mean_reversion = 0.05, initial_volatility = 0.01,
                                 fix_mean_reversion = FALSE, max_iterations = 200L) {
  column_or <- function(frame, name, default) if (name %in% names(frame)) as.numeric(frame[[name]]) else rep(default, nrow(frame))
  need <- function(frame, columns, arg) {
    missing_columns <- setdiff(columns, names(frame))
    if (length(missing_columns)) stop("`", arg, "` needs the columns ", paste(missing_columns, collapse = ", "), ".", call. = FALSE)
  }
  if (is.null(bond_options)) bond_options <- tibble::tibble(option_type = character(), strike = numeric(), maturity = numeric(), bond_maturity = numeric(), price = numeric())
  if (is.null(swaptions)) swaptions <- tibble::tibble(payer = logical(), maturity = numeric(), payment_times = list(), coupons = list(), price = numeric())
  need(bond_options, c("option_type", "strike", "maturity", "bond_maturity", "price"), "bond_options")
  need(swaptions, c("payer", "maturity", "payment_times", "coupons", "price"), "swaptions")
  times <- lapply(swaptions$payment_times, as.numeric)
  coupons <- lapply(swaptions$coupons, as.numeric)
  if (!identical(lengths(times), lengths(coupons))) stop("Each swaption needs as many coupons as payment times.", call. = FALSE)
  fit <- checked(.Call(
    wrap__calibrate_hull_white, as.character(bond_options$option_type), as.numeric(bond_options$strike), as.numeric(bond_options$maturity),
    as.numeric(bond_options$bond_maturity), as.numeric(bond_options$price), column_or(bond_options, "weight", 1),
    as.integer(swaptions$payer), as.numeric(swaptions$maturity), as.integer(lengths(times)), as.numeric(unlist(times)), as.numeric(unlist(coupons)),
    column_or(swaptions, "nominal", 1), as.numeric(swaptions$price), column_or(swaptions, "weight", 1), curve$pointer,
    as.numeric(initial_mean_reversion), as.numeric(initial_volatility), as_flag(fix_mean_reversion), as.integer(max_iterations)
  ))
  market <- c(as.numeric(bond_options$price), as.numeric(swaptions$price))
  repricing <- tibble::tibble(
    instrument = rep(c("bond_option", "swaption"), c(nrow(bond_options), nrow(swaptions))),
    market_price = market, model_price = fit$model_prices, difference = fit$model_prices - market
  )
  fit$model_prices <- NULL
  fit$repricing <- list(repricing)
  tibble::as_tibble(fit)
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
#' @return `compute_sabr_volatility()` returns a numeric vector. `calibrate_sabr()` returns a one-row tibble
#'   with `alpha`, `beta`, `nu`, `rho`, `root_mean_square_error` and `iterations`.
#' @examples
#' k <- c(0.02, 0.025, 0.03, 0.035, 0.04)
#' v <- compute_sabr_volatility("normal", k, 0.03, 2, 0.03, 0.5, 0.4, -0.3)
#' calibrate_sabr("normal", k, v, 0.03, 2, 0.5)
#' @export
compute_sabr_volatility <- function(kind, strike, forward, expiry, alpha, beta, nu, rho, on_error = "stop") {
  i <- tibble(strike = as.numeric(strike), forward = as.numeric(forward), expiry = as.numeric(expiry), alpha = as.numeric(alpha),
              beta = as.numeric(beta), nu = as.numeric(nu), rho = as.numeric(rho))
  finish_values(checked(.Call(wrap__compute_sabr_volatility, kind, i$strike, i$forward, i$expiry, i$alpha, i$beta, i$nu, i$rho)), on_error)
}

#' @rdname compute_sabr_volatility
#' @export
calibrate_sabr <- function(kind, strike, volatility, forward, expiry, beta) {
  tibble::as_tibble(checked(.Call(wrap__calibrate_sabr, kind, as.numeric(strike), as.numeric(volatility), as.numeric(forward), as.numeric(expiry), as.numeric(beta))))
}
