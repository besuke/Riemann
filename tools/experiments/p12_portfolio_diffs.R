# The differences of RiemannFinMath's portfolio functions from the references of tests/golden/portfolio.csv
# (tools/golden/p12_portfolio.R), for docs/spec_portfolio.md. Uses the installed RiemannFinMath.
# Usage (from the Riemann root): Rscript tools/experiments/p12_portfolio_diffs.R

suppressPackageStartupMessages({
  library(dplyr)
  library(purrr)
  library(RiemannFinMath)
})

g <- readr::read_csv(file.path("tests", "golden", "portfolio.csv"), comment = "#", show_col_types = FALSE,
                     col_types = readr::cols(j = readr::col_character(), value = readr::col_character())) |>
  mutate(value = as.numeric(value))

case_inputs <- function(id) {
  d <- filter(g, case == id)
  ret <- filter(d, kind == "return")
  n <- length(unique(ret$j))
  r <- matrix(NA_real_, max(ret$i) + 1, n)
  r[cbind(ret$i + 1, as.integer(ret$j) + 1)] <- ret$value
  colnames(r) <- paste0("a", seq_len(n))
  p <- d |> filter(kind == "parameter") |> (\(x) setNames(x$value, x$j))()
  list(d = d, r = r, p = p, constraints = create_portfolio_constraints(colnames(r), lower = filter(d, kind == "lower")$value,
                                                                      upper = filter(d, kind == "upper")$value))
}

vec <- function(d, k, name) d |> filter(kind == k, j == name) |> arrange(i) |> pull(value)

rows <- map(unique(g$case), \(id) {
  x <- case_inputs(id)
  bounded <- any(is.finite(c(x$constraints$lower, x$constraints$upper)))
  methods <- c("minimum_variance", "mean_variance", "maximum_sharpe", "target_return", "risk_budget",
               if (bounded) c("maximum_return", "minimum_ES", "target_risk"))
  map(methods, \(m) {
    p <- compute_optimal_portfolio(x$r, x$constraints, m, risk_aversion = x$p[["risk_aversion"]], risk_free_rate = x$p[["risk_free_rate"]],
                                   confidence_level = x$p[["confidence_level"]], target_return = x$p[["target_return"]],
                                   target_volatility = x$p[["target_volatility"]])
    w <- p$weights$weight
    ref <- vec(x$d, "weight", m)
    es_ref <- if (m == "minimum_ES") x$p[["minimum_es_value"]] else NA
    risk_rc <- compute_portfolio_risk(x$r, ref, x$p[["confidence_level"]])
    contribution_diff <- max(map_dbl(c("volatility", "normal_var", "normal_es", "historical_es"), \(k)
      max(abs(risk_rc$contributions[[k]] - vec(x$d, "contribution", paste(m, k, sep = ":"))))))
    tibble(case = id, method = m,
           weight_diff = if (m == "minimum_ES") NA else max(abs(w - ref)),
           es_value_diff = if (m == "minimum_ES") abs(compute_portfolio_risk(x$r, w, x$p[["confidence_level"]])$statistics$value[6] - es_ref) else NA,
           contribution_diff = contribution_diff, stationarity_residual = p$stationarity_residual, iterations = p$iterations, solver = p$solver)
  }) |> list_rbind()
}) |> list_rbind()

print(rows, n = Inf, width = 200)
cat("\nmax weight diff", max(rows$weight_diff, na.rm = TRUE), " max ES value diff", max(rows$es_value_diff, na.rm = TRUE),
    " max contribution diff", max(rows$contribution_diff), " max residual", max(rows$stationarity_residual), "\n")

# GaussQuant on LagrangeFinance I-8
x <- case_inputs("lf_i8")
gq <- function(name, stat) x$d |> filter(kind == "gq_value", j == paste(name, stat, sep = ":")) |> pull(value)
one <- function(m, ...) compute_optimal_portfolio(x$r, x$constraints, m, ...)
s <- function(p, k) p$statistics$value[p$statistics$metric == k]
gq_rows <- tibble(
  method = c("minimum_variance", "mean_variance", "maximum_return", "target_return"),
  weight_diff_from_gaussquant = map_dbl(method, \(m) {
    p <- one(m, risk_aversion = 10, target_return = x$p[["target_return"]])
    max(abs(p$weights$weight - vec(x$d, "gq_weight", m)))
  })
)
print(gq_rows)
sharpe <- one("maximum_sharpe"); es <- one("minimum_ES", confidence_level = 0.95); tr <- one("target_risk", target_volatility = x$p[["target_volatility"]])
rb <- one("risk_budget")
es_gq_weights <- compute_portfolio_risk(x$r, vec(x$d, "gq_weight", "minimum_ES"), 0.95)$statistics
rb_gq <- vec(x$d, "gq_weight", "risk_budget"); m <- compute_return_moments(x$r)
rc_gq <- rb_gq * as.numeric(m$covariance %*% rb_gq); rc_gq <- rc_gq / sum(rc_gq)
cat(sprintf("max Sharpe: RiemannFinMath %.6f, GaussQuant %.6f\n", s(sharpe, "sharpe_ratio"), gq("maximum_sharpe", "sharpe_ratio")))
cat(sprintf("min ES (Rockafellar-Uryasev): RiemannFinMath %.6f, at GaussQuant's weights %.6f; mean of losses at or above VaR: %.6f and %.6f\n",
            s(es, "historical_es"), es_gq_weights$value[6], s(es, "tail_mean_es"), es_gq_weights$value[7]))
cat(sprintf("risk budget: largest deviation of the risk shares from 1/20: RiemannFinMath %.2e, GaussQuant %.2e\n",
            max(abs(rb$weights$weight * as.numeric(m$covariance %*% rb$weights$weight) / s(rb, "variance") - 1 / 20)), max(abs(rc_gq - 1 / 20))))
cat(sprintf("target risk %.6f: RiemannFinMath return %.6f at volatility %.6f; GaussQuant return %.6f at volatility %.6f\n",
            x$p[["target_volatility"]], s(tr, "expected_return"), s(tr, "volatility"), gq("target_risk", "expected_return"), gq("target_risk", "volatility")))
cat("periods", nrow(x$r), "assets", ncol(x$r), "\n")
