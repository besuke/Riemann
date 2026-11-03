# QuantLib Guide: a European option and the Greeks of a barrier option
#
# What it computes: the European call of "Instruments and pricing engines" (value and Greeks, and the value
# after changing the spot, the rate, the volatility and the evaluation date) and the up-and-in barrier call
# of "Numerical Greeks" (https://www.quantlibguide.com/). The Guide finds the barrier Greeks by finite
# differences, because QuantLib's barrier engine gives none; RiemannFinMath gives them by automatic
# differentiation. The table compares RiemannFinMath with the Guide's printed values and, when the
# QuantLib R package is installed, with the same calls to the local QuantLib.
#
# Run:   source(system.file("examples", "quantlib_guide", "options.R", package = "RiemannFinMath"))
# Needs: RiemannFinMath and tibble. QuantLib (the R package) is optional.
#
# Conditions: rates and volatilities are flat, Actual/360 for both, continuous compounding, no dividend.
# Times are Actual/360 year fractions from the evaluation date to the expiry.
#
# Not reproduced (docs/quantlib_guide_inventory.md): the Heston model. The Monte Carlo value is shown,
# but with other random numbers it differs from QuantLib's by sampling error.

library(RiemannFinMath)
has_quantlib <- requireNamespace("QuantLib", quietly = TRUE)
rows <- list()
add <- function(page, item, guide, riemann, quantlib = NA_real_, note = "") {
  rows[[length(rows) + 1L]] <<- tibble::tibble(page = page, item = item, guide = guide, quantlib_local = quantlib, riemann = riemann,
                                               riemann_minus_guide = riemann - guide, note = note)
}
years <- function(from, to) as.numeric(as.Date(to) - as.Date(from)) / 360
ql_date <- function(x) QuantLib::Date(as.integer(as.Date(x)) + 25569L)
ql_european <- function(eval, expiry, spot, rate, vol) {
  invisible(QuantLib::Settings_instance()$setEvaluationDate(ql_date(eval)))
  dc <- QuantLib::Actual360()
  process <- QuantLib::BlackScholesProcess(
    QuantLib::QuoteHandle(QuantLib::SimpleQuote(spot)),
    QuantLib::YieldTermStructureHandle(QuantLib::FlatForward(0L, QuantLib::TARGET(), QuantLib::QuoteHandle(QuantLib::SimpleQuote(rate)), dc)),
    QuantLib::BlackVolTermStructureHandle(QuantLib::BlackConstantVol(0L, QuantLib::TARGET(), QuantLib::QuoteHandle(QuantLib::SimpleQuote(vol)), dc))
  )
  option <- QuantLib::VanillaOption(QuantLib::PlainVanillaPayoff("Call", 100), QuantLib::EuropeanExercise(ql_date(expiry)))
  QuantLib::Instrument_setPricingEngine(option, QuantLib::AnalyticEuropeanEngine(process))
  option
}

# ---- "Instruments and pricing engines": evaluation date 7 March 2024, expiry 7 June 2024 -----------------
page <- "Instruments and pricing engines"
t0 <- years("2024-03-07", "2024-06-07")
base <- compute_european_option_value("call", 100, 100, 0.01, 0, 0.20, t0)
q <- rep(NA_real_, 4)
if (has_quantlib) {
  o <- ql_european("2024-03-07", "2024-06-07", 100, 0.01, 0.20)
  q <- c(QuantLib::Instrument_NPV(o), QuantLib::OneAssetOption_delta(o), QuantLib::OneAssetOption_gamma(o), QuantLib::OneAssetOption_vega(o))
}
add(page, "Value", 4.155543462156205, base$value, q[1])
add(page, "Delta", 0.5302223303784392, base$delta, q[2])
add(page, "Gamma", 0.03934493301271913, base$gamma, q[3])
add(page, "Vega", 20.109632428723106, base$vega, q[4])
add(page, "Value at spot 105", 7.275563579278455, compute_european_option_value("call", 105, 100, 0.01, 0, 0.20, t0)$value)
add(page, "Value at spot 105, rate 2%", 7.448878025811252, compute_european_option_value("call", 105, 100, 0.02, 0, 0.20, t0)$value)
add(page, "Value at spot 105, rate 2%, volatility 15%", 6.596556078273314, compute_european_option_value("call", 105, 100, 0.02, 0, 0.15, t0)$value)
add(page, "Value at spot 105, evaluation date 7 April 2024 (a Sunday)", 6.535204576446802,
    compute_european_option_value("call", 105, 100, 0.01, 0, 0.20, years("2024-04-08", "2024-06-07"))$value,
    note = "QuantLib moves the reference date of FlatForward(0, TARGET) to the next business day, 8 April: 60 days here")
mc <- compute_monte_carlo_european_value("call", 105, 100, 0.01, 0, 0.20, t0, paths = 500000, seed = 1)
add(page, "Monte Carlo value at spot 105 (500,000 paths)", 7.291905118788459, mc$value,
    note = sprintf("Sampling error; standard error %.4f. The analytic value is 7.2756", mc$standard_error))

# ---- "Numerical Greeks": evaluation date 8 October 2014, expiry 8 January 2015 -------------------------------
page <- "Numerical Greeks"
t1 <- years("2014-10-08", "2015-01-08")
barrier <- compute_barrier_option_value("call", "up_in", 100, 100, 120, 30, 0.01, 0, 0.20, t1)
add(page, "Up-and-in call, barrier 120, rebate 30: value", 29.24999528163375, barrier$value)
add(page, "Delta", -0.14781859063752734, barrier$delta, note = "Guide: central difference, h = 0.01. Here: exact derivative")
add(page, "Gamma", -0.025175243933972524, barrier$gamma, note = "Guide: central difference, h = 0.01")
add(page, "Rho", -9.984440333461464, barrier$rho, note = "Guide: one-sided difference, h = 0.0001")
add(page, "Vega", -12.997818541258255, barrier$vega, note = "Guide: one-sided difference, h = 0.0001")
add(page, "Theta per year", 5.548998890566246, barrier$theta,
    note = sprintf("Guide: one day later divided by 1/365, times Actual/360. Times 365/360 here: %.4f", barrier$theta * 365 / 360))
european <- compute_european_option_value("call", 100, 100, 0.01, 0, 0.20, t1)
add(page, "European call, same dates: value", 4.155543462156205, european$value)
add(page, "European call: delta", 0.5302223303784392, european$delta)

results <- do.call(rbind, rows)
cat("QuantLib Guide, options\n")
cat("QuantLib R package:", if (has_quantlib) as.character(utils::packageVersion("QuantLib")) else "not installed", "\n\n")
old <- options(width = 250)
print(as.data.frame(results), digits = 12, row.names = FALSE, right = FALSE)
options(old)
invisible(results)
