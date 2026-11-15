# Total return swaps: the dates and amounts of the funding leg and of the return leg. Calculations are in Rust
# (crates/riemann/src/trs.rs); fixings, curves and index levels are arguments, and no state is kept.

trs_payment_lag <- function(payment_lag) if (is.null(payment_lag)) -1L else as.integer(payment_lag)

#' Funding leg of a total return swap
#'
#' An overnight index compounded over each calculation period with a lookback and no observation shift,
#' plus a spread. Each business day of a period (index calendar) carries the fixing of the business day
#' `lookback` days before it, weighted by the index's year fraction to the next business day; the
#' compounded rate is `(prod(1 + r * tau) - 1) / tau(start, end)` and the amount is
#' `notional * (rate + spread) * accrual` with the leg's day count.
#'
#' The calculation periods run from the effective to the termination date, `frequency` a year, and are
#' adjusted with `period_convention` on `calendar`. Payment dates are `payment_lag` business days of
#' `payment_calendar` after each period end, adjusted with `payment_convention`; `payment_lag = NULL`
#' means the confirmation does not define them, and they are `NA`. Reset dates are the period start (or
#' end); fixing dates are `fixing_offset` business days of `fixing_calendar` from the reset date, adjusted
#' with `fixing_convention`. Reset and fixing dates are reported as the confirmation states them; the
#' compounded rate uses the daily observation dates (`get_trs_funding_observations()`).
#'
#' @param effective_date,termination_date Dates: `Date` or ISO strings.
#' @param frequency Periods a year: 1, 2, 4 or 12.
#' @param calendar,payment_calendar,fixing_calendar Calendars (e.g. `"japan"`).
#' @param period_convention,payment_convention,fixing_convention Business day conventions.
#' @param rule Date generation rule of the periods.
#' @param end_of_month End of month rolling of the periods.
#' @param payment_lag Business days from a period end to its payment, or `NULL`.
#' @param index `"tonar"`, `"estr"`, `"sofr"` or `"sonia"`.
#' @param day_count Day count of the leg's accrual.
#' @param notional The notional.
#' @param spread_bp Spread added to the compounded rate, in basis points.
#' @param lookback Business days of the lookback.
#' @param reset_relative_to `"period_start"` or `"period_end"`.
#' @param fixing_offset Business days from the reset date to the fixing date (negative: before).
#' @return A `riemann_trs_funding_leg` value, which does not change.
#' @examples
#' leg <- create_trs_funding_leg("2026-10-08", "2027-10-08", 4, "japan", "modified_following", "japan",
#'   "modified_following", 2, "tonar", "actual_365_fixed", 1e9, 35, lookback = 5, fixing_offset = -2)
#' get_trs_funding_schedule(leg)
#' @export
create_trs_funding_leg <- function(effective_date, termination_date, frequency, calendar, period_convention,
                                   payment_calendar, payment_convention, payment_lag, index, day_count, notional,
                                   spread_bp = 0, lookback = 0L, reset_relative_to = "period_start", fixing_offset = 0L,
                                   fixing_calendar = calendar, fixing_convention = "preceding", rule = "backward",
                                   end_of_month = FALSE) {
  pointer <- checked(.Call(
    wrap__create_trs_funding_leg, as_serial(effective_date), as_serial(termination_date), as.integer(frequency), calendar,
    period_convention, rule, as_flag(end_of_month), payment_calendar, payment_convention, trs_payment_lag(payment_lag),
    index, day_count, as.numeric(notional), as.numeric(spread_bp) / 1e4, as.integer(lookback), reset_relative_to,
    as.integer(fixing_offset), fixing_calendar, fixing_convention
  ))$value
  structure(list(pointer = pointer, index = index, notional = notional, spread_bp = spread_bp), class = "riemann_trs_funding_leg")
}

#' @export
print.riemann_trs_funding_leg <- function(x, ...) {
  cat("<riemann_trs_funding_leg>", x$index, "+", x$spread_bp, "bp on", format(x$notional, big.mark = ","), "\n")
  invisible(x)
}

optional_dates <- function(serial) {
  out <- as_date(serial)
  out[serial == 0L] <- NA
  out
}

#' @rdname create_trs_funding_leg
#' @param leg A leg from `create_trs_funding_leg()` or `create_trs_return_leg()`.
#' @return `get_trs_funding_schedule()`: one row a period with its unadjusted and adjusted dates, reset,
#'   fixing, observation and payment dates, the accrual and the number of business days.
#' @export
get_trs_funding_schedule <- function(leg) {
  r <- .Call(wrap__get_trs_funding_schedule, leg$pointer)
  tibble(period = seq_along(r$accrual_start),
         unadjusted_start = as_date(r$unadjusted_start), unadjusted_end = as_date(r$unadjusted_end),
         accrual_start = as_date(r$accrual_start), accrual_end = as_date(r$accrual_end),
         reset_date = as_date(r$reset_date), fixing_date = as_date(r$fixing_date),
         observation_start = as_date(r$observation_start), observation_end = as_date(r$observation_end),
         payment_date = optional_dates(r$payment_date), accrual = r$accrual, business_days = r$business_days)
}

#' @rdname create_trs_funding_leg
#' @return `get_trs_funding_observations()`: one row a business day of each period, with the fixing date it
#'   uses and its weight.
#' @export
get_trs_funding_observations <- function(leg) {
  r <- .Call(wrap__get_trs_funding_observations, leg$pointer)
  tibble(period = r$period, day = as_date(r$day), fixing_date = as_date(r$fixing_date), day_fraction = r$day_fraction)
}

#' Amounts of the legs of a total return swap
#'
#' Funding leg: a fixing date before `evaluation_date` needs its published fixing; on the evaluation date
#' the fixing is used when given; later fixing dates use the overnight forward rate of `curve` (the
#' discount factors to the next business day of the index) when a curve is given. `status` is `"fixed"`
#' (published fixings only), `"projected"` (with the curve) or `"missing"`, which has no amount and gives
#' the reason.
#'
#' Return leg: `notional * (level_end / level_start - 1)` between valuation dates, the first period
#' starting at the initial price. `levels` has the columns `date`, `level` and `kind` (`"observed"` or
#' `"scenario"`); `status` is `"fixed"`, `"scenario"` or `"missing"`.
#'
#' @param leg A leg.
#' @param evaluation_date The date up to which fixings are published.
#' @param fixings A table with `date` and `rate` (decimal), or `NULL`.
#' @param curve An OIS curve of the leg's index for the forecast, or `NULL`.
#' @param levels A table of levels of the reference asset, or `NULL`.
#' @return A tibble with one row a period.
#' @export
compute_trs_funding_cashflows <- function(leg, evaluation_date, fixings = NULL, curve = NULL) {
  f <- fixing_columns(fixings)
  r <- checked(.Call(wrap__compute_trs_funding_cashflows, leg$pointer, if (is.null(curve)) NULL else curve$pointer,
                     as_serial(evaluation_date), f$dates, f$rates))
  tibble(period = seq_along(r$status), status = r$status, compounded_rate = r$compounded_rate, amount = r$amount,
         fixed_days = r$fixed_days, projected_days = r$projected_days, days = r$days,
         reason = ifelse(nzchar(r$reason), r$reason, NA_character_))
}

#' Return leg of a total return swap
#'
#' Calculation periods as for the funding leg. Valuation dates are `valuation_offset` business days of
#' `valuation_calendar` from each payment date (`valuation_relative_to = "payment_date"`) or period end
#' (`"period_end"`), adjusted with `valuation_convention`. A stated `final_valuation_date` replaces the
#' last one; `valuation_date_by_rule` keeps the date given by the rule for comparison.
#'
#' @inheritParams create_trs_funding_leg
#' @param initial_price The level at the start of the first period.
#' @param valuation_relative_to `"payment_date"` or `"period_end"`.
#' @param valuation_offset Business days from the payment date or period end (negative: before).
#' @param valuation_calendar,valuation_convention Calendar and convention of the valuation dates.
#' @param final_valuation_date A stated final valuation date, or `NULL`.
#' @return A `riemann_trs_return_leg` value.
#' @examples
#' leg <- create_trs_return_leg("2026-10-08", "2027-10-08", 4, "japan", "modified_following", NULL, NULL, NULL,
#'   1e9, 100, "period_end", -2, "japan", final_valuation_date = "2027-10-06")
#' get_trs_return_schedule(leg)
#' @export
create_trs_return_leg <- function(effective_date, termination_date, frequency, calendar, period_convention,
                                  payment_calendar, payment_convention, payment_lag, notional, initial_price,
                                  valuation_relative_to, valuation_offset, valuation_calendar = calendar,
                                  valuation_convention = "preceding", final_valuation_date = NULL, rule = "backward",
                                  end_of_month = FALSE) {
  pointer <- checked(.Call(
    wrap__create_trs_return_leg, as_serial(effective_date), as_serial(termination_date), as.integer(frequency), calendar,
    period_convention, rule, as_flag(end_of_month), payment_calendar %||% "", payment_convention %||% "",
    trs_payment_lag(payment_lag), as.numeric(notional), as.numeric(initial_price), valuation_relative_to,
    as.integer(valuation_offset), valuation_calendar, valuation_convention,
    if (is.null(final_valuation_date)) 0L else as_serial(final_valuation_date)
  ))$value
  structure(list(pointer = pointer, notional = notional, initial_price = initial_price), class = "riemann_trs_return_leg")
}

#' @export
print.riemann_trs_return_leg <- function(x, ...) {
  cat("<riemann_trs_return_leg> initial price", x$initial_price, "on", format(x$notional, big.mark = ","), "\n")
  invisible(x)
}

#' @rdname create_trs_return_leg
#' @param leg A leg from `create_trs_return_leg()`.
#' @return `get_trs_return_schedule()`: one row a period with its dates, the valuation date at its start
#'   (`NA` for the first, which starts at the initial price), its valuation date and payment date.
#' @export
get_trs_return_schedule <- function(leg) {
  r <- .Call(wrap__get_trs_return_schedule, leg$pointer)
  tibble(period = seq_along(r$period_start),
         unadjusted_start = as_date(r$unadjusted_start), unadjusted_end = as_date(r$unadjusted_end),
         period_start = as_date(r$period_start), period_end = as_date(r$period_end),
         start_valuation_date = optional_dates(r$start_valuation_date), valuation_date = as_date(r$valuation_date),
         valuation_date_by_rule = as_date(r$valuation_date_by_rule), payment_date = optional_dates(r$payment_date))
}

#' @rdname compute_trs_funding_cashflows
#' @export
compute_trs_return_cashflows <- function(leg, levels = NULL) {
  if (is.null(levels)) levels <- tibble(date = as.Date(character()), level = numeric(), kind = character())
  missing_columns <- setdiff(c("date", "level", "kind"), names(levels))
  if (length(missing_columns)) stop("`levels` lacks the columns: ", paste(missing_columns, collapse = ", "), call. = FALSE)
  if (anyNA(levels$level)) stop("`levels` has a missing level.", call. = FALSE)
  r <- checked(.Call(wrap__compute_trs_return_cashflows, leg$pointer, as_serial(levels$date), as.numeric(levels$level),
                     as.character(levels$kind)))
  tibble(period = seq_along(r$status), status = r$status, start_level = r$start_level, end_level = r$end_level,
         period_return = r$period_return, amount = r$amount, reason = ifelse(nzchar(r$reason), r$reason, NA_character_))
}
