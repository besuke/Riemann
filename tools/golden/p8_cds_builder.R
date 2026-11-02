# Golden values for credit default swaps made with QuantLib's MakeCreditDefaultSwap, which LagrangeFinance
# II-5 uses, from the QuantLib R package Version 1.42. All inputs are synthetic; no market data is used.
# Usage (from the Riemann root): Rscript tools/golden/p8_cds_builder.R
# Writes tests/golden/cds_builder.csv. The builder's settings (ql/instruments/makecds.hpp at v1.42):
# Actual/360 with the last period on Actual/360 including its last day, 3 cash settlement days,
# Following, WeekendsOnly, protection from the trade date for the CDS rules, an upfront rate of 0.

suppressMessages(library(QuantLib))

set.seed(20261112)
fmt <- function(x) sprintf("%.17g", x)
act365 <- QuantLib::Actual365Fixed()
today <- QuantLib::Date(2L, "November", 2026L)
invisible(QuantLib::Settings_instance()$setEvaluationDate(today))
today_serial <- as.integer(QuantLib::Date_serialNumber(today))

path <- file.path("tests", "golden", "cds_builder.csv")
writeLines(c(
  paste0("# QuantLib R package ", packageVersion("QuantLib"), "; synthetic inputs; tools/golden/p8_cds_builder.R"),
  paste(c("engine", "side", "maturity", "notional", "spread", "rate", "hazard", "recovery",
          "npv", "coupon_leg", "default_leg", "rebate", "fair_spread", "fair_upfront"), collapse = ",")
), path)

for (i in 1:200) {
  engine_name <- sample(c("midpoint", "isda"), 1)
  side <- sample(c("buyer", "seller"), 1)
  years <- sample(1:10, 1)
  maturity <- QuantLib::Date(20L, sample(c("March", "June", "September", "December"), 1), 2026L + years)
  notional <- sample(c(1e6, 1e7), 1)
  spread <- runif(1, 0.001, 0.05)
  rate <- runif(1, 0.001, 0.08)
  hazard <- runif(1, 0.001, 0.15)
  recovery <- runif(1, 0.1, 0.7)
  yield_handle <- QuantLib::YieldTermStructureHandle(QuantLib::FlatForward(today, rate, act365, "Continuous"))
  hazard_handle <- QuantLib::DefaultProbabilityTermStructureHandle(QuantLib::FlatHazardRate(today, QuantLib::QuoteHandle(QuantLib::SimpleQuote(hazard)), act365))
  engine <- if (engine_name == "midpoint") {
    QuantLib::MidPointCdsEngine(hazard_handle, recovery, yield_handle)
  } else {
    QuantLib::IsdaCdsEngine(hazard_handle, recovery, yield_handle)
  }
  builder <- QuantLib::MakeCreditDefaultSwap__SWIG_1(maturity, spread)
  builder <- QuantLib::MakeCreditDefaultSwap_withNominal(builder, notional)
  builder <- QuantLib::MakeCreditDefaultSwap_withSide(builder, if (side == "buyer") QuantLib::Protection_Buyer_get() else QuantLib::Protection_Seller_get())
  builder <- QuantLib::MakeCreditDefaultSwap_withTradeDate(builder, today)
  builder <- QuantLib::MakeCreditDefaultSwap_withCouponTenor(builder, QuantLib::Period(3L, "Months"))
  builder <- QuantLib::MakeCreditDefaultSwap_withDayCounter(builder, QuantLib::Actual360())
  builder <- QuantLib::MakeCreditDefaultSwap_withDateGenerationRule(builder, "CDS2015")
  builder <- QuantLib::MakeCreditDefaultSwap_withPricingEngine(builder, engine)
  cds <- QuantLib::MakeCreditDefaultSwap_makeCDS(builder)
  write(paste(c(engine_name, side, as.integer(QuantLib::Date_serialNumber(maturity)),
                fmt(c(notional, spread, rate, hazard, recovery, cds$NPV(), cds$couponLegNPV(), cds$defaultLegNPV(), cds$accrualRebateNPV(), cds$fairSpread(), cds$fairUpfront()))), collapse = ","),
        path, append = TRUE)
}
cat("written", path, "\n")
