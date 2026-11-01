# Golden values for Riemann P8 (credit default swaps) from the QuantLib R package Version 1.42, called
# as LagrangeFinance II-5 calls it. All inputs are synthetic; no market data is used.
# Usage (from the Riemann root): Rscript tools/golden/p8_cds.R
# Writes tests/golden/cds_{values,hazards}.csv.

suppressMessages(library(QuantLib))

out_dir <- file.path("tests", "golden")
set.seed(20261105)
fmt <- function(x) sprintf("%.17g", x)
act365 <- QuantLib::Actual365Fixed()
act360 <- QuantLib::Actual360()
calendar <- QuantLib::WeekendsOnly()
today <- QuantLib::Date(2L, "November", 2026L)
invisible(QuantLib::Settings_instance()$setEvaluationDate(today))
today_serial <- as.integer(QuantLib::Date_serialNumber(today))
at <- function(serial) QuantLib::Date(as.integer(serial))
header <- function(path, columns) {
  writeLines(c(paste0("# QuantLib R package ", packageVersion("QuantLib"), "; synthetic inputs; tools/golden/p8_cds.R"),
               paste(columns, collapse = ",")), path)
}
add <- function(path, values) write(paste(values, collapse = ","), path, append = TRUE)

yield_handle <- function(rate) {
  QuantLib::YieldTermStructureHandle(QuantLib::FlatForward(today, rate, act365, "Continuous"))
}
hazard_handle <- function(hazard) {
  QuantLib::DefaultProbabilityTermStructureHandle(QuantLib::FlatHazardRate(today, QuantLib::QuoteHandle(QuantLib::SimpleQuote(hazard)), act365))
}
side_of <- function(side) if (side == "buyer") QuantLib::Protection_Buyer_get() else QuantLib::Protection_Seller_get()
schedule_of <- function(start, end, rule) {
  QuantLib::Schedule(at(start), at(end), QuantLib::Period(3L, "Months"), calendar, "Following", "Unadjusted", rule, FALSE)
}

path <- file.path(out_dir, "cds_values.csv")
header(path, c("engine", "side", "rule", "start", "end", "notional", "spread", "upfront", "rate", "hazard", "recovery",
               "npv", "coupon_leg", "default_leg", "fair_spread", "fair_upfront"))
for (i in 1:300) {
  engine_name <- sample(c("midpoint", "isda"), 1)
  side <- sample(c("buyer", "seller"), 1)
  rule <- sample(c("Forward", "Backward", "CDS2015"), 1, prob = c(0.3, 0.3, 0.4))
  start <- today_serial + sample(c(1L, 3L, 30L), 1)
  if (rule == "CDS2015") start <- today_serial - sample(0:60, 1)
  # the twentieth of a quarter month for the CDS rule
  years <- sample(1:10, 1)
  end <- as.integer(QuantLib::Date_serialNumber(QuantLib::Date(20L, "December", 2026L + years)))
  notional <- sample(c(1e6, 1e7), 1)
  spread <- runif(1, 0.001, 0.05)
  upfront <- sample(c(NA, 0.02), 1)
  rate <- runif(1, 0.001, 0.08)
  hazard <- runif(1, 0.001, 0.15)
  recovery <- runif(1, 0.1, 0.7)
  schedule <- schedule_of(start, end, rule)
  cds <- if (is.na(upfront)) {
    QuantLib::CreditDefaultSwap(side_of(side), notional, spread, schedule, "Following", act360)
  } else {
    QuantLib::CreditDefaultSwap(side_of(side), notional, upfront, spread, schedule, "Following", act360)
  }
  engine <- if (engine_name == "midpoint") {
    QuantLib::MidPointCdsEngine(hazard_handle(hazard), recovery, yield_handle(rate))
  } else {
    QuantLib::IsdaCdsEngine(hazard_handle(hazard), recovery, yield_handle(rate))
  }
  cds$setPricingEngine(engine)
  add(path, c(engine_name, side, rule, start, end, fmt(c(notional, spread, if (is.na(upfront)) 0 else upfront, rate, hazard, recovery)),
              fmt(c(cds$NPV(), cds$couponLegNPV(), cds$defaultLegNPV(), cds$fairSpread(), tryCatch(cds$fairUpfront(), error = function(e) NaN)))))
}

# Hazard rates bootstrapped from running spreads
path <- file.path(out_dir, "cds_hazards.csv")
header(path, c("case", "engine", "rate", "recovery", "years", "spread", "node", "survival"))
for (case_id in 1:40) {
  engine_name <- sample(c("midpoint", "isda"), 1)
  rate <- runif(1, 0.001, 0.06)
  recovery <- runif(1, 0.2, 0.6)
  tenor_years <- c(1L, 2L, 3L, 5L, 7L, 10L)
  spreads <- cumsum(c(runif(1, 0.002, 0.02), runif(5, 0, 0.002)))
  model <- if (engine_name == "isda") QuantLib::CreditDefaultSwap_ISDA_get() else QuantLib::CreditDefaultSwap_Midpoint_get()
  helpers <- QuantLib::DefaultProbabilityHelperVector()
  for (k in seq_along(tenor_years)) {
    h <- QuantLib::SpreadCdsHelper(spreads[k], QuantLib::Period(tenor_years[k], "Years"), 0L, calendar, "Quarterly", "Following", "Backward", act360, recovery, yield_handle(rate),
                                   TRUE, TRUE, QuantLib::Date(), act360, TRUE, model)
    helpers$append(h)
  }
  curve <- QuantLib::PiecewiseFlatHazardRate(today, helpers, act365)
  nodes <- curve$dates()
  for (k in seq_along(tenor_years)) {
    # the first date of the curve is the reference date
    add(path, c(case_id, engine_name, fmt(c(rate, recovery)), tenor_years[k], fmt(spreads[k]),
                QuantLib::Date_serialNumber(QuantLib::DateVector___getitem__(nodes, k)),
                fmt(curve$survivalProbability(QuantLib::DateVector___getitem__(nodes, k)))))
  }
}
