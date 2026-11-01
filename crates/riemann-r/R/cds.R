# Credit default swaps. Calculations are in Rust.

#' Credit default swap
#'
#' A swap with face value claims, paying accrual and the claim at default. The side receives the
#' protection (`"buyer"`) or sells it (`"seller"`). When the rule is `"cds"` or `"cds2015"`, the
#' first date is the twentieth of a main IMM month before the effective date, as in QuantLib.
#'
#' @param side `"buyer"` or `"seller"`.
#' @param notional,running_spread The notional and the coupon rate as a decimal.
#' @param effective_date,termination_date Dates.
#' @param tenor Period of the premium, such as `"3M"`.
#' @param calendar,convention,termination_convention,rule,payment_convention,day_count Conventions;
#'   see [RiemannFinMath-package]. The defaults are examples, not market conventions.
#' @param upfront Paid at the cash settlement date, per unit of notional.
#' @param rebates_accrual `TRUE` or `FALSE`. `NA` is an error.
#' @return A `riemann_cds` value. It does not change.
#' @examples
#' cds <- credit_default_swap("buyer", 1e7, 0.01, "2026-11-03", "2031-12-20")
#' rates <- build_ois_curve("2026-11-02", "2026-11-02", "tonar", "actual_365_fixed",
#'                          ois_instruments(c("1Y", "5Y", "10Y"), c(0.01, 0.015, 0.02), 2L, 1L, 0L))
#' hazard <- hazard_curve("2026-11-02", "2040-01-01", 0.02)
#' cds_value(cds, hazard, 0.4, rates, "2026-11-02", "midpoint")
#' @export
credit_default_swap <- function(side, notional, running_spread, effective_date, termination_date, tenor = "3M", calendar = "weekends_only",
                                convention = "following", termination_convention = "unadjusted", rule = "forward", payment_convention = "following",
                                day_count = "actual_360", upfront = 0, rebates_accrual = TRUE) {
  pointer <- checked(.Call(wrap__credit_default_swap, side, as.numeric(notional), as.numeric(running_spread), as_serial(effective_date), as_serial(termination_date), tenor, calendar,
                           convention, termination_convention, rule, payment_convention, day_count, as.numeric(upfront), as_flag(rebates_accrual)))$value
  structure(list(pointer = pointer), class = "riemann_cds")
}

#' @export
print.riemann_cds <- function(x, ...) {
  cat("<riemann_cds>\n")
  invisible(x)
}

#' Survival curves and CDS values
#'
#' `hazard_curve()` makes a curve with piecewise flat hazard rates, where each rate holds up to its
#' date, and the last rate holds beyond it. `bootstrap_hazard_curve()` fits the rates to running
#' spreads. `cds_value()` values a swap with the MidPoint or ISDA engine. The ISDA engine needs both
#' curves on the evaluation date, on Actual/365 (Fixed).
#'
#' `cds_quote_sensitivity()` gives the change of the value for +1bp on each spread, with the hazard
#' curve following, and the change for +1% of recovery with the hazard curve fixed. These derivatives
#' come from automatic differentiation and the implicit function theorem.
#'
#' @param reference_date,evaluation_date Dates.
#' @param dates,hazard Node dates and hazard rates.
#' @param day_count Day count of the hazard curve's times.
#' @param curve A survival curve from `hazard_curve()` or `bootstrap_hazard_curve()`.
#' @param date Dates.
#' @param cds A swap from [credit_default_swap()].
#' @param recovery Recovery rate.
#' @param rates A curve from [build_ois_curve()].
#' @param engine `"midpoint"` or `"isda"`.
#' @param maturity,spread Maturities and running spreads of the quotes.
#' @param settlement_days,calendar,tenor,payment_convention,rule,cds_day_count,accuracy Conventions of
#'   the quotes and the accuracy of the search; the defaults are examples, not market conventions.
#' @param credit A curve from `bootstrap_hazard_curve()`.
#' @return `hazard_curve()` returns a `riemann_hazard_curve`, and `bootstrap_hazard_curve()` also keeps
#'   its `nodes` tibble. `survival_probability()` returns a numeric vector. `cds_value()` returns a
#'   one-row tibble with `npv`, `coupon_leg_npv`, `default_leg_npv`, `upfront_npv`,
#'   `accrual_rebate_npv`, `fair_spread` and `fair_upfront`. `cds_quote_sensitivity()` returns a tibble
#'   with `maturity`, `spread`, `npv`, `npv_per_bp` and `npv_per_recovery_percent`.
#' @export
hazard_curve <- function(reference_date, dates, hazard, day_count = "actual_365_fixed") {
  pointer <- checked(.Call(wrap__hazard_curve, as_serial(reference_date), day_count, as_serial(dates), as.numeric(hazard)))$value
  structure(list(pointer = pointer), class = "riemann_hazard_curve")
}

#' @export
print.riemann_hazard_curve <- function(x, ...) {
  cat("<riemann_hazard_curve>\n")
  invisible(x)
}

#' @rdname hazard_curve
#' @export
survival_probability <- function(curve, date, on_error = "stop") {
  finish_values(.Call(wrap__survival_probability, curve$pointer, as_serial(date)), on_error)
}

#' @rdname hazard_curve
#' @param on_error `"stop"` or `"na"`; see [RiemannFinMath-package].
#' @export
cds_value <- function(cds, curve, recovery, rates, evaluation_date, engine = "midpoint") {
  tibble::as_tibble(checked(.Call(wrap__cds_value, cds$pointer, curve$pointer, as.numeric(recovery), rates$pointer, as_serial(evaluation_date), engine)))
}

#' @rdname hazard_curve
#' @export
bootstrap_hazard_curve <- function(evaluation_date, maturity, spread, recovery, rates, engine = "midpoint", settlement_days = 0L, calendar = "weekends_only", tenor = "3M",
                                   payment_convention = "following", rule = "backward", cds_day_count = "actual_360", day_count = "actual_365_fixed", accuracy = 1e-12) {
  i <- tibble(maturity = as_serial(maturity), spread = as.numeric(spread))
  x <- checked(.Call(wrap__bootstrap_hazard_curve, as_serial(evaluation_date), i$maturity, i$spread, as.numeric(recovery), rates$pointer, engine, as.integer(settlement_days),
                     calendar, tenor, payment_convention, rule, cds_day_count, day_count, as.numeric(accuracy)))
  structure(list(pointer = x$hazard_curve, credit = x$value, nodes = tibble(date = as_date(x$node_date), hazard = x$hazard)), class = "riemann_hazard_curve")
}

#' @rdname hazard_curve
#' @export
cds_quote_sensitivity <- function(cds, credit, rates, evaluation_date, engine = "midpoint") {
  x <- checked(.Call(wrap__cds_quote_sensitivity, cds$pointer, credit$credit, rates$pointer, as_serial(evaluation_date), engine))
  tibble(maturity = as_date(x$maturity), spread = x$spread, npv = x$npv, npv_per_bp = x$d_npv * 1e-4, npv_per_recovery_percent = x$d_npv_d_recovery * 0.01)
}

#' Maturity of a standard CDS
#'
#' The twentieth of a main IMM month, as QuantLib `cdsMaturity`.
#'
#' @param trade_date A date.
#' @param tenor A multiple of 3 months, such as `"5Y"`.
#' @param rule `"cds"` or `"cds2015"`.
#' @param on_error `"stop"` or `"na"`; see [RiemannFinMath-package].
#' @return A `Date`.
#' @examples
#' cds_maturity("2026-11-02", "5Y", "cds2015")
#' @export
cds_maturity <- function(trade_date, tenor, rule = "cds2015", on_error = "stop") {
  as_date(finish_values(.Call(wrap__cds_maturity, as_serial(trade_date), tenor, rule), on_error))
}
