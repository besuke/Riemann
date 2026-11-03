# Overnight indexed swaps through the public functions, against QuantLib where the package is present.
# Synthetic inputs.

swap_curve <- function(quotes = c(0.008, 0.01, 0.012, 0.014), interpolation = "log_linear") {
  bootstrap_ois_curve("2026-11-02", "2026-11-02", "tonar", "actual_365_fixed",
                  create_ois_instruments(c("1Y", "3Y", "5Y", "7Y"), quotes, 0L, 1L, 0L), interpolation = interpolation)
}
payer_swap <- function(rate = 0.012) create_ois_swap("payer", 1e7, "2026-11-04", "2031-11-04", "tonar", rate)

test_that("the sides are opposite, and the fair rate gives a value of 0", {
  curve <- swap_curve()
  v <- compute_ois_swap_value(payer_swap(), curve, "2026-11-02")
  r <- compute_ois_swap_value(create_ois_swap("receiver", 1e7, "2026-11-04", "2031-11-04", "tonar", 0.012), curve, "2026-11-02")
  expect_equal(v$npv, -r$npv, tolerance = 1e-10)
  expect_equal(v$npv, v$fixed_leg_npv + v$overnight_leg_npv, tolerance = 1e-12)
  expect_lt(abs(compute_ois_swap_value(payer_swap(v$fair_rate), curve, "2026-11-02")$npv), 1e-7)
  expect_s3_class(payer_swap(), "riemann_ois_swap")
})

test_that("cash flows list both legs", {
  flows <- compute_ois_swap_cashflows(payer_swap(), swap_curve(), "2026-11-02")
  expect_equal(sum(flows$leg == "fixed"), 5)
  expect_equal(sum(flows$leg == "overnight"), 5)
  expect_true(all(is.na(flows$error)))
  expect_equal(flows$amount[1], 1e7 * 0.012 * as.numeric(flows$accrual_end[1] - flows$accrual_start[1]) / 365, tolerance = 1e-12)
})

test_that("fixings before the evaluation date are required, and paid coupons need none", {
  curve <- swap_curve()
  expect_error(compute_ois_swap_value(payer_swap(), curve, "2026-11-06"), "Missing")
  flows <- compute_ois_swap_cashflows(payer_swap(), curve, "2026-11-06", on_error = "na")
  expect_true(any(!is.na(flows$error)))
  expect_true(all(is.na(flows$amount[!is.na(flows$error)])))
  expect_error(compute_ois_swap_cashflows(payer_swap(), curve, "2026-11-06"), "Missing")
  fixings <- tibble::tibble(date = as.Date(c("2026-11-04", "2026-11-05")), rate = c(0.05, 0.05))
  with_fixings <- compute_ois_swap_value(payer_swap(), curve, "2026-11-06", fixings)
  without_past_rates <- compute_ois_swap_value(payer_swap(), curve, "2026-11-02")
  expect_gt(with_fixings$overnight_leg_npv, without_past_rates$overnight_leg_npv)
  days <- compute_ois_swap_daily_rates(payer_swap(), curve, "2026-11-06", fixings)
  expect_equal(days$applied_rate[1:2], c(0.05, 0.05))
  expect_equal(days$applied_rate[3], days$forward_rate[3])
  expect_error(compute_ois_swap_daily_rates(payer_swap(), curve, "2026-11-02", coupon = 99L), "outside")
  expect_error(compute_ois_swap_value(payer_swap(), curve, "2026-11-06", tibble::tibble(date = "2026-11-04")), "lacks")
  expect_error(compute_ois_swap_value(payer_swap(), curve, "2026-11-06", tibble::tibble(date = c("2026-11-04", "2026-11-04"), rate = c(0.01, 0.01))), "twice")
})

test_that("the forecasting curve must be of the swap's index, and bad terms fail", {
  sofr <- bootstrap_ois_curve("2026-11-02", "2026-11-02", "sofr", "actual_360", create_ois_instruments(c("1Y", "5Y", "10Y"), c(0.04, 0.038, 0.036), 2L, 1L, 0L))
  expect_error(compute_ois_swap_value(payer_swap(), sofr, "2026-11-02"), "index")
  expect_silent(compute_ois_swap_value(payer_swap(), swap_curve(), "2026-11-02", discount_curve = sofr))
  expect_error(create_ois_swap("broker", 1e7, "2026-11-04", "2031-11-04", "tonar", 0.01), "swap side")
  expect_error(create_ois_swap("payer", 1e7, "2031-11-04", "2026-11-04", "tonar", 0.01), "termination")
  expect_error(create_ois_swap("payer", 1e7, "2026-11-04", "2031-11-04", "tonar", 0.01, fixed_frequency = 3L), "frequency")
})

test_that("quote sensitivities agree with rebuilding the curve", {
  q <- c(0.008, 0.01, 0.012, 0.014)
  s <- compute_ois_swap_quote_sensitivity(payer_swap(), swap_curve(q), "2026-11-02")
  expect_equal(s$tenor, c("1Y", "3Y", "5Y", "7Y"))
  for (i in seq_along(q)) {
    up <- q; up[i] <- up[i] + 1e-6
    down <- q; down[i] <- down[i] - 1e-6
    numeric <- (compute_ois_swap_value(payer_swap(), swap_curve(up), "2026-11-02")$npv - compute_ois_swap_value(payer_swap(), swap_curve(down), "2026-11-02")$npv) / 2e-6 * 1e-4
    expect_lt(abs(s$npv_per_bp[i] - numeric), 1e-3)
  }
})

test_that("values agree with the QuantLib R package", {
  skip_if_not_installed("QuantLib")
  date_of <- function(x) QuantLib::Date(as.integer(as.Date(x)) + 25569L)
  curve <- swap_curve()
  nodes <- get_discount_nodes(curve)
  QuantLib::Settings_instance()$setEvaluationDate(date_of("2026-11-02"))
  dates <- do.call(QuantLib::DateVector, list())
  for (d in as.character(nodes$date)) QuantLib::DateVector_push_back(dates, date_of(d))
  ql_curve <- QuantLib::DiscountCurve(dates, nodes$discount, QuantLib::Actual365Fixed())
  QuantLib::TermStructure_enableExtrapolation(ql_curve)
  index <- QuantLib::OvernightIndex("TONA", 0L, QuantLib::JPYCurrency(), QuantLib::Japan(), QuantLib::Actual365Fixed(), QuantLib::YieldTermStructureHandle(ql_curve))
  builder <- QuantLib::MakeOIS(swapTenor = QuantLib::Period(1826L, "Days"), overnightIndex = index, fixedRate = 0.012)
  builder <- QuantLib::MakeOIS_withEffectiveDate(builder, date_of("2026-11-04"))
  builder <- QuantLib::MakeOIS_withTerminationDate(builder, date_of("2031-11-04"))
  builder <- QuantLib::MakeOIS_withNominal(builder, 1e7)
  builder <- QuantLib::MakeOIS_withDiscountingTermStructure(builder, QuantLib::YieldTermStructureHandle(ql_curve))
  quantlib <- QuantLib::MakeOIS_makeOIS(builder)
  ours <- compute_ois_swap_value(payer_swap(), curve, "2026-11-02")
  expect_equal(ours$npv, quantlib$NPV(), tolerance = 1e-10)
  expect_equal(ours$fair_rate, QuantLib::FixedVsFloatingSwap_fairRate(quantlib), tolerance = 1e-10)
})
