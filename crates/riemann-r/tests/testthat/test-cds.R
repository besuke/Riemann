# Credit default swaps through the public functions. Synthetic inputs.

cds_rates <- function() {
  bootstrap_ois_curve("2026-11-02", "2026-11-02", "tonar", "actual_365_fixed",
                  create_ois_instruments(c("1Y", "5Y", "10Y", "20Y"), c(0.01, 0.015, 0.02, 0.022), 2L, 1L, 0L))
}

flat_nodes <- function() {
  bootstrap_ois_curve("2026-11-02", "2026-11-02", "tonar", "actual_365_fixed",
                  create_ois_instruments(c("1Y", "5Y", "10Y", "20Y"), c(0.01, 0.015, 0.02, 0.022), 2L, 1L, 0L), interpolation = "log_linear")
}

test_that("the fair spread makes the value 0 in both engines", {
  hazard <- create_hazard_curve("2026-11-02", "2040-01-01", 0.02)
  for (engine in c("midpoint", "isda")) {
    v <- compute_cds_value(create_credit_default_swap("buyer", 1e7, 0.01, "2026-11-03", "2031-12-20"), hazard, 0.4, cds_rates(), "2026-11-02", engine)
    fair <- compute_cds_value(create_credit_default_swap("buyer", 1e7, v$fair_spread, "2026-11-03", "2031-12-20"), hazard, 0.4, cds_rates(), "2026-11-02", engine)
    expect_lt(abs(fair$npv), 1e-3)
    seller <- compute_cds_value(create_credit_default_swap("seller", 1e7, 0.01, "2026-11-03", "2031-12-20"), hazard, 0.4, cds_rates(), "2026-11-02", engine)
    expect_equal(seller$npv, -v$npv, tolerance = 1e-10)
  }
})

test_that("survival falls and a bootstrap reprices its spreads", {
  m <- as.Date(c("2027-12-20", "2029-12-20", "2031-12-20"))
  s <- c(0.006, 0.009, 0.012)
  curve <- bootstrap_hazard_curve("2026-11-02", m, s, 0.4, cds_rates(), "midpoint")
  expect_true(all(diff(compute_survival_probability(curve, m)) < 0))
  expect_equal(nrow(curve$nodes), 3)
  for (i in seq_along(m)) {
    v <- compute_cds_value(create_credit_default_swap("buyer", 100, 0.01, "2026-11-02", m[i], rule = "backward"), curve, 0.4, cds_rates(), "2026-11-02", "midpoint")
    # the quotes rebate one day of accrual and this swap, traded the day before, rebates none
    expect_lt(abs(v$fair_spread - s[i]), 5e-5)
  }
})

test_that("quote sensitivities agree with rebuilding the curve", {
  m <- as.Date(c("2027-12-20", "2029-12-20", "2031-12-20"))
  s <- c(0.006, 0.009, 0.012)
  cds <- create_credit_default_swap("buyer", 1e7, 0.01, "2026-11-03", "2030-12-20")
  value <- function(q, r = 0.4) compute_cds_value(cds, bootstrap_hazard_curve("2026-11-02", m, q, 0.4, cds_rates(), "midpoint"), r, cds_rates(), "2026-11-02", "midpoint")$npv
  x <- compute_cds_quote_sensitivity(cds, bootstrap_hazard_curve("2026-11-02", m, s, 0.4, cds_rates(), "midpoint"), cds_rates(), "2026-11-02", "midpoint")
  for (i in seq_along(s)) {
    up <- s; up[i] <- up[i] + 1e-5
    down <- s; down[i] <- down[i] - 1e-5
    numeric <- (value(up) - value(down)) / 2e-5 * 1e-4
    expect_lt(abs(x$npv_per_bp[i] - numeric), 1e-3)
  }
  expect_lt(abs(x$npv_per_recovery_percent[1] - (value(s, 0.41) - value(s, 0.39)) / 2), 1e-2)
})

test_that("maturities are on the twentieth, and bad names fail", {
  expect_equal(format(compute_cds_maturity("2026-11-02", "5Y"), "%d"), "20")
  expect_error(create_credit_default_swap("broker", 1e7, 0.01, "2026-11-03", "2031-12-20"), "protection side")
  expect_error(create_credit_default_swap("buyer", 1e7, 0.01, "2026-11-03", "2031-12-20", rebates_accrual = NA), "TRUE or FALSE")
})

test_that("the settings of QuantLib's builder agree with QuantLib", {
  skip_if_not_installed("QuantLib")
  date_of <- function(x) QuantLib::Date(as.integer(as.Date(x)) + 25569L)
  today <- date_of("2026-11-02")
  QuantLib::Settings_instance()$setEvaluationDate(today)
  dc <- QuantLib::Actual365Fixed()
  yields <- QuantLib::YieldTermStructureHandle(QuantLib::DiscountCurve(
    do.call(function(...) { v <- QuantLib::DateVector(); for (d in list(...)) QuantLib::DateVector_push_back(v, d); v }, lapply(as.character(get_discount_nodes(flat_nodes())$date), date_of)),
    get_discount_nodes(flat_nodes())$discount, dc
  ))
  hazard <- QuantLib::DefaultProbabilityTermStructureHandle(QuantLib::FlatHazardRate(today, QuantLib::QuoteHandle(QuantLib::SimpleQuote(0.02)), dc))
  builder <- QuantLib::MakeCreditDefaultSwap__SWIG_1(date_of("2031-12-20"), 0.01)
  builder <- QuantLib::MakeCreditDefaultSwap_withNominal(builder, 1e7)
  builder <- QuantLib::MakeCreditDefaultSwap_withTradeDate(builder, today)
  builder <- QuantLib::MakeCreditDefaultSwap_withDateGenerationRule(builder, "CDS2015")
  builder <- QuantLib::MakeCreditDefaultSwap_withPricingEngine(builder, QuantLib::IsdaCdsEngine(hazard, 0.4, yields))
  quantlib <- QuantLib::MakeCreditDefaultSwap_makeCDS(builder)
  swap <- create_credit_default_swap("buyer", 1e7, 0.01, "2026-11-02", "2031-12-20", rule = "cds2015", trade_date = "2026-11-02",
                              protection_start = "2026-11-02", last_period_day_count = "actual_360_include_last_day")
  ours <- compute_cds_value(swap, create_hazard_curve("2026-11-02", "2060-01-01", 0.02), 0.4, flat_nodes(), "2026-11-02", "isda")
  expect_equal(ours$npv, quantlib$NPV(), tolerance = 1e-9)
  expect_equal(ours$coupon_leg_npv, quantlib$couponLegNPV(), tolerance = 1e-9)
  expect_equal(ours$accrual_rebate_npv, quantlib$accrualRebateNPV(), tolerance = 1e-9)
  expect_equal(ours$fair_upfront, quantlib$fairUpfront(), tolerance = 1e-9)
})

test_that("a last period that counts its last day adds one day of coupon", {
  plain <- compute_cds_value(create_credit_default_swap("buyer", 1e7, 0.01, "2026-11-03", "2031-12-20"), create_hazard_curve("2026-11-02", "2060-01-01", 0.02), 0.4, flat_nodes(), "2026-11-02", "midpoint")
  longer <- compute_cds_value(create_credit_default_swap("buyer", 1e7, 0.01, "2026-11-03", "2031-12-20", last_period_day_count = "actual_360_include_last_day"), create_hazard_curve("2026-11-02", "2060-01-01", 0.02), 0.4, flat_nodes(), "2026-11-02", "midpoint")
  expect_lt(longer$coupon_leg_npv, plain$coupon_leg_npv)
  expect_lt(plain$coupon_leg_npv - longer$coupon_leg_npv, 1e7 * 0.01 / 360)
  expect_equal(longer$default_leg_npv, plain$default_leg_npv)
  expect_error(create_credit_default_swap("buyer", 1e7, 0.01, "2026-11-03", "2031-12-20", last_period_day_count = "nonsense"))
})
