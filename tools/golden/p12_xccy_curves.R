# Golden values for curves bootstrapped from FX swaps and cross-currency basis swaps, from the QuantLib R
# package Version 1.42 (FxSwapRateHelper, ConstNotionalCrossCurrencyBasisSwapRateHelper,
# MtMCrossCurrencyBasisSwapRateHelper with overnight indices, PiecewiseLogLinearDiscount). All inputs are
# synthetic; no market data is used. The overnight indices forecast from flat continuous curves.
# Usage (from the Riemann root): Rscript tools/golden/p12_xccy_curves.R
# Writes tests/golden/xccy_curves.csv: one row per case and node, with the discount factor.

suppressMessages(library(QuantLib))

out <- file.path("tests", "golden", "xccy_curves.csv")
today <- QuantLib::Date(4L, "November", 2026L)
invisible(QuantLib::Settings_instance()$setEvaluationDate(today))
dc <- QuantLib::Actual365Fixed()
flat <- function(rate) QuantLib::YieldTermStructureHandle(QuantLib::FlatForward(today, rate, dc))
serial <- function(d) as.integer(QuantLib::Date_serialNumber(d))
index_of <- function(name, handle) switch(name, sofr = QuantLib::Sofr(handle), tonar = QuantLib::Tona(handle), estr = QuantLib::Estr(handle))
calendar_of <- function(name) switch(name, sofr = QuantLib::UnitedStates("SOFR"), tonar = QuantLib::Japan(), estr = QuantLib::TARGET())
period_of <- function(tenor) QuantLib::Period(as.integer(sub("[MY]$", "", tenor)), if (endsWith(tenor, "M")) "Months" else "Years")

# Each case: the pair (base, quote), the collateral, flat rates of the two indices and of the collateral
# curve, the FX swaps and the basis swaps.
cases <- list(
  list(id = 1L, base = "sofr", quote = "tonar", base_rate = 0.042, quote_rate = 0.005, collateral_rate = 0.005, base_is_collateral = FALSE,
       spot = 150, fx = list(c("1M", -0.55), c("3M", -1.62), c("6M", -3.2)),
       basis = list(c("1Y", -0.0035), c("2Y", -0.0042), c("5Y", -0.0050), c("10Y", -0.0048)), basis_on_base = FALSE, resetting = "none"),
  list(id = 2L, base = "sofr", quote = "tonar", base_rate = 0.042, quote_rate = 0.005, collateral_rate = 0.005, base_is_collateral = FALSE,
       spot = 150, fx = list(),
       basis = list(c("1Y", -0.0035), c("3Y", -0.0045), c("7Y", -0.0050)), basis_on_base = FALSE, resetting = "base"),
  list(id = 3L, base = "estr", quote = "sofr", base_rate = 0.022, quote_rate = 0.042, collateral_rate = 0.042, base_is_collateral = FALSE,
       spot = 1.08, fx = list(c("3M", 0.0052)),
       basis = list(c("2Y", -0.0012), c("5Y", -0.0015), c("10Y", -0.0010)), basis_on_base = TRUE, resetting = "none"),
  list(id = 4L, base = "estr", quote = "sofr", base_rate = 0.022, quote_rate = 0.042, collateral_rate = 0.042, base_is_collateral = FALSE,
       spot = 1.08, fx = list(),
       basis = list(c("2Y", -0.0012), c("5Y", -0.0015)), basis_on_base = TRUE, resetting = "base"),
  list(id = 5L, base = "sofr", quote = "tonar", base_rate = 0.042, quote_rate = 0.005, collateral_rate = 0.041, base_is_collateral = TRUE,
       spot = 150, fx = list(c("6M", -3.1)),
       basis = list(c("1Y", -0.0030), c("5Y", -0.0045)), basis_on_base = FALSE, resetting = "quote")
)

writeLines(c(paste0("# QuantLib R package ", packageVersion("QuantLib"), "; synthetic inputs; tools/golden/p12_xccy_curves.R"),
             "case,node,discount"), out)
purrr::walk(cases, \(cs) {
  base_index <- index_of(cs$base, flat(cs$base_rate))
  quote_index <- index_of(cs$quote, flat(cs$quote_rate))
  collateral <- flat(cs$collateral_rate)
  calendar <- QuantLib::JointCalendar(calendar_of(cs$base), calendar_of(cs$quote))
  helpers <- QuantLib::RateHelperVector()
  purrr::walk(cs$fx, \(q) helpers$append(QuantLib::FxSwapRateHelper(
    QuantLib::QuoteHandle(QuantLib::SimpleQuote(as.numeric(q[2]))), QuantLib::QuoteHandle(QuantLib::SimpleQuote(cs$spot)), period_of(q[1]), 2L,
    calendar, "ModifiedFollowing", FALSE, cs$base_is_collateral, collateral)))
  purrr::walk(cs$basis, \(q) {
    basis <- QuantLib::QuoteHandle(QuantLib::SimpleQuote(as.numeric(q[2])))
    helper <- if (cs$resetting == "none") {
      QuantLib::ConstNotionalCrossCurrencyBasisSwapRateHelper(basis, period_of(q[1]), 2L, calendar, "ModifiedFollowing", FALSE, base_index, quote_index,
                                                               collateral, cs$base_is_collateral, cs$basis_on_base, "Quarterly", 0L)
    } else {
      QuantLib::MtMCrossCurrencyBasisSwapRateHelper(basis, period_of(q[1]), 2L, calendar, "ModifiedFollowing", FALSE, base_index, quote_index,
                                                     collateral, cs$base_is_collateral, cs$basis_on_base, cs$resetting == "base", "Quarterly", 0L)
    }
    helpers$append(helper)
  })
  curve <- QuantLib::PiecewiseLogLinearDiscount(today, helpers, dc)
  dates <- curve$dates()
  purrr::walk(seq_len(QuantLib::DateVector_size(dates)) - 1L, \(i) {
    d <- QuantLib::DateVector___getitem__(dates, i)
    write(sprintf("%d,%d,%.17g", cs$id, serial(d), curve$discount(d)), out, append = TRUE)
  })
})
cat("cases:", length(cases), "\n")
