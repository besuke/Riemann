# Total return swaps through the public functions. Dates against QuantLib's calendars and schedules (when the
# package is present), and the compounding against a direct calculation in R. Synthetic inputs.

ql_date <- function(d) QuantLib::DateParser_parseISO(format(as.Date(d)))
r_date <- function(qd) as.Date(QuantLib::Date_ISO(qd))
ql_advance <- function(cal, d, n) r_date(QuantLib::Calendar_advance(cal, ql_date(d), as.integer(n), "Days"))
ql_adjust <- function(cal, d, convention) r_date(QuantLib::Calendar_adjust(cal, ql_date(d), convention))
ql_schedule <- function(start, end, months, cal, convention, eom = FALSE) {
  s <- QuantLib::Schedule(ql_date(start), ql_date(end), QuantLib::Period(as.integer(months), "Months"), cal, convention, convention,
                          "Backward", eom)
  as.Date(vapply(seq_len(s$size()) - 1L, function(i) QuantLib::Date_ISO(s$date(i)), ""))
}

# The two cases: the demo confirmation (TONA, quarterly, lag 2, lookback 5, fixing 2 days before the period start)
# and a monthly SOFR leg with other choices (end of month, lag 1, lookback 2, fixing 1 day before the period end)
cases <- list(
  jpy = list(start = "2026-10-08", end = "2027-10-08", freq = 4, months = 3, cal = "japan", conv = "modified_following",
             ql_conv = "ModifiedFollowing", lag = 2, index = "tonar", dc = "actual_365_fixed", lookback = 5, reset = "period_start",
             fixing_offset = -2, basis = 365, eom = FALSE),
  usd = list(start = "2026-11-30", end = "2027-05-31", freq = 12, months = 1, cal = "us_sofr", conv = "following",
             ql_conv = "Following", lag = 1, index = "sofr", dc = "actual_360", lookback = 2, reset = "period_end",
             fixing_offset = -1, basis = 360, eom = TRUE)
)
funding_leg <- function(k, spread_bp = 35) {
  create_trs_funding_leg(k$start, k$end, k$freq, k$cal, k$conv, k$cal, k$conv, k$lag, k$index, k$dc, 1e9, spread_bp,
                         lookback = k$lookback, reset_relative_to = k$reset, fixing_offset = k$fixing_offset,
                         fixing_convention = "preceding", end_of_month = k$eom)
}
ql_calendar <- function(k) if (k$index == "tonar") QuantLib::Japan() else QuantLib::Sofr()$fixingCalendar()

test_that("funding leg dates agree with QuantLib's calendars and schedules", {
  skip_if_not_installed("QuantLib")
  for (k in cases) {
    cal <- ql_calendar(k)
    s <- get_trs_funding_schedule(funding_leg(k))
    dates <- ql_schedule(k$start, k$end, k$months, cal, k$ql_conv, k$eom)
    expect_equal(s$accrual_start, head(dates, -1))
    expect_equal(s$accrual_end, dates[-1])
    expect_equal(s$payment_date, as.Date(vapply(s$accrual_end, \(d) format(ql_adjust(cal, ql_advance(cal, d, k$lag), k$ql_conv)), "")))
    reset <- if (k$reset == "period_start") s$accrual_start else s$accrual_end
    expect_equal(s$reset_date, reset)
    expect_equal(s$fixing_date, as.Date(vapply(reset, \(d) format(ql_adjust(cal, ql_advance(cal, d, k$fixing_offset), "Preceding")), "")))
    obs <- get_trs_funding_observations(funding_leg(k))
    expect_equal(obs$fixing_date, as.Date(vapply(obs$day, \(d) format(ql_advance(cal, d, -k$lookback)), "")))
    # the business days of each period are those of QuantLib's calendar, and the weights add up to the period
    for (p in s$period) {
      days <- seq(s$accrual_start[p], s$accrual_end[p] - 1, by = "day")
      days <- days[vapply(days, \(d) QuantLib::Calendar_isBusinessDay(cal, ql_date(d)), TRUE)]
      expect_equal(obs$day[obs$period == p], days)
      expect_equal(sum(obs$day_fraction[obs$period == p]), as.numeric(s$accrual_end[p] - s$accrual_start[p]) / k$basis, tolerance = 1e-14)
    }
  }
})

test_that("the compounded rate with a lookback is the direct product, and the status says how it is known", {
  for (k in cases) {
    leg <- funding_leg(k)
    obs <- get_trs_funding_observations(leg)
    s <- get_trs_funding_schedule(leg)
    # synthetic fixings that differ by date, so a wrong fixing date would show
    fixings <- tibble::tibble(date = sort(unique(obs$fixing_date)))
    fixings$rate <- 0.01 + 0.0001 * as.numeric(fixings$date - fixings$date[1]) / 30
    flows <- compute_trs_funding_cashflows(leg, max(obs$fixing_date) + 1, fixings)
    expect_true(all(flows$status == "fixed"))
    for (p in s$period) {
      o <- obs[obs$period == p, ]
      r <- fixings$rate[match(o$fixing_date, fixings$date)]
      tau <- as.numeric(s$accrual_end[p] - s$accrual_start[p]) / k$basis
      rate <- (prod(1 + r * o$day_fraction) - 1) / tau
      expect_equal(flows$compounded_rate[p], rate, tolerance = 1e-12)
      expect_equal(flows$amount[p], 1e9 * (rate + 0.0035) * s$accrual[p], tolerance = 1e-12)
    }
    # without fixings: missing, with no amount and a reason
    none <- compute_trs_funding_cashflows(leg, max(obs$fixing_date) + 1)
    expect_true(all(none$status == "missing") && all(is.na(none$amount)) && all(grepl("no published fixing", none$reason)))
  }
})

test_that("a forecast curve projects the fixings after the evaluation date", {
  k <- cases$jpy
  leg <- funding_leg(k)
  curve <- bootstrap_ois_curve("2026-10-06", "2026-10-06", "tonar", "actual_365_fixed",
                               create_ois_instruments(c("1Y", "2Y", "3Y"), c(0.008, 0.009, 0.01), 2L, 1L, 0L))
  obs <- get_trs_funding_observations(leg)
  # fixings published up to the day before the evaluation date
  past <- tibble::tibble(date = sort(unique(obs$fixing_date[obs$fixing_date < as.Date("2026-10-06")])), rate = 0.0073)
  flows <- compute_trs_funding_cashflows(leg, "2026-10-06", past, curve)
  expect_equal(flows$status, rep("projected", 4))
  expect_equal(flows$fixed_days[1], nrow(past))
  # the projected fixing is the curve's overnight forward rate to the next business day
  p2 <- obs[obs$period == 2, ]
  nxt <- compute_advanced_date("japan", p2$fixing_date, "1D")
  fwd <- (compute_discount(curve, p2$fixing_date) / compute_discount(curve, nxt) - 1) / (as.numeric(nxt - p2$fixing_date) / 365)
  s <- get_trs_funding_schedule(leg)
  rate <- (prod(1 + fwd * p2$day_fraction) - 1) / (as.numeric(s$accrual_end[2] - s$accrual_start[2]) / 365)
  expect_equal(flows$compounded_rate[2], rate, tolerance = 1e-12)
  # without the earlier fixings the first period cannot be computed
  expect_equal(compute_trs_funding_cashflows(leg, "2026-10-06", NULL, curve)$status[1], "missing")
})

test_that("return leg valuation dates follow their rule, and a stated final date is kept beside the rule's", {
  skip_if_not_installed("QuantLib")
  cal <- QuantLib::Japan()
  by_end <- create_trs_return_leg("2026-10-08", "2027-10-08", 4, "japan", "modified_following", NULL, NULL, NULL, 1e9, 100,
                                  "period_end", -2, "japan", final_valuation_date = "2027-10-06")
  s <- get_trs_return_schedule(by_end)
  expect_equal(s$valuation_date_by_rule, as.Date(vapply(s$period_end, \(d) format(ql_adjust(cal, ql_advance(cal, d, -2), "Preceding")), "")))
  expect_equal(s$valuation_date[4], as.Date("2027-10-06"))
  expect_true(all(is.na(s$payment_date)))
  expect_equal(s$start_valuation_date, c(NA, head(s$valuation_date, -1)))
  by_payment <- create_trs_return_leg("2026-10-08", "2027-10-08", 4, "japan", "modified_following", "japan", "modified_following", 2,
                                      1e9, 100, "payment_date", -2, "japan")
  sp <- get_trs_return_schedule(by_payment)
  expect_equal(sp$valuation_date, as.Date(vapply(sp$payment_date, \(d) format(ql_adjust(cal, ql_advance(cal, d, -2), "Preceding")), "")))
  levels <- tibble::tibble(date = s$valuation_date[1:2], level = c(101, 100.5), kind = c("observed", "scenario"))
  a <- compute_trs_return_cashflows(by_end, levels)
  expect_equal(a$status, c("fixed", "scenario", "missing", "missing"))
  expect_equal(a$amount[1:2], 1e9 * c(101 / 100 - 1, 100.5 / 101 - 1), tolerance = 1e-14)
  expect_true(all(is.na(a$amount[3:4])))
})
