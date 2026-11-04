#' Dates, schedules and day counts
#'
#' The calendar, schedule and day-count rules that the bonds, swaps and CDS of this package use, exposed
#' directly (QuantLib `Calendar::adjust`, `advance` and `endOfMonth`, `Schedule`, `DayCounter::yearFraction`).
#'
#' @param calendar A calendar name; see [is_business_day()].
#' @param date,start,end Dates.
#' @param convention,termination_convention Business-day conventions: `"following"`, `"modified_following"`,
#'   `"preceding"`, `"modified_preceding"`, `"half_month_modified_following"` or `"unadjusted"`.
#' @param period A period such as `"2D"`, `"1W"`, `"3M"` or `"1Y"`. Days are business days.
#' @param end_of_month Keep month ends at month ends.
#' @param effective_date,termination_date,tenor,rule,first_date,next_to_last_date The schedule: `rule` is
#'   `"backward"`, `"forward"`, `"zero"`, `"third_wednesday"`, `"twentieth"`, `"twentieth_imm"`, `"cds"` or
#'   `"cds2015"`; `first_date` and `next_to_last_date` give a long or short first or last period.
#' @param day_count A day count name, such as `"thirty_360_bond_basis"` or `"act_act_isma"`.
#' @param reference_start,reference_end The reference period, used by Actual/Actual (ISMA) and Actual/365
#'   (Canadian) and ignored by the others; `NULL` for none.
#' @param on_error `"stop"` or `"na"`; see [RiemannFinMath-package].
#' @return Dates, a numeric vector of year fractions, or for `generate_schedule()` a tibble of `date` and
#'   `is_regular` (whether the period that starts at the date is regular; `NA` for the last date).
#' @examples
#' compute_adjusted_date("target", as.Date("2023-05-01"), "following")
#' compute_advanced_date("target", as.Date("2023-06-19"), "7D")
#' generate_schedule("2021-05-11", "2025-02-11", "6M", "target", rule = "backward")
#' compute_year_fraction("act_act_isma", "2018-01-01", "2018-01-15", "2018-01-01", "2018-07-01")
#' @export
compute_adjusted_date <- function(calendar, date, convention = "following", on_error = "stop") {
  as_date(finish_values(checked(.Call(wrap__compute_adjusted_date, calendar, as_serial(date), convention)), on_error))
}

#' @rdname compute_adjusted_date
#' @export
compute_advanced_date <- function(calendar, date, period, convention = "following", end_of_month = FALSE, on_error = "stop") {
  as_date(finish_values(checked(.Call(wrap__compute_advanced_date, calendar, as_serial(date), period, convention, as_flag(end_of_month))), on_error))
}

#' @rdname compute_adjusted_date
#' @export
compute_end_of_month <- function(calendar, date, on_error = "stop") {
  as_date(finish_values(checked(.Call(wrap__compute_end_of_month, calendar, as_serial(date))), on_error))
}

#' @rdname compute_adjusted_date
#' @export
generate_schedule <- function(effective_date, termination_date, tenor, calendar, convention = "following",
                              termination_convention = convention, rule = "backward", end_of_month = FALSE,
                              first_date = NULL, next_to_last_date = NULL) {
  r <- checked(.Call(wrap__generate_schedule, as_serial(effective_date), as_serial(termination_date), tenor, calendar, convention,
                     termination_convention, rule, as_flag(end_of_month), as_optional_serial(first_date), as_optional_serial(next_to_last_date)))
  tibble::tibble(date = as_date(r$date), is_regular = c(r$is_regular, NA))
}

#' @rdname compute_adjusted_date
#' @export
compute_year_fraction <- function(day_count, start, end, reference_start = NULL, reference_end = NULL, on_error = "stop") {
  i <- tibble::tibble(start = as_serial(start), end = as_serial(end),
                      reference_start = as_optional_serial(reference_start),
                      reference_end = as_optional_serial(reference_end))
  finish_values(checked(.Call(wrap__compute_year_fraction, day_count, i$start, i$end, i$reference_start, i$reference_end)), on_error)
}

#' NPV and BPS of cash flows on a curve
#'
#' QuantLib `CashFlows::npv` and `CashFlows::bps` on a yield curve. A flow has occurred, and counts 0, if
#' its date is before `settlement_date`, or on it unless `include_settlement_cashflows`. The BPS is the
#' change in value for one basis point on the coupon rates, the sum of `nominal * accrual * discount * 1e-4`;
#' flows without a nominal or an accrual (a redemption, for example) add nothing to it.
#'
#' @param cashflows A data frame with `date` (or `payment_date`) and `amount`, and optionally `nominal` and
#'   `accrual` for the BPS; [get_bond_cashflows()] gives one. A coupon's accrual is computed from
#'   `accrual_start` and `accrual_end` when those columns are there and `accrual` is not, with `day_count`.
#' @param curve A yield curve.
#' @param settlement_date The date before which flows have occurred; the curve's reference date by default.
#' @param include_settlement_cashflows Count a flow on the settlement date.
#' @param npv_date The date of the value; `settlement_date` by default.
#' @param nominal The nominal of the coupons, when `cashflows` has no `nominal` column.
#' @param day_count The day count of the coupons' accrual, used only as described above.
#' @return A one-row tibble with `npv`, `bps` and the list column `flows`: the input with `discount`,
#'   `npv` and `bps` of each flow.
#' @examples
#' bond <- create_fixed_rate_bond(3L, 10000, "2025-04-08", "2030-04-08", "6M", "target", "following", "following",
#'                                "backward", FALSE, 0.03, "thirty_360_bond_basis", "following", 100)
#' curve <- create_flat_curve("2025-07-28", 0.04, "actual_360")
#' compute_cashflow_npv(get_bond_cashflows(bond), curve, nominal = 10000, day_count = "thirty_360_bond_basis")
#' @export
compute_cashflow_npv <- function(cashflows, curve, settlement_date = NULL, include_settlement_cashflows = FALSE, npv_date = NULL,
                                 nominal = NULL, day_count = NULL) {
  flows <- tibble::as_tibble(cashflows)
  has <- function(column) column %in% names(flows)
  date_column <- if (has("date")) "date" else if (has("payment_date")) "payment_date" else NA
  if (is.na(date_column) || !has("amount")) stop("`cashflows` needs `date` (or `payment_date`) and `amount`.", call. = FALSE)
  dates <- flows[[date_column]]
  accrual <- rep(0, nrow(flows))
  if (has("accrual")) {
    accrual <- ifelse(is.na(flows$accrual), 0, flows$accrual)
  } else if (has("accrual_start") && has("accrual_end") && !is.null(day_count)) {
    # only the flows with an accrual period, such as coupons; a redemption has none
    with_period <- !is.na(flows$accrual_start) & !is.na(flows$accrual_end)
    if (any(with_period)) {
      accrual[with_period] <- compute_year_fraction(day_count, flows$accrual_start[with_period], flows$accrual_end[with_period])
    }
  }
  nominals <- if (has("nominal")) ifelse(is.na(flows$nominal), 0, flows$nominal) else rep(if (is.null(nominal)) 0 else nominal, nrow(flows))
  reference <- get_curve_reference_date(curve)
  settlement <- if (is.null(settlement_date)) reference else as.Date(settlement_date)
  at <- if (is.null(npv_date)) settlement else as.Date(npv_date)
  r <- checked(.Call(wrap__compute_cashflow_npv, as_serial(dates), as.numeric(flows$amount), as.numeric(nominals), as.numeric(accrual),
                     curve$pointer, as_serial(settlement), as_flag(include_settlement_cashflows), as_serial(at)))
  flows$discount <- r$discount; flows$npv <- r$flow_npv; flows$bps <- r$flow_bps
  tibble::tibble(npv = r$npv, bps = r$bps, flows = list(flows))
}

#' The reference date of a curve
#'
#' @param curve A yield curve.
#' @return A date.
#' @examples
#' get_curve_reference_date(create_flat_curve("2024-05-02", 0.04, "actual_360"))
#' @export
get_curve_reference_date <- function(curve) as_date(.Call(wrap__get_curve_reference_date, curve$pointer))
