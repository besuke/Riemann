# Cash flows and zero spreads through the public functions, against QuantLib where the package is present.
# Synthetic inputs.

spread_bond <- function() {
  fixed_rate_bond(
    settlement_days = 2L, face_amount = 100, effective_date = "2024-03-20", termination_date = "2031-03-20",
    tenor = "6M", calendar = "japan", convention = "unadjusted", termination_convention = "unadjusted", rule = "backward",
    end_of_month = FALSE, coupon_rate = 0.015, day_count = "actual_365_fixed", payment_convention = "following",
    redemption = 100, issue_date = "2024-03-20"
  )
}
spread_curve <- function() {
  build_ois_curve(
    "2026-11-02", "2026-11-02", "tonar", "actual_365_fixed",
    ois_instruments(c("ON", "1Y", "2Y", "5Y", "10Y"), c(0.002, 0.004, 0.005, 0.009, 0.014), 2L, 1L, 0L)
  )
}

test_that("the cash flows have coupons and one redemption, and a zero spread of 0 changes nothing", {
  flows <- bond_cashflows(spread_bond())
  expect_equal(nrow(flows), 15)
  expect_equal(flows$kind, c(rep("coupon", 14), "redemption"))
  expect_false(is.unsorted(flows$payment_date))
  expect_equal(flows$amount[15], 100)
  expect_true(is.na(flows$accrual_start[15]) && is.na(flows$accrual_end[15]))
  expect_s3_class(flows$payment_date, "Date")
  expect_equal(RiemannFinMath::value_with_zero_spread(spread_bond(), spread_curve(), 0, "continuous", 1L, "2026-11-02")$npv,
               value_on_curve(spread_bond(), spread_curve(), "2026-11-02")$npv, tolerance = 1e-12)
})

test_that("a higher spread lowers the value, and rows fail alone", {
  v <- value_with_zero_spread(spread_bond(), spread_curve(), c(0, 0.005, 0.01), "compounded", 2L, "2026-11-02")
  expect_true(all(diff(v$dirty_price) < 0))
  expect_equal(v$spread, c(0, 0.005, 0.01))
  expect_error(value_with_zero_spread(spread_bond(), spread_curve(), 0.01, "compounded", 0L, "2026-11-02"))
  expect_error(value_with_zero_spread(spread_bond(), spread_curve(), 0.01, "weekly", 2L, "2026-11-02"))
})

test_that("the spread of a price reprices that price", {
  settlement <- settlement_date(spread_bond(), "2026-11-02")
  for (price_type in c("clean", "dirty")) {
    z <- z_spread_from_price(spread_bond(), spread_curve(), 99, price_type, "compounded", 2L, settlement, accuracy = 1e-12)
    v <- value_with_zero_spread(spread_bond(), spread_curve(), z, "compounded", 2L, "2026-11-02")
    expect_equal(if (price_type == "clean") v$clean_price else v$dirty_price, 99, tolerance = 1e-8)
  }
})

test_that("the bond past the curve's last date needs extrapolation, with the spread as well", {
  long_bond <- fixed_rate_bond(
    settlement_days = 2L, face_amount = 100, effective_date = "2024-03-20", termination_date = "2044-03-20",
    tenor = "6M", calendar = "japan", convention = "unadjusted", termination_convention = "unadjusted", rule = "backward",
    end_of_month = FALSE, coupon_rate = 0.015, day_count = "actual_365_fixed", payment_convention = "following",
    redemption = 100, issue_date = "2024-03-20"
  )
  expect_error(value_with_zero_spread(long_bond, spread_curve(), 0.005, "compounded", 2L, "2026-11-02"), "curve")
  v <- value_with_zero_spread(long_bond, with_extrapolation(spread_curve()), 0.005, "compounded", 2L, "2026-11-02")
  expect_gt(v$npv, 0)
})

test_that("values and spreads agree with the QuantLib R package", {
  skip_if_not_installed("QuantLib")
  date_of <- function(x) QuantLib::Date(as.integer(as.Date(x)) + 25569L)
  QuantLib::Settings_instance()$setEvaluationDate(date_of("2026-11-02"))
  schedule <- QuantLib::Schedule(date_of("2024-03-20"), date_of("2031-03-20"), QuantLib::Period(6L, "Months"), QuantLib::Japan(), "Unadjusted", "Unadjusted", "Backward", FALSE)
  dc <- QuantLib::Actual365Fixed()
  bond <- QuantLib::FixedRateBond(2L, 100, schedule, 0.015, dc, "Following", 100, date_of("2024-03-20"))
  # the same nodes on a log-linear curve
  ours <- build_ois_curve(
    "2026-11-02", "2026-11-02", "tonar", "actual_365_fixed",
    ois_instruments(c("ON", "1Y", "2Y", "5Y", "10Y"), c(0.002, 0.004, 0.005, 0.009, 0.014), 2L, 1L, 0L),
    interpolation = "log_linear"
  )
  nodes <- discount_nodes(ours)
  dates <- do.call(QuantLib::DateVector, list())
  for (d in as.character(nodes$date)) QuantLib::DateVector_push_back(dates, date_of(d))
  curve <- QuantLib::DiscountCurve(dates, nodes$discount, dc)
  QuantLib::TermStructure_enableExtrapolation(curve)
  handle <- QuantLib::YieldTermStructureHandle(curve)
  spreaded <- QuantLib::ZeroSpreadedTermStructure(handle, QuantLib::QuoteHandle(QuantLib::SimpleQuote(0.005)), "Compounded", "Semiannual", dc)
  QuantLib::Instrument_setPricingEngine(bond, QuantLib::DiscountingBondEngine(QuantLib::YieldTermStructureHandle(spreaded)))
  v <- value_with_zero_spread(spread_bond(), ours, 0.005, "compounded", 2L, "2026-11-02")
  expect_equal(v$npv, QuantLib::Instrument_NPV(bond), tolerance = 1e-10)
  expect_equal(v$clean_price, QuantLib::Bond_cleanPrice(bond), tolerance = 1e-10)
  settlement <- QuantLib::Bond_settlementDate(bond)
  quantlib_z <- QuantLib::BondFunctions_zSpread(bond, QuantLib::BondPrice(99, "Clean"), curve, dc, "Compounded", "Semiannual", settlement, 1e-12, 1000L, 0)
  ours_z <- z_spread_from_price(spread_bond(), ours, 99, "clean", "compounded", 2L, settlement_date(spread_bond(), "2026-11-02"), accuracy = 1e-12, max_evaluations = 1000L)
  expect_equal(ours_z, quantlib_z, tolerance = 1e-8)
})
