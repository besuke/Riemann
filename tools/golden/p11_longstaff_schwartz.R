# Golden values for P11-4, in three layers, so that a difference can be placed: the Sobol sequence, the
# paths built from it, and the Longstaff-Schwartz values of QuantLib's low-discrepancy American engine.
# All inputs are synthetic.
# Usage (from the Riemann root): Rscript tools/golden/p11_longstaff_schwartz.R
# Writes tests/golden/ls_sobol.csv, ls_paths.csv and ls_american.csv.

suppressMessages(library(QuantLib))

out_dir <- file.path("tests", "golden")
fmt <- function(x) sprintf("%.17g", x)
version <- paste0("# QuantLib R package ", packageVersion("QuantLib"), "; synthetic inputs; tools/golden/p11_longstaff_schwartz.R")
today <- QuantLib::Date(2L, "November", 2026L)
invisible(QuantLib::Settings_instance()$setEvaluationDate(today))
today_serial <- as.integer(QuantLib::Date_serialNumber(today))
day_count <- QuantLib::Actual365Fixed()
calendar <- QuantLib::NullCalendar()
flat <- function(rate) QuantLib::YieldTermStructureHandle(QuantLib::FlatForward(today, rate, day_count, "Continuous"))
process_of <- function(spot, rate, dividend, volatility) {
  QuantLib::BlackScholesMertonProcess(
    QuantLib::QuoteHandle(QuantLib::SimpleQuote(spot)), flat(dividend), flat(rate),
    QuantLib::BlackVolTermStructureHandle(QuantLib::BlackConstantVol(today, calendar, volatility, day_count))
  )
}

# ---- 1. the Sobol sequence: QuantLib's default direction integers (Jaeckel) ---------------------------
path <- file.path(out_dir, "ls_sobol.csv")
writeLines(c(version, "dimension,index,points"), path)
for (dimension in c(1L, 2L, 3L, 8L, 9L, 12L, 32L)) {
  rsg <- QuantLib::SobolRsg(dimension)
  for (index in 1:512) {
    sample <- QuantLib::SobolRsg_nextSequence(rsg)
    values <- vapply(seq_len(dimension), function(k) QuantLib::SampleRealVector_value(sample)[[k]], 1)
    write(paste(c(dimension, index, paste(fmt(values), collapse = ";")), collapse = ","), path, append = TRUE)
  }
}

# ---- 1b. what the seed means ------------------------------------------------------------------------------
# sobolrsg.cpp uses the seed only to draw the direction integers of the dimensions beyond the table
# (32 for Jaeckel). It is not a start position or a skip. Checked here by the points themselves.
first_points <- function(make, n = 64L) t(vapply(seq_len(n), function(i) make(), numeric(environment(make)$dimension)))
sobol_points <- function(dimension, seed) {
  rsg <- QuantLib::SobolRsg(dimension, seed)
  make <- function() QuantLib::SampleRealVector_value(QuantLib::SobolRsg_nextSequence(rsg))
  environment(make)$dimension <- dimension
  first_points(make)
}
seed_check <- c(
  table_12_seed_0_vs_1 = identical(sobol_points(12L, 0L), sobol_points(12L, 1L)),
  table_12_seed_0_vs_12345 = identical(sobol_points(12L, 0L), sobol_points(12L, 12345L)),
  beyond_40_first_32_seed_0_vs_1 = identical(sobol_points(40L, 0L)[, 1:32], sobol_points(40L, 1L)[, 1:32]),
  beyond_40_last_8_seed_0_vs_1 = identical(sobol_points(40L, 0L)[, 33:40], sobol_points(40L, 1L)[, 33:40])
)
print(seed_check)
stopifnot(seed_check[1:3], !seed_check[4])
# LagrangeFinance's generator: UniformLowDiscrepancySequenceGenerator(12, seed = 1), then the inverse normal
uniform <- QuantLib::UniformLowDiscrepancySequenceGenerator(12L, 1L)
gaussian <- QuantLib::GaussianLowDiscrepancySequenceGenerator(QuantLib::UniformLowDiscrepancySequenceGenerator(12L, 1L))
path <- file.path(out_dir, "ls_normals.csv")
writeLines(c(version, "index,uniform,normal"), path)
for (index in 1:64) {
  u <- QuantLib::SampleRealVector_value(QuantLib::UniformLowDiscrepancySequenceGenerator_nextSequence(uniform))
  z <- QuantLib::SampleRealVector_value(QuantLib::GaussianLowDiscrepancySequenceGenerator_nextSequence(gaussian))
  write(paste(c(index, paste(fmt(u), collapse = ";"), paste(fmt(z), collapse = ";")), collapse = ","), path, append = TRUE)
}
stopifnot(identical(as.matrix(read.csv(path, comment.char = "#")$uniform[1:3]), as.matrix(read.csv(path, comment.char = "#")$uniform[1:3])))

# ---- 2. paths: GaussianSobolPathGenerator, with and without the Brownian bridge ---------------------
path <- file.path(out_dir, "ls_paths.csv")
writeLines(c(version, "case,spot,rate,dividend,volatility,time,steps,bridge,path,prices"), path)
case <- 0L
for (steps in c(1L, 5L, 12L, 32L)) {
  for (bridge in c(FALSE, TRUE)) {
    case <- case + 1L
    spot <- 100; rate <- 0.03 + 0.01 * case %% 3; dividend <- 0.01 * (case %% 2); volatility <- 0.15 + 0.02 * case; time <- 0.5 + 0.25 * case
    generator <- QuantLib::GaussianSobolPathGenerator(
      process_of(spot, rate, dividend, volatility), time, steps,
      QuantLib::GaussianLowDiscrepancySequenceGenerator(QuantLib::UniformLowDiscrepancySequenceGenerator(steps, 1L)), bridge
    )
    for (p in 1:128) {
      sample <- generator$`next`()$value()
      prices <- vapply(0:steps, function(i) as.numeric(sample$value(i)), 1)
      write(paste(c(case, fmt(c(spot, rate, dividend, volatility, time)), steps, as.integer(bridge), p, paste(fmt(prices), collapse = ";")), collapse = ","), path, append = TRUE)
    }
  }
}

# LagrangeFinance II-4.6.1 exactly: spot 100, rate 0.05, no dividend, volatility 0.2, 367 days, 12 steps, seed 1
case <- case + 1L
generator <- QuantLib::GaussianSobolPathGenerator(
  process_of(100, 0.05, 0, 0.2), 367 / 365, 12L,
  QuantLib::GaussianLowDiscrepancySequenceGenerator(QuantLib::UniformLowDiscrepancySequenceGenerator(12L, 1L)), FALSE
)
for (p in 1:64) {
  sample <- generator$`next`()$value()
  prices <- vapply(0:12, function(i) as.numeric(sample$value(i)), 1)
  write(paste(c(case, fmt(c(100, 0.05, 0, 0.2, 367 / 365)), 12L, 0L, p, paste(fmt(prices), collapse = ";")), collapse = ","), path, append = TRUE)
}

# ---- 3. Longstaff-Schwartz: MCLDAmericanEngine ------------------------------------------------------------
# The engine regresses on the first nCalibrationSamples points and prices requiredSamples points from the
# start of the same sequence. Its basis is the monomials of S / K up to polynomOrder, plus the payoff.
# It passes no Brownian bridge (mcamericanengine.hpp). NA reaches QuantLib as Null.
path <- file.path(out_dir, "ls_american.csv")
writeLines(c(version, "case,type,spot,strike,rate,dividend,volatility,days,steps,order,calibration,samples,value"), path)
set.seed(20261103)
case <- 0L
for (i in 1:40) {
  case <- case + 1L
  type <- if (i %% 4 == 0) "Call" else "Put"
  spot <- round(runif(1, 80, 120), 2); strike <- 100; rate <- round(runif(1, 0.01, 0.08), 4)
  dividend <- if (type == "Call") round(runif(1, 0.03, 0.09), 4) else round(runif(1, 0, 0.03), 4)
  volatility <- round(runif(1, 0.1, 0.45), 4); days <- sample(c(91L, 182L, 365L, 730L), 1)
  steps <- sample(c(4L, 12L, 25L, 32L), 1); order <- sample(0:4, 1)
  calibration <- sample(c(512L, 2048L, 4096L), 1); samples <- sample(c(1024L, 4096L, 8192L), 1)
  maturity <- QuantLib::Date(today_serial + days)
  option <- QuantLib::VanillaOption(QuantLib::PlainVanillaPayoff(type, strike), QuantLib::AmericanExercise(today, maturity))
  engine <- QuantLib::MCLDAmericanEngine(
    process_of(spot, rate, dividend, volatility), steps, NA_integer_, FALSE, FALSE, samples, NA_real_, NA_integer_, 0L,
    order, QuantLib::LsmBasisSystem_Monomial_get(), calibration
  )
  QuantLib::Instrument_setPricingEngine(option, engine)
  value <- option$NPV()
  # the engine seed: within the table it must change nothing. Given alone, it also moves the calibration
  # seed (to seed + 1768237423, mclongstaffschwartzengine.hpp), so both are checked by this one change.
  other_seed <- QuantLib::MCLDAmericanEngine(
    process_of(spot, rate, dividend, volatility), steps, NA_integer_, FALSE, FALSE, samples, NA_real_, NA_integer_, 42L,
    order, QuantLib::LsmBasisSystem_Monomial_get(), calibration
  )
  QuantLib::Instrument_setPricingEngine(option, other_seed)
  stopifnot(identical(option$NPV(), value))
  # NA reaches QuantLib as Null: the number of samples must change the value
  if (i == 1L) {
    fewer <- QuantLib::MCLDAmericanEngine(
      process_of(spot, rate, dividend, volatility), steps, NA_integer_, FALSE, FALSE, samples %/% 2L, NA_real_, NA_integer_, 0L,
      order, QuantLib::LsmBasisSystem_Monomial_get(), calibration
    )
    QuantLib::Instrument_setPricingEngine(option, fewer)
    stopifnot(option$NPV() != value)
  }
  write(paste(c(case, tolower(type), fmt(c(spot, strike, rate, dividend, volatility)), days, steps, order, calibration, samples, fmt(value)), collapse = ","), path, append = TRUE)
  cat(sprintf("%2d %-4s S=%6.2f r=%.4f q=%.4f v=%.4f days=%3d steps=%2d order=%d cal=%4d n=%4d value=%.10f\n",
              case, type, spot, rate, dividend, volatility, days, steps, order, calibration, samples, value))
}
