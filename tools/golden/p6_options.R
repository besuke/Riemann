# Golden values for Riemann P6 (equity options and options on a forward) from the QuantLib R package
# Version 1.42. All inputs are synthetic random numbers; no market data is used.
# Usage (from the Riemann root): Rscript tools/golden/p6_options.R
# Writes tests/golden/option_{european,barrier,tree,forward,implied}.csv. Numbers have 17 significant
# digits; time is days / 365 and the rates are continuous, so QuantLib's flat curves reduce to the
# numeric inputs of Riemann.

suppressMessages(library(QuantLib))

out_dir <- file.path("tests", "golden")
set.seed(20261101)
fmt <- function(x) sprintf("%.17g", x)
day_count <- QuantLib::Actual365Fixed()
calendar <- QuantLib::NullCalendar()
today <- QuantLib::Date(2L, "November", 2026L)
invisible(QuantLib::Settings_instance()$setEvaluationDate(today))

process_of <- function(spot, rate, dividend, volatility) {
  QuantLib::BlackScholesMertonProcess(
    QuantLib::QuoteHandle(QuantLib::SimpleQuote(spot)),
    QuantLib::YieldTermStructureHandle(QuantLib::FlatForward(today, dividend, day_count)),
    QuantLib::YieldTermStructureHandle(QuantLib::FlatForward(today, rate, day_count)),
    QuantLib::BlackVolTermStructureHandle(QuantLib::BlackConstantVol(today, calendar, volatility, day_count))
  )
}
type_of <- function(kind) if (kind == "call") QuantLib::Option_Call_get() else QuantLib::Option_Put_get()
expiry <- function(days) QuantLib::Date(as.integer(QuantLib::Date_serialNumber(today)) + as.integer(days))

draw_inputs <- function(n) {
  data.frame(
    kind = sample(c("call", "put"), n, TRUE), spot = runif(n, 50, 150), rate = runif(n, -0.01, 0.08),
    dividend = runif(n, 0, 0.05), volatility = runif(n, 0.05, 0.8), days = sample(30:3650, n, TRUE)
  )
}
header <- function(path, columns) {
  writeLines(c(paste0("# QuantLib R package ", packageVersion("QuantLib"), "; synthetic inputs; tools/golden/p6_options.R"),
               paste(columns, collapse = ",")), path)
}
append <- function(path, values) {
  write(paste(values, collapse = ","), path, append = TRUE)
}

# European options, with QuantLib's analytic Greeks
path <- file.path(out_dir, "option_european.csv")
header(path, c("kind", "spot", "strike", "rate", "dividend", "volatility", "days", "value", "delta", "gamma", "vega", "rho", "dividend_rho", "theta", "strike_sensitivity"))
inputs <- draw_inputs(300)
inputs$strike <- inputs$spot * exp(rnorm(300, 0, 0.3))
for (i in seq_len(nrow(inputs))) {
  r <- inputs[i, ]
  option <- QuantLib::VanillaOption(QuantLib::PlainVanillaPayoff(type_of(r$kind), r$strike), QuantLib::EuropeanExercise(expiry(r$days)))
  QuantLib::Instrument_setPricingEngine(option, QuantLib::AnalyticEuropeanEngine(process_of(r$spot, r$rate, r$dividend, r$volatility)))
  append(path, c(r$kind, fmt(c(r$spot, r$strike, r$rate, r$dividend, r$volatility)), r$days,
                 fmt(c(option$NPV(), option$delta(), option$gamma(), option$vega(), option$rho(), option$dividendRho(), option$theta(), option$strikeSensitivity()))))
}

# Barrier options
path <- file.path(out_dir, "option_barrier.csv")
header(path, c("kind", "barrier_type", "spot", "strike", "barrier", "rebate", "rate", "dividend", "volatility", "days", "value"))
barrier_types <- list(down_in = QuantLib::Barrier_DownIn_get(), up_in = QuantLib::Barrier_UpIn_get(),
                      down_out = QuantLib::Barrier_DownOut_get(), up_out = QuantLib::Barrier_UpOut_get())
inputs <- draw_inputs(320)
inputs$strike <- inputs$spot * exp(rnorm(320, 0, 0.2))
inputs$barrier_type <- rep(names(barrier_types), length.out = 320)
inputs$barrier <- ifelse(grepl("down", inputs$barrier_type), inputs$spot * runif(320, 0.5, 0.98), inputs$spot * runif(320, 1.02, 1.6))
inputs$rebate <- sample(c(0, 3), 320, TRUE)
for (i in seq_len(nrow(inputs))) {
  r <- inputs[i, ]
  option <- QuantLib::BarrierOption(barrier_types[[r$barrier_type]], r$barrier, r$rebate,
                                    QuantLib::PlainVanillaPayoff(type_of(r$kind), r$strike), QuantLib::EuropeanExercise(expiry(r$days)))
  QuantLib::Instrument_setPricingEngine(option, QuantLib::AnalyticBarrierEngine(process_of(r$spot, r$rate, r$dividend, r$volatility)))
  append(path, c(r$kind, r$barrier_type, fmt(c(r$spot, r$strike, r$barrier, r$rebate, r$rate, r$dividend, r$volatility)), r$days, fmt(option$NPV())))
}

# Cox-Ross-Rubinstein trees
path <- file.path(out_dir, "option_tree.csv")
header(path, c("kind", "exercise", "steps", "spot", "strike", "rate", "dividend", "volatility", "days", "value", "delta", "gamma"))
inputs <- draw_inputs(200)
inputs$volatility <- runif(200, 0.1, 0.8)
inputs$days <- sample(90:1825, 200, TRUE)
inputs$strike <- inputs$spot * exp(rnorm(200, 0, 0.2))
inputs$exercise <- sample(c("american", "european"), 200, TRUE, c(0.75, 0.25))
inputs$steps <- sample(c(25L, 100L, 200L), 200, TRUE)
for (i in seq_len(nrow(inputs))) {
  r <- inputs[i, ]
  exercise <- if (r$exercise == "american") QuantLib::AmericanExercise(today, expiry(r$days)) else QuantLib::EuropeanExercise(expiry(r$days))
  option <- QuantLib::VanillaOption(QuantLib::PlainVanillaPayoff(type_of(r$kind), r$strike), exercise)
  QuantLib::Instrument_setPricingEngine(option, QuantLib::BinomialCRRVanillaEngine(process_of(r$spot, r$rate, r$dividend, r$volatility), r$steps))
  append(path, c(r$kind, r$exercise, r$steps, fmt(c(r$spot, r$strike, r$rate, r$dividend, r$volatility)), r$days, fmt(c(option$NPV(), option$delta(), option$gamma()))))
}

# Black and Bachelier formulas: QuantLib takes the standard deviation
path <- file.path(out_dir, "option_forward.csv")
header(path, c("model", "kind", "forward", "strike", "volatility", "time", "discount", "value"))
n <- 400
for (i in seq_len(n)) {
  model <- if (i %% 2 == 0) "black" else "bachelier"
  kind <- sample(c("call", "put"), 1)
  forward <- runif(1, 0.5, 200)
  strike <- forward * exp(rnorm(1, 0, 0.3))
  time <- runif(1, 0.05, 15)
  discount <- exp(-runif(1, -0.01, 0.06) * time)
  volatility <- if (model == "black") runif(1, 0.03, 1) else forward * runif(1, 0.03, 1)
  value <- if (model == "black") {
    QuantLib::blackFormula(type_of(kind), strike, forward, volatility * sqrt(time), discount)
  } else {
    QuantLib::bachelierBlackFormula(type_of(kind), strike, forward, volatility * sqrt(time), discount)
  }
  append(path, c(model, kind, fmt(c(forward, strike, volatility, time, discount, value))))
}

# Implied volatility: the same search range and accuracy as Riemann's test
path <- file.path(out_dir, "option_implied.csv")
header(path, c("kind", "spot", "strike", "rate", "dividend", "true_volatility", "days", "price", "implied"))
inputs <- draw_inputs(100)
inputs$strike <- inputs$spot * exp(rnorm(100, 0, 0.15))
for (i in seq_len(nrow(inputs))) {
  r <- inputs[i, ]
  process <- process_of(r$spot, r$rate, r$dividend, r$volatility)
  option <- QuantLib::VanillaOption(QuantLib::PlainVanillaPayoff(type_of(r$kind), r$strike), QuantLib::EuropeanExercise(expiry(r$days)))
  QuantLib::Instrument_setPricingEngine(option, QuantLib::AnalyticEuropeanEngine(process))
  price <- option$NPV()
  implied <- option$impliedVolatility(price, process, 1e-10, 200L, 1e-7, 4)
  append(path, c(r$kind, fmt(c(r$spot, r$strike, r$rate, r$dividend, r$volatility)), r$days, fmt(c(price, implied))))
}
