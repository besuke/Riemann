# Golden values for the American option approximations (Barone-Adesi-Whaley, Bjerksund-Stensland) from
# the QuantLib R package Version 1.42. All inputs are synthetic; no market data is used.
# Usage (from the Riemann root): Rscript tools/golden/p11_american_approximations.R
# Writes tests/golden/american_approximations.csv. A case QuantLib refuses with an error has the value NA;
# a case where it returns NaN has NaN.

suppressMessages(library(QuantLib))

out <- file.path("tests", "golden", "american_approximations.csv")
today <- QuantLib::Date(1L, "March", 2024L)
invisible(QuantLib::Settings_instance()$setEvaluationDate(today))
dc <- QuantLib::Actual365Fixed()
flat <- function(rate) QuantLib::YieldTermStructureHandle(QuantLib::FlatForward(today, rate, dc))

value_of <- function(method, type, spot, strike, rate, dividend, volatility, days) {
  process <- QuantLib::BlackScholesMertonProcess(
    QuantLib::QuoteHandle(QuantLib::SimpleQuote(spot)), flat(dividend), flat(rate),
    QuantLib::BlackVolTermStructureHandle(QuantLib::BlackConstantVol(today, QuantLib::TARGET(), volatility, dc))
  )
  option <- QuantLib::VanillaOption(QuantLib::PlainVanillaPayoff(type, strike), QuantLib::AmericanExercise(today, today + days))
  engine <- switch(method,
    barone_adesi_whaley = QuantLib::BaroneAdesiWhaleyApproximationEngine(process),
    bjerksund_stensland = QuantLib::BjerksundStenslandApproximationEngine(process)
  )
  invisible(QuantLib::Instrument_setPricingEngine(option, engine))
  QuantLib::Instrument_NPV(option)
}

cases <- tidyr::expand_grid(
  method = c("barone_adesi_whaley", "bjerksund_stensland"), type = c("Call", "Put"), spot = c(60, 90, 100, 110, 160), strike = 100,
  rate = c(-0.01, 0, 0.03, 0.08), dividend = c(0, 0.04, 0.12), volatility = c(0.1, 0.3), days = c(30L, 365L, 1825L)
)
cases$value <- purrr::pmap_dbl(cases, purrr::possibly(value_of, NA_real_))
cases$time <- cases$days / 365

writeLines(c(paste0("# QuantLib R package ", packageVersion("QuantLib"), "; synthetic inputs; tools/golden/p11_american_approximations.R"),
             "method,type,spot,strike,rate,dividend,volatility,time,value"), out)
lines <- purrr::pmap_chr(cases, \(method, type, spot, strike, rate, dividend, volatility, days, value, time) {
  paste(method, tolower(type), sprintf("%.17g", spot), sprintf("%.17g", strike), sprintf("%.17g", rate), sprintf("%.17g", dividend),
        sprintf("%.17g", volatility), sprintf("%.17g", time), if (is.nan(value)) "NaN" else if (is.na(value)) "NA" else sprintf("%.17g", value), sep = ",")
})
write(lines, out, append = TRUE)
cat(nrow(cases), "cases,", sum(is.na(cases$value) & !is.nan(cases$value)), "refused by QuantLib,", sum(is.nan(cases$value)), "NaN\n")
