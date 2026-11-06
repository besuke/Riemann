# Golden values for the forward market model from the QuantLib R package Version 1.42 (market models
# framework): the exponential correlation (ExponentialForwardCorrelation with a long-term correlation of 0
# and gamma 1), the spot-measure drift (LMMDriftCalculator::computePlain with the numeraire at the first
# alive rate, and the terminal-measure drift with the numeraire at the last time), and swap rates and
# discount ratios (LMMCurveState). QuantLib's LiborForwardModel and the
# market-model evolvers are not exposed by the R binding, so the paths themselves are not compared.
# All inputs are synthetic; no market data is used.
# Usage (from the Riemann root): Rscript tools/golden/p13_lmm_market.R
# Writes tests/golden/lmm_market.csv: case, kind, i, j, value. Kinds: time, forward, volatility, beta
# (inputs), correlation, drift and drift_terminal (i = alive, j = rate), swap_rate (i = start, j = end), discount_ratio.
#
# The binding's R wrapper of computePlain converts its output argument to a numeric copy, so the drifts are
# read by calling the package's own C entry with a DoubleVector.

suppressMessages(library(QuantLib))

out <- file.path("tests", "golden", "lmm_market.csv")
set.seed(20261105)
fmt <- function(x) sprintf("%.17g", x)
ref <- function(x) if (inherits(x, "ExternalReference")) methods::slot(x, "ref") else x

cases <- purrr::map(1:6, \(id) {
  n <- sample(3:8, 1)
  list(id = id, times = cumsum(c(runif(1, 0.25, 2), runif(n, 0.25, 1))), forwards = runif(n, 0.005, 0.06),
       volatilities = runif(n, 0.1, 0.4), beta = runif(1, 0.02, 1.5))
})

rows <- purrr::map(cases, \(cs) {
  n <- length(cs$forwards)
  input <- rbind(
    data.frame(kind = "time", i = seq_along(cs$times) - 1L, j = 0L, value = cs$times),
    data.frame(kind = "forward", i = seq_len(n) - 1L, j = 0L, value = cs$forwards),
    data.frame(kind = "volatility", i = seq_len(n) - 1L, j = 0L, value = cs$volatilities),
    data.frame(kind = "beta", i = 0L, j = 0L, value = cs$beta)
  )
  m <- QuantLib::PiecewiseConstantCorrelation_correlation(QuantLib::ExponentialForwardCorrelation(cs$times, 0.0, cs$beta, 1.0), 0L)
  a <- QuantLib::Matrix_dataVector(m)
  rho <- matrix(purrr::map_dbl(seq_len(n * n) - 1L, \(k) QuantLib::Array___getitem__(a, k)), n, n, byrow = TRUE)
  correlation <- expand.grid(i = seq_len(n) - 1L, j = seq_len(n) - 1L)
  correlation <- data.frame(kind = "correlation", i = correlation$i, j = correlation$j, value = rho[cbind(correlation$i + 1L, correlation$j + 1L)])

  lower <- t(chol(outer(cs$volatilities, cs$volatilities) * rho))
  pseudo <- QuantLib::Matrix(n, n, 0)
  purrr::walk(seq_len(n), \(i) purrr::walk(seq_len(n), \(j) QuantLib::Matrix_setitem(pseudo, i - 1L, j - 1L, lower[i, j])))
  state <- QuantLib::LMMCurveState(cs$times)
  QuantLib::LMMCurveState_setOnForwardRates(state, cs$forwards, 0L)
  drift <- purrr::map(seq_len(n) - 1L, \(alive) {
    calculator <- QuantLib::LMMDriftCalculator(pseudo, rep(0, n), diff(cs$times), alive, alive)
    out <- QuantLib::DoubleVector(rep(0, n))
    .Call("R_swig_LMMDriftCalculator_computePlain__SWIG_0", ref(calculator), ref(state), out, PACKAGE = "QuantLib")
    values <- purrr::map_dbl(seq_len(n) - 1L, \(k) .Call("R_swig_DoubleVector___getitem__", out, k, FALSE, PACKAGE = "QuantLib"))
    data.frame(kind = "drift", i = alive, j = seq_len(n) - 1L, value = values)
  }) |> purrr::list_rbind()

  spans <- subset(expand.grid(i = 0:(n - 1L), j = 1:n), i < j)
  swap <- data.frame(kind = "swap_rate", i = spans$i, j = spans$j,
                     value = purrr::map2_dbl(spans$i, spans$j, \(a, b) QuantLib::CurveState_swapRate(state, a, b)))
  ratio <- data.frame(kind = "discount_ratio", i = spans$i, j = spans$j,
                      value = purrr::map2_dbl(spans$i, spans$j, \(a, b) QuantLib::CurveState_discountRatio(state, a, b)))
  # the terminal measure: the numeraire is the bond to the last time (numeraire = n)
  terminal <- purrr::map(seq_len(n) - 1L, \(alive) {
    calculator <- QuantLib::LMMDriftCalculator(pseudo, rep(0, n), diff(cs$times), n, alive)
    out <- QuantLib::DoubleVector(rep(0, n))
    .Call("R_swig_LMMDriftCalculator_computePlain__SWIG_0", ref(calculator), ref(state), out, PACKAGE = "QuantLib")
    values <- purrr::map_dbl(seq_len(n) - 1L, \(k) .Call("R_swig_DoubleVector___getitem__", out, k, FALSE, PACKAGE = "QuantLib"))
    data.frame(kind = "drift_terminal", i = alive, j = seq_len(n) - 1L, value = values)
  }) |> purrr::list_rbind()
  cbind(case = cs$id, rbind(input, correlation, drift, swap, ratio, terminal))
}) |> purrr::list_rbind()

writeLines(c(paste0("# QuantLib R package ", packageVersion("QuantLib"), "; synthetic inputs; tools/golden/p13_lmm_market.R"),
             "case,kind,i,j,value"), out)
write(sprintf("%d,%s,%d,%d,%s", rows$case, rows$kind, rows$i, rows$j, fmt(rows$value)), out, append = TRUE)
cat(length(cases), "cases,", nrow(rows), "rows\n")
