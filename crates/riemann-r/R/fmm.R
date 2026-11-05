# Backward-looking RFR rates in the generalized forward market model (FMM). Calculations are in Rust.

#' Caplets on backward-looking compounded rates (FMM)
#'
#' The generalized forward market model of Lyashenko and Mercurio (LagrangeFinance II-7.5). The rate of a
#' period is the overnight rate compounded over it, known only at the period end.
#'
#' `compute_backward_forward_rate()` gives its forward: `(realized * discount_start / discount_end - 1) /
#' accrual`. `realized` is the compounding factor of the fixings already known (1 before the period
#' starts), `discount_start` the discount factor to the later of the evaluation date and the period start,
#' and `discount_end` the one to the period end.
#'
#' `compute_backward_caplet_value()` values a caplet paying `accrual * (R - strike)^+` at the period end.
#' The rate's volatility holds until the period start and falls linearly to 0 at its end, so the formula
#' of [compute_caplet_value()] applies with the variance time `time_to_start + length / 3` before the period
#' and `time_to_end^3 / (3 length^2)` during it. With a period of length 0 it is the forward-looking caplet.
#'
#' @param realized Compounding factor of the fixings already known.
#' @param discount_start,discount_end Discount factors as described above.
#' @param accrual Year fraction of the period.
#' @param forward The forward from `compute_backward_forward_rate()`.
#' @param strike,volatility,discount_factor,notional,model As in [compute_caplet_value()].
#' @param time_to_start Years to the period start; negative during the period.
#' @param time_to_end Years to the period end; positive.
#' @param on_error `"stop"` or `"na"`; see [RiemannFinMath-package].
#' @return Numeric vectors.
#' @examples
#' f <- compute_backward_forward_rate(1, 0.990, 0.985, 0.25)
#' compute_backward_caplet_value(f, 0.021, 0.3, time_to_start = 0.75, time_to_end = 1, accrual = 0.25, discount_factor = 0.985)
#' # a month into the period, with the fixings so far compounding to 1.0017
#' f <- compute_backward_forward_rate(1.0017, 1, 0.9965, 0.25)
#' compute_backward_caplet_value(f, 0.021, 0.3, time_to_start = -1 / 12, time_to_end = 1 / 6, accrual = 0.25, discount_factor = 0.9965)
#' @export
compute_backward_forward_rate <- function(realized, discount_start, discount_end, accrual, on_error = "stop") {
  i <- tibble(realized = as.numeric(realized), discount_start = as.numeric(discount_start), discount_end = as.numeric(discount_end),
              accrual = as.numeric(accrual))
  finish_values(.Call(wrap__compute_backward_forward_rate, i$realized, i$discount_start, i$discount_end, i$accrual), on_error)
}

#' @rdname compute_backward_forward_rate
#' @export
compute_backward_caplet_value <- function(forward, strike, volatility, time_to_start, time_to_end, accrual, discount_factor, notional = 1,
                                          model = "black", on_error = "stop") {
  i <- tibble(forward = as.numeric(forward), strike = as.numeric(strike), volatility = as.numeric(volatility),
              time_to_start = as.numeric(time_to_start), time_to_end = as.numeric(time_to_end), accrual = as.numeric(accrual),
              discount_factor = as.numeric(discount_factor), notional = as.numeric(notional))
  finish_values(checked(.Call(wrap__compute_backward_caplet_value, as.character(model), i$forward, i$strike, i$volatility, i$time_to_start,
                              i$time_to_end, i$accrual, i$discount_factor, i$notional)), on_error)
}
