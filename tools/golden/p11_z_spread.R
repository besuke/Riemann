# Golden values for Riemann P11-1 (bond value with a zero spread, and the spread of a price) from the
# QuantLib R package Version 1.42. All inputs are synthetic; no market data is used.
# Usage (from the Riemann root): Rscript tools/golden/p11_z_spread.R
# Writes tests/golden/z_spreads.csv. At v1.42 the spread always uses the curve's day count: the
# day counter passed to ZeroSpreadedTermStructure and BondFunctions_zSpread is ignored.

suppressMessages(library(QuantLib))

set.seed(20261111)
fmt <- function(x) sprintf("%.17g", x)
at <- function(serial) QuantLib::Date(as.integer(serial))
calendars <- list(japan = QuantLib::Japan(), target = QuantLib::TARGET())
day_counts <- list(actual_365_fixed = QuantLib::Actual365Fixed(), thirty_360_bond_basis = QuantLib::Thirty360("BondBasis"))
frequencies <- c(Annual = 1L, Semiannual = 2L, Quarterly = 4L)

path <- file.path("tests", "golden", "z_spreads.csv")
writeLines(c(
  paste0("# QuantLib R package ", packageVersion("QuantLib"), "; synthetic inputs; tools/golden/p11_z_spread.R; nodes are serial:discount"),
  paste(c("calendar", "day_count", "settlement_days", "issue", "maturity", "coupon", "evaluation", "nodes", "compounding", "frequency",
          "spread", "npv", "settlement_value", "dirty", "clean", "price", "price_type", "z_spread"), collapse = ",")
), path)

for (k in 1:200) {
  calendar_name <- sample(names(calendars), 1)
  day_count_name <- sample(names(day_counts), 1)
  calendar <- calendars[[calendar_name]]
  day_count <- day_counts[[day_count_name]]
  evaluation <- 43000L + sample(0:4000, 1)
  if (!QuantLib::Calendar_isBusinessDay(calendar, at(evaluation))) evaluation <- as.integer(QuantLib::Date_serialNumber(QuantLib::Calendar_adjust(calendar, at(evaluation))))
  QuantLib::Settings_instance()$setEvaluationDate(at(evaluation))
  settlement_days <- sample(0:3, 1)
  issue <- evaluation - sample(1:900, 1)
  maturity <- issue + 365L * sample(c(3L, 5L, 10L, 20L), 1) + sample(0:30, 1)
  coupon <- round(runif(1, 0, 0.08), 4)
  schedule <- QuantLib::Schedule(at(issue), at(maturity), QuantLib::Period(6L, "Months"), calendar, "Unadjusted", "Unadjusted", "Backward", FALSE)
  bond <- QuantLib::FixedRateBond(settlement_days, 100, schedule, coupon, day_count, "Following", 100, at(issue))

  node_days <- c(0L, 30L, 180L, 365L, 730L, 1825L, 3650L, 7300L, 10950L)
  level <- runif(1, -0.002, 0.05)
  zero <- level + cumsum(c(0, runif(8, -0.002, 0.003)))
  discounts <- exp(-zero * node_days / 365)
  dates <- do.call(QuantLib::DateVector, list())
  for (d in node_days) QuantLib::DateVector_push_back(dates, at(evaluation + d))
  curve <- QuantLib::DiscountCurve(dates, discounts, day_count)
  QuantLib::TermStructure_enableExtrapolation(curve)
  handle <- QuantLib::YieldTermStructureHandle(curve)

  compounding <- sample(c("Compounded", "Continuous", "Simple"), 1, prob = c(0.6, 0.3, 0.1))
  frequency <- if (compounding == "Compounded") sample(names(frequencies), 1) else "Annual"
  spread <- sample(c(-0.005, 0, 0.0025, 0.005, 0.02, 0.05), 1)
  spreaded <- QuantLib::ZeroSpreadedTermStructure(handle, QuantLib::QuoteHandle(QuantLib::SimpleQuote(spread)), compounding, frequency, day_count)
  QuantLib::TermStructure_enableExtrapolation(spreaded)
  QuantLib::Instrument_setPricingEngine(bond, QuantLib::DiscountingBondEngine(QuantLib::YieldTermStructureHandle(spreaded)))
  npv <- QuantLib::Instrument_NPV(bond)
  settlement_value <- QuantLib::Bond_settlementValue(bond)
  dirty <- QuantLib::Bond_dirtyPrice(bond)
  clean <- QuantLib::Bond_cleanPrice(bond)

  price_type <- sample(c("Clean", "Dirty"), 1)
  price <- (if (price_type == "Clean") clean else dirty) + sample(c(0, -1.5, 2.25), 1)
  settlement <- QuantLib::Bond_settlementDate(bond)
  z <- QuantLib::BondFunctions_zSpread(bond, QuantLib::BondPrice(price, price_type), curve, day_count, compounding, frequency, settlement, 1e-12, 1000L, 0)

  write(paste(c(calendar_name, day_count_name, settlement_days, issue, maturity, fmt(coupon), evaluation,
                paste(paste0(evaluation + node_days, ":", fmt(discounts)), collapse = " "), compounding, frequencies[[frequency]],
                fmt(spread), fmt(c(npv, settlement_value, dirty, clean, price)), price_type, fmt(z)), collapse = ","), path, append = TRUE)
}
cat("written", path, "\n")
