# QuantLib official example: EquityOption (QuantLib v1.42, Examples/EquityOption/EquityOption.cpp)
#
# What it computes: the put of the example valued with the methods RiemannFinMath has: the Black-Scholes
# formula (European), the Barone-Adesi-Whaley and Bjerksund-Stensland approximations (American), and the
# Cox-Ross-Rubinstein tree with 801 steps (European and American). RiemannFinMath's Rust functions do the
# calculation. The example prints its values only when it runs, so the table compares them with the same
# engines of the local QuantLib R package, when it is installed.
#
# Run:   source(system.file("examples", "quantlib_examples", "equity_option.R", package = "RiemannFinMath"))
# Needs: RiemannFinMath, purrr and tibble. QuantLib (the R package) is optional.
#
# Conditions: put, spot 36, strike 40, risk-free rate 6%, dividend yield 0, volatility 20%; curves and
# volatility from the settlement date 17 May 1998 to the maturity 17 May 1999 on Actual/365 (Fixed), so
# one year (the evaluation date is 15 May 1998); American exercise from the settlement date.
#
# Not available (docs/quantlib_guide_inventory.md): Bermudan exercise; the Black-Vasicek, Heston and Bates
# models; QD+; the integral and finite-difference engines; the Jarrow-Rudd, additive equiprobabilities,
# Trigeorgis, Tian, Leisen-Reimer and Joshi trees; Monte Carlo and quasi-Monte Carlo with QuantLib's
# random numbers and Longstaff-Schwartz on QuantLib's paths (RiemannFinMath has its own generators, so
# their values are not comparable).

library(RiemannFinMath)
has_quantlib <- requireNamespace("QuantLib", quietly = TRUE)
put <- list(spot = 36, strike = 40, rate = 0.06, dividend = 0, volatility = 0.20, time = 1)
steps <- 801L

methods <- tibble::tribble(
  ~method,                         ~exercise,
  "Black-Scholes",                 "european",
  "Barone-Adesi/Whaley",           "american",
  "Bjerksund/Stensland",           "american",
  "Binomial Cox-Ross-Rubinstein",  "european",
  "Binomial Cox-Ross-Rubinstein",  "american"
)
riemann_value <- function(method, exercise) {
  with(put, switch(method,
    "Black-Scholes" = compute_european_option_value("put", spot, strike, rate, dividend, volatility, time)$value,
    "Barone-Adesi/Whaley" = compute_american_approximation_value("barone_adesi_whaley", "put", spot, strike, rate, dividend, volatility, time)$value,
    "Bjerksund/Stensland" = compute_american_approximation_value("bjerksund_stensland", "put", spot, strike, rate, dividend, volatility, time)$value,
    "Binomial Cox-Ross-Rubinstein" = compute_binomial_option_value("put", exercise, spot, strike, rate, dividend, volatility, time, steps)$value
  ))
}

quantlib_value <- function(method, exercise) NA_real_
if (has_quantlib) {
  ql_date <- function(x) QuantLib::Date(as.integer(as.Date(x)) + 25569L)
  invisible(QuantLib::Settings_instance()$setEvaluationDate(ql_date("1998-05-15")))
  settlement <- ql_date("1998-05-17")
  maturity <- ql_date("1999-05-17")
  dc <- QuantLib::Actual365Fixed()
  flat <- function(rate) QuantLib::YieldTermStructureHandle(QuantLib::FlatForward(settlement, rate, dc))
  process <- QuantLib::BlackScholesMertonProcess(
    QuantLib::QuoteHandle(QuantLib::SimpleQuote(put$spot)), flat(put$dividend), flat(put$rate),
    QuantLib::BlackVolTermStructureHandle(QuantLib::BlackConstantVol(settlement, QuantLib::TARGET(), put$volatility, dc))
  )
  payoff <- QuantLib::PlainVanillaPayoff("Put", put$strike)
  quantlib_value <- function(method, exercise) {
    option <- QuantLib::VanillaOption(payoff, if (exercise == "european") QuantLib::EuropeanExercise(maturity) else QuantLib::AmericanExercise(settlement, maturity))
    engine <- switch(method,
      "Black-Scholes" = QuantLib::AnalyticEuropeanEngine(process),
      "Barone-Adesi/Whaley" = QuantLib::BaroneAdesiWhaleyApproximationEngine(process),
      "Bjerksund/Stensland" = QuantLib::BjerksundStenslandApproximationEngine(process),
      "Binomial Cox-Ross-Rubinstein" = QuantLib::BinomialCRRVanillaEngine(process, steps)
    )
    invisible(QuantLib::Instrument_setPricingEngine(option, engine))
    QuantLib::Instrument_NPV(option)
  }
}

results <- tibble::tibble(
  methods,
  quantlib_local = purrr::pmap_dbl(methods, quantlib_value),
  riemann = purrr::pmap_dbl(methods, riemann_value)
)
results$riemann_minus_quantlib <- results$riemann - results$quantlib_local

cat("QuantLib official example EquityOption: a put, spot 36, strike 40, one year\n")
cat("QuantLib R package:", if (has_quantlib) as.character(utils::packageVersion("QuantLib")) else "not installed", "\n\n")
print(as.data.frame(results), digits = 15, row.names = FALSE)
if (has_quantlib) cat("\nLargest difference from QuantLib 1.42:", format(max(abs(results$riemann_minus_quantlib)), digits = 2), "\n")
invisible(results)
