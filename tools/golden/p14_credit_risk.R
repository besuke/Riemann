# Golden values for the one-factor Gaussian credit portfolio (LagrangeFinance I-10; docs/spec_credit_risk.md).
#
# Usage (from the rmql root): Rscript tools/golden/p14_credit_risk.R
# Writes tests/golden/credit_risk.csv: case, kind, i, j, value.
#
# The manuscript's portfolio (I-10.5): 100 obligors, PD from 0.5% to 5%, EAD 100, LGD 60%, rho 0.20.
# References, all in R:
#   conditional PD             pnorm((qnorm(PD) - sqrt(rho) z) / sqrt(1 - rho)) (I-10.4, I-10.6)
#   exact loss distribution    given Z = z the defaults are independent: the distribution of the number of loss
#                              units by recursion over the obligors, integrated over z by the trapezoidal rule on
#                              [-12, 12] with step 1/1000 (the integrand is smooth and decays like a Gaussian, so the
#                              rule converges faster than any power of the step). rmql uses Gauss-Hermite
#                              quadrature instead: an independent method.
#   VaR and ES of it           VaR = min{l : P(L <= l) >= alpha}; ES of Rockafellar and Uryasev for a discrete law, and
#                              E[L | L >= VaR] (the manuscript's tail mean, I-10.11)
#   the manuscript's Monte Carlo  its chunks as written (furrr, 20 x 5000 scenarios, seed 20260918; and 20000 per rho
#                              for the correlation sensitivity, I-10.12): another random stream, compared within errors
#   ASRF                       sum EAD LGD pnorm((qnorm(PD) + sqrt(rho) qnorm(alpha)) / sqrt(1 - rho))
#   Basel IRB, corporates      the correlation R(PD), maturity adjustment b(PD) and capital K (Basel framework CRE31)

suppressPackageStartupMessages({
  library(dplyr)
  library(purrr)
})

out <- file.path("tests", "golden", "credit_risk.csv")
fmt <- function(x) sprintf("%.17g", x)

n_obligors <- 100L
pd <- seq(0.005, 0.05, length.out = n_obligors)
ead <- rep(100, n_obligors); lgd <- rep(0.60, n_obligors)

# ---- exact distribution: units of loss per obligor, recursion given z, trapezoid over z -------------------------
exact_distribution <- function(pd, ead, lgd, rho, unit) {
  units <- round(ead * lgd / unit); stopifnot(all(abs(units * unit - ead * lgd) < 1e-9))
  h <- 1e-3; z <- seq(-12, 12, by = h)
  wz <- stats::dnorm(z) * h; wz[c(1, length(wz))] <- wz[c(1, length(wz))] / 2
  states <- sum(units) + 1L
  dist <- matrix(0, length(z), states); dist[, 1] <- 1
  for (i in seq_along(pd)) {
    p <- stats::pnorm((stats::qnorm(pd[i]) - sqrt(rho) * z) / sqrt(1 - rho))
    shifted <- cbind(matrix(0, length(z), units[i]), dist[, seq_len(states - units[i]), drop = FALSE])
    dist <- dist * (1 - p) + shifted * p
  }
  probability <- as.numeric(crossprod(wz, dist))
  tibble(loss = (seq_len(states) - 1L) * unit, probability = probability)
}
measures <- function(d, alpha) {
  cum <- cumsum(d$probability)
  k <- which(cum >= alpha - 1e-15)[1]
  var <- d$loss[k]
  above <- sum(d$loss[d$loss > var] * d$probability[d$loss > var])
  at_or_above <- d$loss >= var
  c(var = var, es = (above + var * (cum[k] - alpha)) / (1 - alpha),
    tail_mean_es = sum(d$loss[at_or_above] * d$probability[at_or_above]) / sum(d$probability[at_or_above]))
}

rows <- list()
add <- function(case, kind, i, j, value) rows[[length(rows) + 1L]] <<- tibble(case = case, kind = kind, i = as.integer(i), j = as.character(j), value = as.numeric(value))

add("lf_i10", "pd", seq_along(pd) - 1L, "", pd)
add("lf_i10", "parameter", 0L, c("ead", "lgd", "rho", "unit"), c(100, 0.6, 0.2, 60))
add("lf_i10", "measure", 0L, "expected_loss", sum(pd * ead * lgd))

# conditional PD of the representative obligor (I-10.6)
z <- seq(-4, 4, length.out = 9)
add("lf_i10", "conditional_pd", seq_along(z) - 1L, fmt(z), stats::pnorm((stats::qnorm(0.02) - sqrt(0.2) * z) / sqrt(0.8)))

walk(c(0, 0.1, 0.2, 0.4), \(rho) {
  d <- exact_distribution(pd, ead, lgd, rho, 60)
  add("lf_i10", "distribution", seq_len(nrow(d)) - 1L, paste0("rho=", rho), d$probability)
  mean_loss <- sum(d$loss * d$probability); sd_loss <- sqrt(sum((d$loss - mean_loss)^2 * d$probability))
  m <- map(c(0.95, 0.99, 0.999), \(a) measures(d, a))
  add("lf_i10", "exact", 0L, paste0(c("mean", "sd", "total_probability", "var@0.95", "es@0.95", "tail_mean_es@0.95", "var@0.99", "es@0.99", "tail_mean_es@0.99",
             "var@0.999", "es@0.999", "tail_mean_es@0.999"), ":rho=", rho),
      c(mean_loss, sd_loss, sum(d$probability), unlist(m)))
  add("lf_i10", "asrf", 0L, paste0(c("var@0.99", "var@0.999"), ":rho=", rho),
      map_dbl(c(0.99, 0.999), \(a) sum(ead * lgd * stats::pnorm((stats::qnorm(pd) + sqrt(rho) * stats::qnorm(a)) / sqrt(1 - rho)))))
})

# ---- the manuscript's Monte Carlo (I-10.7 to I-10.12), as written, on one worker -------------------------------
future::plan(future::sequential)
set.seed(20260918)
n_scenarios <- 100000L; n_batches <- 20L; n_per_batch <- n_scenarios / n_batches
vec_rho <- rep(0.2, n_obligors)
losses <- furrr::future_map(seq_len(n_batches), \(batch_id) purrr::map_dbl(seq_len(n_per_batch), \(scenario_id) {
  systematic_factor <- rnorm(1); vec_idiosyncratic <- rnorm(n_obligors)
  latent <- sqrt(vec_rho) * systematic_factor + sqrt(1 - vec_rho) * vec_idiosyncratic
  sum(ead * lgd * (latent < qnorm(pd)))
}), .options = furrr::furrr_options(seed = TRUE)) |> unlist(use.names = FALSE)
mc_var <- map_dbl(c(0.95, 0.99, 0.999), \(a) quantile(losses, a, names = FALSE, type = 7))
mc_es <- map2_dbl(c(0.95, 0.99, 0.999), mc_var, \(a, v) mean(losses[losses >= v]))
add("lf_i10", "manuscript_mc", 0L, c("mean", "sd", "var@0.95", "var@0.99", "var@0.999", "es@0.95", "es@0.99", "es@0.999"),
    c(mean(losses), stats::sd(losses), mc_var, mc_es))

set.seed(20260918)
sensitivity <- map(c(0, 0.10, 0.20, 0.40), \(rho_value) {
  l <- purrr::map_dbl(seq_len(20000L), \(scenario_id) {
    systematic_factor <- rnorm(1); vec_idiosyncratic <- rnorm(n_obligors)
    latent <- sqrt(rho_value) * systematic_factor + sqrt(1 - rho_value) * vec_idiosyncratic
    sum(ead * lgd * (latent < qnorm(pd)))
  })
  c(mean(l), quantile(l, 0.99, names = FALSE), quantile(l, 0.999, names = FALSE))
})
walk2(sensitivity, c(0, 0.1, 0.2, 0.4), \(x, rho) add("lf_i10", "manuscript_sensitivity", 0L, paste0(c("mean", "var@0.99", "var@0.999"), ":rho=", rho), x))

# ---- Basel IRB, corporates (CRE31) --------------------------------------------------------------------------------
irb <- function(pd, lgd, m) {
  f <- (1 - exp(-50 * pd)) / (1 - exp(-50))
  r <- 0.12 * f + 0.24 * (1 - f)
  b <- (0.11852 - 0.05478 * log(pd))^2
  k <- (lgd * stats::pnorm((stats::qnorm(pd) + sqrt(r) * stats::qnorm(0.999)) / sqrt(1 - r)) - pd * lgd) * (1 + (m - 2.5) * b) / (1 - 1.5 * b)
  cbind(correlation = r, maturity_adjustment = b, capital = k)
}
irb_pd <- c(0.0003, 0.001, 0.005, 0.01, 0.02, 0.05, 0.1, 0.2)
x <- irb(irb_pd, 0.45, 2.5)
add("irb", "input", seq_along(irb_pd) - 1L, "pd", irb_pd)
iwalk(colnames(x), \(nm, k) add("irb", "output", seq_along(irb_pd) - 1L, nm, x[, nm]))
add("irb", "output", seq_along(irb_pd) - 1L, "capital_m5", irb(irb_pd, 0.45, 5)[, "capital"])

all_rows <- bind_rows(rows)
dir.create(dirname(out), recursive = TRUE, showWarnings = FALSE)
writeLines(c(
  "# One-factor Gaussian credit portfolio: references in R (tools/golden/p14_credit_risk.R)",
  sprintf("# R %s, furrr %s, future %s", getRversion(), packageVersion("furrr"), packageVersion("future")),
  "case,kind,i,j,value",
  sprintf("%s,%s,%d,%s,%s", all_rows$case, all_rows$kind, all_rows$i, all_rows$j, fmt(all_rows$value))
), out)
message(sprintf("%d rows -> %s", nrow(all_rows), out))
print(all_rows |> filter(kind %in% c("measure", "exact", "asrf", "manuscript_mc", "manuscript_sensitivity")) |> select(kind, j, value), n = 80)
