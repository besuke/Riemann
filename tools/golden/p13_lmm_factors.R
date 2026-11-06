# Golden values for the market model with fewer factors, from the QuantLib R package Version 1.42:
# the eigenvalues of the exponential correlation (SymmetricSchurDecomposition), the rank-reduced
# correlation B B^T of the pseudo-root B that rankReducedSqrt(matrix, m, 1.0, SalvagingAlgorithm::None)
# builds, and the spot-measure drifts with that n-by-m pseudo-root (LMMDriftCalculator::computePlain and
# computeReduced, the numeraire at the first alive rate).
#
# The binding does not expose rankReducedSqrt, so its steps (QuantLib v1.42,
# ql/math/matrixutilities/pseudosqrt.cpp) are written out here on QuantLib's own eigen decomposition:
# with componentRetainedPercentage = 1 every component is retained up to maxRank = m; the pseudo-root is
# eigenvectors[, 1:m] * sqrt(eigenvalues[1:m]), each row rescaled to the length sqrt(matrix[i, i])
# (normalizePseudoRoot). All inputs are synthetic; no market data is used.
# Usage (from the Riemann root): Rscript tools/golden/p13_lmm_factors.R
# Writes tests/golden/lmm_factors.csv: case, factors, kind, i, j, value. Kinds: time, forward, volatility,
# beta (inputs, factors 0), eigenvalue (factors 0), correlation (the reduced one), drift_plain and
# drift_reduced (i = alive, j = rate).

suppressMessages(library(QuantLib))

out <- file.path("tests", "golden", "lmm_factors.csv")
set.seed(20261106)
fmt <- function(x) sprintf("%.17g", x)
ref <- function(x) if (inherits(x, "ExternalReference")) methods::slot(x, "ref") else x
# Matrix_dataVector gives the entries by columns (checked on the eigenvectors, which are not symmetric)
read_matrix <- function(m, rows, cols) {
  a <- QuantLib::Matrix_dataVector(m)
  matrix(purrr::map_dbl(seq_len(rows * cols) - 1L, \(k) QuantLib::Array___getitem__(a, k)), rows, cols)
}
to_matrix <- function(x) {
  m <- QuantLib::Matrix(nrow(x), ncol(x), 0)
  purrr::walk(seq_len(nrow(x)), \(i) purrr::walk(seq_len(ncol(x)), \(j) QuantLib::Matrix_setitem(m, i - 1L, j - 1L, x[i, j])))
  m
}

cases <- purrr::map(1:6, \(id) {
  n <- sample(4:9, 1)
  list(id = id, times = cumsum(c(runif(1, 0.25, 2), runif(n, 0.25, 1))), forwards = runif(n, 0.005, 0.06),
       volatilities = runif(n, 0.1, 0.4), beta = runif(1, 0.02, 1.5))
})

rows <- purrr::map(cases, \(cs) {
  n <- length(cs$forwards)
  fixing <- cs$times[seq_len(n)]
  input <- rbind(
    data.frame(factors = 0L, kind = "time", i = seq_along(cs$times) - 1L, j = 0L, value = cs$times),
    data.frame(factors = 0L, kind = "forward", i = seq_len(n) - 1L, j = 0L, value = cs$forwards),
    data.frame(factors = 0L, kind = "volatility", i = seq_len(n) - 1L, j = 0L, value = cs$volatilities),
    data.frame(factors = 0L, kind = "beta", i = 0L, j = 0L, value = cs$beta)
  )
  rho <- read_matrix(QuantLib::PiecewiseConstantCorrelation_correlation(
    QuantLib::ExponentialForwardCorrelation(cs$times, 0.0, cs$beta, 1.0), 0L), n, n)
  schur <- QuantLib::SymmetricSchurDecomposition(to_matrix(rho))
  values <- purrr::map_dbl(seq_len(n) - 1L, \(k) QuantLib::Array___getitem__(QuantLib::SymmetricSchurDecomposition_eigenvalues(schur), k))
  vectors <- read_matrix(QuantLib::SymmetricSchurDecomposition_eigenvectors(schur), n, n)
  stopifnot(values[n] >= -1e-16)
  eigen_rows <- data.frame(factors = 0L, kind = "eigenvalue", i = seq_len(n) - 1L, j = 0L, value = values)

  state <- QuantLib::LMMCurveState(cs$times)
  QuantLib::LMMCurveState_setOnForwardRates(state, cs$forwards, 0L)
  reduced <- purrr::map(unique(c(1L, 2L, 3L, n)), \(m) {
    root <- vectors[, seq_len(m), drop = FALSE] %*% diag(sqrt(values[seq_len(m)]), m)
    root <- root * sqrt(diag(rho) / rowSums(root^2))
    corr <- root %*% t(root)
    grid <- expand.grid(i = seq_len(n) - 1L, j = seq_len(n) - 1L)
    correlation <- data.frame(factors = m, kind = "correlation", i = grid$i, j = grid$j, value = corr[cbind(grid$i + 1L, grid$j + 1L)])
    pseudo <- to_matrix(cs$volatilities * root)
    drift <- purrr::map(seq_len(n) - 1L, \(alive) {
      calculator <- QuantLib::LMMDriftCalculator(pseudo, rep(0, n), diff(cs$times), alive, alive)
      read_drift <- function(entry) {
        dv <- QuantLib::DoubleVector(rep(0, n))
        .Call(entry, ref(calculator), ref(state), dv, PACKAGE = "QuantLib")
        purrr::map_dbl(seq_len(n) - 1L, \(k) .Call("R_swig_DoubleVector___getitem__", dv, k, FALSE, PACKAGE = "QuantLib"))
      }
      rbind(
        data.frame(factors = m, kind = "drift_plain", i = alive, j = seq_len(n) - 1L, value = read_drift("R_swig_LMMDriftCalculator_computePlain__SWIG_0")),
        data.frame(factors = m, kind = "drift_reduced", i = alive, j = seq_len(n) - 1L, value = read_drift("R_swig_LMMDriftCalculator_computeReduced__SWIG_0"))
      )
    }) |> purrr::list_rbind()
    rbind(correlation, drift)
  }) |> purrr::list_rbind()
  cbind(case = cs$id, rbind(input, eigen_rows, reduced))
}) |> purrr::list_rbind()

writeLines(c(paste0("# QuantLib R package ", packageVersion("QuantLib"), "; synthetic inputs; tools/golden/p13_lmm_factors.R"),
             "case,factors,kind,i,j,value"), out)
write(sprintf("%d,%d,%s,%d,%d,%s", rows$case, rows$factors, rows$kind, rows$i, rows$j, fmt(rows$value)), out, append = TRUE)
cat(length(cases), "cases,", nrow(rows), "rows\n")
