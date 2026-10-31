# Bonds. Every convention is an explicit argument, because QuantLib's defaults are not market
# conventions. Names are those of the Rust functions.

#' A fixed-rate bond. Returns a value that does not change.
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

#' The settlement date of a bond for each evaluation date.
settlement_date <- function(bond, evaluation_date, on_error = "stop") {
  rows <- .Call(wrap__settlement_date, bond$pointer, as_serial(evaluation_date))
  as_date(finish_values(rows, on_error))
}

#' Prices and risk of a bond at each yield and settlement date, one row each. A measure that
#' fails is NA, with its message in `error`. `duration_macaulay` is NA unless the yield is
#' compounded; `duration()` reports that as an error.
bond_values <- function(bond, rate, day_count, compounding, frequency, settlement_date, on_error = "stop") {
  inputs <- tibble(rate = as.numeric(rate), settlement_date = as_serial(settlement_date))
  rows <- checked(.Call(wrap__bond_values, bond$pointer, inputs$rate, day_count, compounding, as.integer(frequency), inputs$settlement_date))
  rows <- finish_rows(rows, on_error)
  tibble(rate = inputs$rate, settlement_date = as_date(inputs$settlement_date), as_tibble(rows))
}

bond_measure <- function(name) {
  function(bond, rate, day_count, compounding, frequency, settlement_date, on_error = "stop") {
    bond_values(bond, rate, day_count, compounding, frequency, settlement_date, on_error)[[name]]
  }
}

clean_price <- bond_measure("clean_price")
dirty_price <- bond_measure("dirty_price")
convexity <- bond_measure("convexity")
basis_point_value <- bond_measure("basis_point_value")
yield_value_basis_point <- bond_measure("yield_value_basis_point")

#' Duration of a bond; `duration_type` is simple, modified or macaulay.
duration <- function(bond, rate, day_count, compounding, frequency, settlement_date, duration_type, on_error = "stop") {
  inputs <- tibble(rate = as.numeric(rate), settlement_date = as_serial(settlement_date))
  rows <- checked(.Call(wrap__duration, bond$pointer, inputs$rate, day_count, compounding, as.integer(frequency), inputs$settlement_date, duration_type))
  finish_values(rows, on_error)
}

#' Accrued amount of a bond at each settlement date.
accrued_amount <- function(bond, settlement_date, on_error = "stop") {
  # the accrued amount does not depend on the yield
  bond_values(bond, 0, "actual_365_fixed", "continuous", 1L, settlement_date, on_error)$accrued_amount
}

#' The yield giving each price. The solver settings default to QuantLib's.
yield_from_price <- function(bond, price, price_type, day_count, compounding, frequency, settlement_date,
                             accuracy = 1e-10, max_iterations = 100L, guess = 0.05, on_error = "stop") {
  inputs <- tibble(price = as.numeric(price), settlement_date = as_serial(settlement_date))
  rows <- checked(.Call(wrap__yield_from_price, bond$pointer, inputs$price, price_type, day_count, compounding, as.integer(frequency),
                        inputs$settlement_date, as.numeric(accuracy), as.integer(max_iterations), as.numeric(guess)))
  finish_values(rows, on_error)
}

#' Value of a bond on an OIS curve at each evaluation date.
value_on_curve <- function(bond, curve, evaluation_date, on_error = "stop") {
  serials <- as_serial(evaluation_date)
  rows <- finish_rows(.Call(wrap__value_on_curve, bond$pointer, curve$pointer, serials), on_error)
  tibble(evaluation_date = as_date(serials), as_tibble(rows))
}

# ---- Japanese bond calculations; rules in docs/spec_bonds.md, chapter 5 ----------------------

#' Remaining years between two dates by the Japanese rule.
remaining_years <- function(start, end, on_error = "stop") {
  inputs <- tibble(start = as_serial(start), end = as_serial(end))
  finish_values(.Call(wrap__remaining_years, inputs$start, inputs$end), on_error)
}

yen_inputs <- function(...) {
  # recycles the columns to one length
  as.list(as_tibble(lapply(list(...), as.numeric)))
}

#' Price from a simple yield; `digits = NA` means no truncation.
price_from_simple_rate <- function(t, coupon_rate, simple_rate, redemption, digits = 3L, on_error = "stop") {
  x <- yen_inputs(t = t, coupon_rate = coupon_rate, simple_rate = simple_rate, redemption = redemption)
  finish_values(.Call(wrap__price_from_simple_rate, x$t, x$coupon_rate, x$simple_rate, x$redemption, as.integer(digits)), on_error)
}

#' Simple yield from a price.
simple_rate_from_price <- function(t, coupon_rate, price, redemption, digits = 7L, max_iterations = 100L, on_error = "stop") {
  x <- yen_inputs(t = t, coupon_rate = coupon_rate, price = price, redemption = redemption)
  finish_values(.Call(wrap__simple_rate_from_price, x$t, x$coupon_rate, x$price, x$redemption, as.integer(digits), as.integer(max_iterations)), on_error)
}

#' Price from a compound yield; `digits = NA` means no truncation.
price_from_compound_yield <- function(t, coupon_rate, compound_yield, redemption, coupon_frequency = 2L,
                                      compound_frequency = 2L, digits = 3L, on_error = "stop") {
  x <- yen_inputs(t = t, coupon_rate = coupon_rate, compound_yield = compound_yield, redemption = redemption)
  finish_values(.Call(wrap__price_from_compound_yield, x$t, x$coupon_rate, x$compound_yield, x$redemption,
                      as.integer(coupon_frequency), as.integer(compound_frequency), as.integer(digits)), on_error)
}

#' Compound yield from a price.
compound_yield_from_price <- function(t, coupon_rate, price, redemption, coupon_frequency = 2L, compound_frequency = 2L,
                                      digits = 7L, max_iterations = 100L, on_error = "stop") {
  x <- yen_inputs(t = t, coupon_rate = coupon_rate, price = price, redemption = redemption)
  finish_values(.Call(wrap__compound_yield_from_price, x$t, x$coupon_rate, x$price, x$redemption, as.integer(coupon_frequency),
                      as.integer(compound_frequency), as.integer(digits), as.integer(max_iterations)), on_error)
}

#' Risk measures from a simple yield, one row each.
risk_from_simple_rate <- function(t, coupon_rate, simple_rate, redemption, accrued_interest, on_error = "stop") {
  x <- yen_inputs(t = t, coupon_rate = coupon_rate, simple_rate = simple_rate, redemption = redemption, accrued_interest = accrued_interest)
  as_tibble(finish_rows(.Call(wrap__risk_from_simple_rate, x$t, x$coupon_rate, x$simple_rate, x$redemption, x$accrued_interest), on_error))
}

#' Risk measures from a compound yield, one row each.
risk_from_compound_yield <- function(t, coupon_rate, compound_yield, redemption, accrued_interest, coupon_frequency = 2L,
                                     compound_frequency = 2L, on_error = "stop") {
  x <- yen_inputs(t = t, coupon_rate = coupon_rate, compound_yield = compound_yield, redemption = redemption, accrued_interest = accrued_interest)
  as_tibble(finish_rows(.Call(wrap__risk_from_compound_yield, x$t, x$coupon_rate, x$compound_yield, x$redemption, x$accrued_interest,
                              as.integer(coupon_frequency), as.integer(compound_frequency)), on_error))
}
