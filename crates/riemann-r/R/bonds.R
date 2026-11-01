# Bonds. Every convention is an explicit argument, because QuantLib's defaults are not market
# conventions. Names are those of the Rust functions.

#' Build a fixed-rate bond
#'
#' Returns a value that does not change. Conventions have no defaults, because QuantLib's defaults
#' are not market conventions.
#'
#' @param settlement_days Business days from the evaluation date to settlement.
#' @param face_amount Face amount.
#' @param effective_date,termination_date First and last dates of the schedule: `Date` or ISO string.
#' @param tenor Coupon period, such as `"6M"` or `"1Y"`.
#' @param calendar Calendar name; see [is_business_day()].
#' @param convention,termination_convention Business day conventions of the schedule dates and
#'   of the termination date; see [RiemannFinMath-package].
#' @param rule Date generation rule; see [RiemannFinMath-package].
#' @param end_of_month `TRUE` or `FALSE`. `NA` is an error.
#' @param coupon_rate Coupon rate as a decimal: `0.015` is 1.5%.
#' @param day_count Day count of the coupons; see [RiemannFinMath-package].
#' @param payment_convention Business day convention of the payment dates.
#' @param redemption Redemption per 100 of notional.
#' @param first_date,next_to_last_date Optional stub dates, or `NULL`.
#' @param issue_date Optional issue date, or `NULL`.
#' @return A `riemann_bond` value, to pass to the other bond functions.
#' @examples
#' bond <- fixed_rate_bond(
#'   settlement_days = 1L, face_amount = 100,
#'   effective_date = "2020-03-20", termination_date = as.Date("2030-03-20"),
#'   tenor = "6M", calendar = "japan", convention = "following",
#'   termination_convention = "following", rule = "backward", end_of_month = FALSE,
#'   coupon_rate = 0.015, day_count = "act_act_isma", payment_convention = "following",
#'   redemption = 100, issue_date = "2020-03-20"
#' )
#' bond
#' @export
fixed_rate_bond <- function(settlement_days, face_amount, effective_date, termination_date, tenor, calendar,
                            convention, termination_convention, rule, end_of_month, coupon_rate, day_count,
                            payment_convention, redemption, first_date = NULL, next_to_last_date = NULL,
                            issue_date = NULL) {
  pointer <- checked(.Call(
    wrap__fixed_rate_bond, as.integer(settlement_days), as.numeric(face_amount), as_serial(effective_date),
    as_serial(termination_date), tenor, calendar, convention, termination_convention, rule, as_flag(end_of_month),
    as_optional_serial(first_date), as_optional_serial(next_to_last_date), as.numeric(coupon_rate), day_count,
    payment_convention, as.numeric(redemption), as_optional_serial(issue_date)
  ))$value
  structure(
    list(pointer = pointer, coupon_rate = coupon_rate, termination_date = as_date(as_serial(termination_date))),
    class = "riemann_bond"
  )
}

#' @export
print.riemann_bond <- function(x, ...) {
  cat("<riemann_bond> coupon", x$coupon_rate, "maturing", format(x$termination_date), "\n")
  invisible(x)
}

#' Settlement date of a bond
#'
#' @param bond A bond from [fixed_rate_bond()].
#' @param evaluation_date Evaluation dates: `Date` or ISO strings.
#' @param on_error `"stop"` or `"na"`; see [RiemannFinMath-package].
#' @return A `Date` vector, one for each evaluation date.
#' @examples
#' bond <- fixed_rate_bond(1L, 100, "2020-03-20", "2030-03-20", "6M", "japan", "following",
#'                         "following", "backward", FALSE, 0.015, "act_act_isma", "following", 100)
#' settlement_date(bond, "2026-11-02")
#' @export
settlement_date <- function(bond, evaluation_date, on_error = "stop") {
  rows <- .Call(wrap__settlement_date, bond$pointer, as_serial(evaluation_date))
  as_date(finish_values(rows, on_error))
}

#' Prices and risk of a bond
#'
#' `bond_values()` returns every measure, one row for each yield and settlement date. The other
#' functions return one measure. A measure that fails is `NA`, with its message in `error`.
#'
#' `duration_macaulay` is `NA` unless the yield is compounded, because the Macaulay duration is
#' defined for compounded yields only. That is not a failure of the row; `duration()` with
#' `duration_type = "macaulay"` reports it as an error.
#'
#' @param bond A bond from [fixed_rate_bond()].
#' @param rate Yields as decimals.
#' @param day_count Day count of the yield; see [RiemannFinMath-package].
#' @param compounding `"simple"`, `"compounded"` or `"continuous"`.
#' @param frequency Compounding periods per year.
#' @param settlement_date Settlement dates: `Date` or ISO strings.
#' @param duration_type `"simple"`, `"modified"` or `"macaulay"`.
#' @param on_error `"stop"` or `"na"`; see [RiemannFinMath-package].
#' @return `bond_values()` returns a tibble with the columns `rate`, `settlement_date`,
#'   `clean_price`, `dirty_price`, `accrued_amount`, `duration_simple`, `duration_modified`,
#'   `duration_macaulay`, `convexity`, `basis_point_value`, `yield_value_basis_point` and `error`.
#'   The other functions return a numeric vector.
#' @examples
#' bond <- fixed_rate_bond(1L, 100, "2020-03-20", "2030-03-20", "6M", "japan", "following",
#'                         "following", "backward", FALSE, 0.015, "act_act_isma", "following", 100)
#' bond_values(bond, c(0.01, 0.02), "act_act_isma", "compounded", 2L, "2026-11-04")
#' clean_price(bond, 0.02, "act_act_isma", "compounded", 2L, "2026-11-04")
#' duration(bond, 0.02, "act_act_isma", "compounded", 2L, "2026-11-04", "modified")
#' accrued_amount(bond, "2026-11-04")
#' @export
bond_values <- function(bond, rate, day_count, compounding, frequency, settlement_date, on_error = "stop") {
  inputs <- tibble(rate = as.numeric(rate), settlement_date = as_serial(settlement_date))
  rows <- checked(.Call(wrap__bond_values, bond$pointer, inputs$rate, day_count, compounding, as.integer(frequency), inputs$settlement_date))
  rows <- finish_rows(rows, on_error)
  tibble(rate = inputs$rate, settlement_date = as_date(inputs$settlement_date), as_tibble(rows))
}

#' @rdname bond_values
#' @export
clean_price <- function(bond, rate, day_count, compounding, frequency, settlement_date, on_error = "stop") {
  bond_values(bond, rate, day_count, compounding, frequency, settlement_date, on_error)$clean_price
}

#' @rdname bond_values
#' @export
dirty_price <- function(bond, rate, day_count, compounding, frequency, settlement_date, on_error = "stop") {
  bond_values(bond, rate, day_count, compounding, frequency, settlement_date, on_error)$dirty_price
}

#' @rdname bond_values
#' @export
convexity <- function(bond, rate, day_count, compounding, frequency, settlement_date, on_error = "stop") {
  bond_values(bond, rate, day_count, compounding, frequency, settlement_date, on_error)$convexity
}

#' @rdname bond_values
#' @export
basis_point_value <- function(bond, rate, day_count, compounding, frequency, settlement_date, on_error = "stop") {
  bond_values(bond, rate, day_count, compounding, frequency, settlement_date, on_error)$basis_point_value
}

#' @rdname bond_values
#' @export
yield_value_basis_point <- function(bond, rate, day_count, compounding, frequency, settlement_date, on_error = "stop") {
  bond_values(bond, rate, day_count, compounding, frequency, settlement_date, on_error)$yield_value_basis_point
}

#' @rdname bond_values
#' @export
duration <- function(bond, rate, day_count, compounding, frequency, settlement_date, duration_type, on_error = "stop") {
  inputs <- tibble(rate = as.numeric(rate), settlement_date = as_serial(settlement_date))
  rows <- checked(.Call(wrap__duration, bond$pointer, inputs$rate, day_count, compounding, as.integer(frequency), inputs$settlement_date, duration_type))
  finish_values(rows, on_error)
}

#' @rdname bond_values
#' @export
accrued_amount <- function(bond, settlement_date, on_error = "stop") {
  # the accrued amount does not depend on the yield
  bond_values(bond, 0, "actual_365_fixed", "continuous", 1L, settlement_date, on_error)$accrued_amount
}

#' Yield of a bond from its price
#'
#' @inheritParams bond_values
#' @param price Prices per 100 of notional.
#' @param price_type `"clean"` or `"dirty"`.
#' @param accuracy,max_iterations,guess Solver settings; the defaults are QuantLib's.
#' @return A numeric vector of yields as decimals.
#' @examples
#' bond <- fixed_rate_bond(1L, 100, "2020-03-20", "2030-03-20", "6M", "japan", "following",
#'                         "following", "backward", FALSE, 0.015, "act_act_isma", "following", 100)
#' yield_from_price(bond, 98.5, "clean", "act_act_isma", "compounded", 2L, "2026-11-04")
#' @export
yield_from_price <- function(bond, price, price_type, day_count, compounding, frequency, settlement_date,
                             accuracy = 1e-10, max_iterations = 100L, guess = 0.05, on_error = "stop") {
  inputs <- tibble(price = as.numeric(price), settlement_date = as_serial(settlement_date))
  rows <- checked(.Call(wrap__yield_from_price, bond$pointer, inputs$price, price_type, day_count, compounding, as.integer(frequency),
                        inputs$settlement_date, as.numeric(accuracy), as.integer(max_iterations), as.numeric(guess)))
  finish_values(rows, on_error)
}

#' Value of a bond on an OIS curve
#'
#' @inheritParams settlement_date
#' @param curve A curve from [build_ois_curve()].
#' @return A tibble with the columns `evaluation_date`, `npv`, `settlement_value`, `dirty_price`,
#'   `clean_price` and `error`.
#' @examples
#' bond <- fixed_rate_bond(1L, 100, "2020-03-20", "2030-03-20", "6M", "japan", "following",
#'                         "following", "backward", FALSE, 0.015, "act_act_isma", "following", 100)
#' instruments <- ois_instruments(
#'   c("ON", "1Y", "5Y", "10Y"), c(0.002, 0.004, 0.009, 0.014), 2L, 1L, 0L
#' )
#' curve <- build_ois_curve("2026-11-02", "2026-11-02", "tonar", "actual_365_fixed", instruments)
#' value_on_curve(bond, curve, "2026-11-02")
#' @export
value_on_curve <- function(bond, curve, evaluation_date, on_error = "stop") {
  serials <- as_serial(evaluation_date)
  rows <- finish_rows(.Call(wrap__value_on_curve, bond$pointer, curve$pointer, serials), on_error)
  tibble(evaluation_date = as_date(serials), as_tibble(rows))
}

# ---- Japanese bond calculations ---------------------------------------------------------------

yen_inputs <- function(...) {
  # recycles the columns to one length
  as.list(as_tibble(lapply(list(...), as.numeric)))
}

#' Remaining years by the Japanese rule
#'
#' Years between two dates for Japanese bond calculations, truncated to 7 decimals. An anniversary
#' of February 29th in a non-leap year is February 28th, which counts as one year. This rule is
#' for Japanese bond calculations only.
#'
#' @param start,end Dates: `Date` or ISO strings.
#' @param on_error `"stop"` or `"na"`; see [RiemannFinMath-package].
#' @return A numeric vector of years.
#' @examples
#' remaining_years("2024-02-29", "2025-02-28")
#' @export
remaining_years <- function(start, end, on_error = "stop") {
  inputs <- tibble(start = as_serial(start), end = as_serial(end))
  finish_values(.Call(wrap__remaining_years, inputs$start, inputs$end), on_error)
}

#' Japanese simple and compound yields
#'
#' Prices per 100 and yields in percent, for Japanese bond calculations.
#'
#' @param t Remaining years; see [remaining_years()].
#' @param coupon_rate Coupon rate in percent per year.
#' @param simple_rate,compound_yield Yields in percent.
#' @param price Clean price per 100.
#' @param redemption Redemption per 100.
#' @param coupon_frequency,compound_frequency Coupons and compounding periods per year.
#' @param digits Decimals kept by truncation; `NA` keeps all, for the price functions.
#' @param max_iterations Maximum iterations of the solver.
#' @param on_error `"stop"` or `"na"`; see [RiemannFinMath-package].
#' @return A numeric vector.
#' @examples
#' price_from_simple_rate(5, 1.5, 1.2, 100)
#' simple_rate_from_price(5, 1.5, 101.4, 100)
#' price_from_compound_yield(5, 1.5, 1.2, 100)
#' compound_yield_from_price(5, 1.5, 101.4, 100)
#' @name yen_bond_yields
NULL

#' @rdname yen_bond_yields
#' @export
price_from_simple_rate <- function(t, coupon_rate, simple_rate, redemption, digits = 3L, on_error = "stop") {
  x <- yen_inputs(t = t, coupon_rate = coupon_rate, simple_rate = simple_rate, redemption = redemption)
  finish_values(.Call(wrap__price_from_simple_rate, x$t, x$coupon_rate, x$simple_rate, x$redemption, as.integer(digits)), on_error)
}

#' @rdname yen_bond_yields
#' @export
simple_rate_from_price <- function(t, coupon_rate, price, redemption, digits = 7L, max_iterations = 100L, on_error = "stop") {
  x <- yen_inputs(t = t, coupon_rate = coupon_rate, price = price, redemption = redemption)
  finish_values(.Call(wrap__simple_rate_from_price, x$t, x$coupon_rate, x$price, x$redemption, as.integer(digits), as.integer(max_iterations)), on_error)
}

#' @rdname yen_bond_yields
#' @export
price_from_compound_yield <- function(t, coupon_rate, compound_yield, redemption, coupon_frequency = 2L,
                                      compound_frequency = 2L, digits = 3L, on_error = "stop") {
  x <- yen_inputs(t = t, coupon_rate = coupon_rate, compound_yield = compound_yield, redemption = redemption)
  finish_values(.Call(wrap__price_from_compound_yield, x$t, x$coupon_rate, x$compound_yield, x$redemption,
                      as.integer(coupon_frequency), as.integer(compound_frequency), as.integer(digits)), on_error)
}

#' @rdname yen_bond_yields
#' @export
compound_yield_from_price <- function(t, coupon_rate, price, redemption, coupon_frequency = 2L, compound_frequency = 2L,
                                      digits = 7L, max_iterations = 100L, on_error = "stop") {
  x <- yen_inputs(t = t, coupon_rate = coupon_rate, price = price, redemption = redemption)
  finish_values(.Call(wrap__compound_yield_from_price, x$t, x$coupon_rate, x$price, x$redemption, as.integer(coupon_frequency),
                      as.integer(compound_frequency), as.integer(digits), as.integer(max_iterations)), on_error)
}

#' Risk measures of Japanese yields
#'
#' @inheritParams yen_bond_yields
#' @param accrued_interest Accrued interest per 100.
#' @return A tibble with the columns `price`, `accrued_interest`, `dirty_price`, `dp_dy_pct`,
#'   `d2p_dy2_pct`, `bpv_yen`, `bpv_sen`, `modified_duration`, `convexity` and `error`.
#' @examples
#' risk_from_simple_rate(5, 1.5, 1.2, 100, 0.3)
#' risk_from_compound_yield(5, 1.5, 1.2, 100, 0.3)
#' @name yen_bond_risk
NULL

#' @rdname yen_bond_risk
#' @export
risk_from_simple_rate <- function(t, coupon_rate, simple_rate, redemption, accrued_interest, on_error = "stop") {
  x <- yen_inputs(t = t, coupon_rate = coupon_rate, simple_rate = simple_rate, redemption = redemption, accrued_interest = accrued_interest)
  as_tibble(finish_rows(.Call(wrap__risk_from_simple_rate, x$t, x$coupon_rate, x$simple_rate, x$redemption, x$accrued_interest), on_error))
}

#' @rdname yen_bond_risk
#' @export
risk_from_compound_yield <- function(t, coupon_rate, compound_yield, redemption, accrued_interest, coupon_frequency = 2L,
                                     compound_frequency = 2L, on_error = "stop") {
  x <- yen_inputs(t = t, coupon_rate = coupon_rate, compound_yield = compound_yield, redemption = redemption, accrued_interest = accrued_interest)
  as_tibble(finish_rows(.Call(wrap__risk_from_compound_yield, x$t, x$coupon_rate, x$compound_yield, x$redemption, x$accrued_interest,
                              as.integer(coupon_frequency), as.integer(compound_frequency)), on_error))
}
