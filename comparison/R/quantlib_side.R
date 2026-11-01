# Builds a QuantLib OIS curve from a table of tenors and rates. "ON" is an overnight deposit.
ql_period <- function(tenor) {
  unit <- c(D = "Days", W = "Weeks", M = "Months", Y = "Years")[[str_sub(tenor, -1)]]
  QuantLib::Period(as.integer(str_sub(tenor, 1, -2)), unit)
}

ql_quote <- function(rate) QuantLib::QuoteHandle(QuantLib::SimpleQuote(rate))

ql_overnight_index <- function(index) {
  switch(index,
    tonar = QuantLib::OvernightIndex("TONA", 0L, QuantLib::JPYCurrency(), QuantLib::Japan(), QuantLib::Actual365Fixed()),
    estr = QuantLib::Estr(),
    sofr = QuantLib::Sofr(),
    sonia = QuantLib::Sonia()
  )
}

ql_ois_curve <- function(date, index, day_count, tenor, rate, settlement_days, frequency = "Annual", payment_lag = 0L,
                         payment_convention = "Following", interpolation = "monotonic_log_cubic") {
  ql_set_evaluation_date(date)
  overnight <- ql_overnight_index(index)
  frequency_value <- switch(frequency, Annual = QuantLib::Frequency_Annual_get(), Semiannual = QuantLib::Frequency_Semiannual_get())
  helpers <- map2(tenor, rate, \(t, r) {
    if (t == "ON") {
      QuantLib::DepositRateHelper(ql_quote(r), overnight)
    } else {
      QuantLib::OISRateHelper(
        settlement_days, ql_period(t), ql_quote(r), overnight, QuantLib::YieldTermStructureHandle(),
        FALSE, payment_lag, payment_convention, frequency_value
      )
    }
  })
  vector <- QuantLib::RateHelperVector()
  walk(helpers, \(h) QuantLib::RateHelperVector_append(vector, h))
  constructor <- switch(interpolation,
    monotonic_log_cubic = QuantLib::PiecewiseLogCubicDiscount,
    log_linear = QuantLib::PiecewiseLogLinearDiscount
  )
  curve <- constructor(ql_date(date), vector, day_count)
  QuantLib::TermStructure_enableExtrapolation(curve)
  list(curve = curve, helpers = helpers, day_count = day_count)
}

ql_discount <- function(curve, dates) {
  map_dbl(as.Date(dates), \(d) QuantLib::YieldTermStructure_discount(curve$curve, ql_date(d)))
}

ql_zero_percent <- function(curve, dates) {
  map_dbl(as.Date(dates), \(d) {
    100 * QuantLib::InterestRate_rate(QuantLib::YieldTermStructure_zeroRate(curve$curve, ql_date(d), curve$day_count, "Continuous", "Annual"))
  })
}

ql_forward_percent <- function(curve, dates) {
  map_dbl(as.Date(dates), \(d) {
    100 * QuantLib::InterestRate_rate(QuantLib::YieldTermStructure_forwardRate(curve$curve, ql_date(d), ql_date(d + 1L), curve$day_count, "Simple", "Annual"))
  })
}

ql_nodes <- function(curve) {
  dates <- curve$curve$dates() |> ql_date_vector() |> map(ql_iso) |> list_c()
  tibble(date = dates, discount = as.numeric(curve$curve$data()))
}

ql_pillars <- function(curve) map(curve$helpers, \(h) ql_iso(h$pillarDate())) |> list_c()

ql_implied_percent <- function(curve) map_dbl(curve$helpers, \(h) 100 * h$impliedQuote())

ql_fixed_bond <- function(issue, maturity, coupon, calendar, day_count, settlement_days = 2L, payment_convention = "Following") {
  schedule <- QuantLib::Schedule(ql_date(issue), ql_date(maturity), QuantLib::Period(6L, "Months"), calendar, "Unadjusted", "Unadjusted", "Backward", FALSE)
  QuantLib::FixedRateBond(settlement_days, 100, schedule, coupon, day_count, payment_convention, 100, ql_date(issue))
}
