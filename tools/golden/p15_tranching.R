# Golden values for credit tranches and CMO waterfalls (LagrangeFinance I-11; docs/spec_tranching.md).
#
# Usage (from the rmql root): Rscript tools/golden/p15_tranching.R
# Writes tests/golden/tranching.csv: case, kind, i, j, value.
#
# Credit tranches on I-10's portfolio (100 obligors, PD 0.5% to 5%, EAD 100, LGD 60%, rho 0.20; loss rate
# l = L / 10000), tranches 0-3%, 3-7%, 7-10%, 10-100% (I-11.8):
#   tranche loss               min(max(l - A, 0), D - A) (the manuscript's tranche_loss_LFL, I-11.9)
#   expected tranche loss      over the exact loss distribution of P14 (trapezoidal rule in z, recursion over the
#                              obligors), with the probabilities that the tranche is hit and wiped out, and the
#                              tranche's VaR 99%
#   large homogeneous pool     E[min(max(LGD p(Z) - A, 0), D - A)] with p(Z) the conditional PD, by integrate() split at
#                              the kinks where LGD p(z) crosses A and D
#   legs and fair spread       I-11.12: each obligor's PD at t is 1 - exp(-h t) with h = -log(1 - PD) (so the
#                              one-year PD is the manuscript's); quarterly dates to 5 years, discount exp(-0.01 t);
#                              premium leg per unit spread sum DF(t) E[outstanding_t] dt, loss leg sum DF(t) E[dL(t)]
# CMO on a pass-through pool (P11-5: 1000, mortgage 5.5%, pass-through 5%, 360 months, refinancing rate 5.5% so
# that CPR is the PSA rule), its collateral cash flows from rmql::compute_mbs_cashflows():
#   sequential A, B, C and Z   balances 400, 300, 200, 100, coupons 5%; Z accrues while A, B or C is outstanding
#   PAC and support            schedule = the smaller principal of the pool at 100% and 300% PSA each month;
#                              PAC 700 and support 300 at 5%, run at 50%, 150% and 400% PSA
# The waterfalls are written here as plain loops, independently of rmql's.

suppressPackageStartupMessages({
  library(dplyr)
  library(purrr)
  library(rmql)
})

out <- file.path("tests", "golden", "tranching.csv")
fmt <- function(x) sprintf("%.17g", x)
rows <- list()
add <- function(case, kind, i, j, value) rows[[length(rows) + 1L]] <<- tibble(case = case, kind = kind, i = as.integer(i), j = as.character(j), value = as.numeric(value))

tranche_loss <- function(l, a, d) pmin(pmax(l - a, 0), d - a)
tranches <- tibble(name = c("equity", "mezzanine", "junior", "senior"), a = c(0, 0.03, 0.07, 0.10), d = c(0.03, 0.07, 0.10, 1))
add("tranches", "attachment", seq_len(4) - 1L, tranches$name, tranches$a)
add("tranches", "detachment", seq_len(4) - 1L, tranches$name, tranches$d)

# the manuscript's example (I-11.9)
l <- seq(0, 0.15, by = 0.001)
pwalk(tranches, \(name, a, d) add("example", "tranche_loss", seq_along(l) - 1L, name, tranche_loss(l, a, d)))

# ---- exact distribution (as tools/golden/p14_credit_risk.R) -----------------------------------------------------
n_obligors <- 100L
pd <- seq(0.005, 0.05, length.out = n_obligors)
exact_distribution <- function(pd, rho, h = 2e-3) {
  z <- seq(-12, 12, by = h); wz <- stats::dnorm(z) * h; wz[c(1, length(wz))] <- wz[c(1, length(wz))] / 2
  dist <- matrix(0, length(z), length(pd) + 1L); dist[, 1] <- 1
  for (i in seq_along(pd)) {
    p <- stats::pnorm((stats::qnorm(pd[i]) - sqrt(rho) * z) / sqrt(1 - rho))
    dist <- dist * (1 - p) + cbind(0, dist[, -ncol(dist), drop = FALSE]) * p
  }
  as.numeric(crossprod(wz, dist))  # P(k defaults); loss rate 60 k / 10000
}
rate <- 60 * (0:n_obligors) / 10000

walk(c(0.1, 0.2, 0.4), \(rho) {
  p <- exact_distribution(pd, rho)
  pwalk(tranches, \(name, a, d) {
    tl <- tranche_loss(rate, a, d); cum <- cumsum(p)
    var99 <- tl[which(cum >= 0.99 - 1e-15)[1]]
    add("exact", "measure", 0L, paste0(name, c(":expected_tranche_loss", ":hit_probability", ":wipeout_probability", ":var@0.99"), ":rho=", rho),
        c(sum(p * tl), sum(p[rate > a]), sum(p[rate >= d]), var99))
  })
})

# ---- large homogeneous pool -----------------------------------------------------------------------------------
lhp <- function(pd, lgd, rho, a, d) {
  f <- function(z) stats::dnorm(z) * tranche_loss(lgd * stats::pnorm((stats::qnorm(pd) - sqrt(rho) * z) / sqrt(1 - rho)), a, d)
  # split at the kinks, where lgd p(z) crosses a and d
  k <- c(a, d); k <- k[k > 0 & k < lgd]
  cuts <- sort(c(-12, (stats::qnorm(pd) - sqrt(1 - rho) * stats::qnorm(k / lgd)) / sqrt(rho), 12))
  sum(map2_dbl(head(cuts, -1), cuts[-1], \(lo, hi) stats::integrate(f, lo, hi, rel.tol = 1e-12, abs.tol = 0, subdivisions = 2000L)$value))
}
walk(c(0.1, 0.2, 0.4), \(rho) pwalk(tranches, \(name, a, d) add("lhp", "expected_tranche_loss", 0L, paste0(name, ":rho=", rho), lhp(0.02, 0.6, rho, a, d))))

# ---- legs and fair spread (I-11.12) ---------------------------------------------------------------------------
hazard <- -log(1 - pd)
times <- seq(0.25, 5, by = 0.25)
etl_by_date <- map(times, \(t) {
  p <- exact_distribution(1 - exp(-hazard * t), 0.2)
  map_dbl(seq_len(nrow(tranches)), \(k) sum(p * tranche_loss(rate, tranches$a[k], tranches$d[k])))
})
etl <- do.call(rbind, etl_by_date)  # dates x tranches
df <- exp(-0.01 * times)
add("legs", "time", seq_along(times) - 1L, "", times)
pwalk(list(seq_len(nrow(tranches)), tranches$name, tranches$a, tranches$d), \(k, name, a, d) {
  e <- etl[, k]
  annuity <- sum(df * 0.25 * ((d - a) - e))
  loss_leg <- sum(df * diff(c(0, e)))
  add("legs", "expected_tranche_loss", seq_along(times) - 1L, name, e)
  add("legs", "measure", 0L, paste0(name, c(":premium_per_unit_spread", ":loss_leg", ":fair_spread")), c(annuity, loss_leg, loss_leg / annuity))
})

# ---- CMO --------------------------------------------------------------------------------------------------------
collateral <- function(psa) {
  m <- create_mbs_pass_through(1000, 0.055, 0.05, psa_multiplier = psa, refinancing_cost_rate = 0)
  compute_mbs_cashflows(m, 0.055)
}
sequential <- function(cf, balances, coupons, z_last) {
  n <- length(balances); months <- nrow(cf)
  bal <- balances; int_paid <- matrix(0, months, n); prin_paid <- matrix(0, months, n); accrued <- matrix(0, months, n)
  residual <- numeric(months)
  for (t in seq_len(months)) {
    avail_int <- cf$interest[t]; avail_prin <- cf$scheduled_principal[t] + cf$prepaid_principal[t]
    regular <- if (z_last) seq_len(n - 1L) else seq_len(n)
    for (k in regular) { i <- coupons[k] / 12 * bal[k]; int_paid[t, k] <- i; avail_int <- avail_int - i }
    if (z_last) {
      i <- coupons[n] / 12 * bal[n]
      if (any(bal[regular] > 0)) { accrued[t, n] <- i; bal[n] <- bal[n] + i; avail_prin <- avail_prin + i }
      else int_paid[t, n] <- i
      avail_int <- avail_int - i
    }
    for (k in seq_len(n)) { pay <- min(avail_prin, bal[k]); prin_paid[t, k] <- pay; bal[k] <- bal[k] - pay; avail_prin <- avail_prin - pay }
    residual[t] <- avail_int + avail_prin
  }
  list(int = int_paid, prin = prin_paid, accrued = accrued, residual = residual)
}
pac_support <- function(cf, schedule, pac, support, coupon) {
  months <- nrow(cf); bal <- c(pac, support); int_paid <- matrix(0, months, 2); prin_paid <- matrix(0, months, 2); residual <- numeric(months)
  shortfall <- 0
  for (t in seq_len(months)) {
    avail_int <- cf$interest[t]; avail_prin <- cf$scheduled_principal[t] + cf$prepaid_principal[t]
    for (k in 1:2) { i <- coupon / 12 * bal[k]; int_paid[t, k] <- i; avail_int <- avail_int - i }
    due <- min(bal[1], schedule[t] + shortfall)
    pay <- min(avail_prin, due); shortfall <- due - pay
    prin_paid[t, 1] <- pay; bal[1] <- bal[1] - pay; avail_prin <- avail_prin - pay
    pay <- min(avail_prin, bal[2]); prin_paid[t, 2] <- pay; bal[2] <- bal[2] - pay; avail_prin <- avail_prin - pay
    pay <- min(avail_prin, bal[1]); prin_paid[t, 1] <- prin_paid[t, 1] + pay; bal[1] <- bal[1] - pay; avail_prin <- avail_prin - pay
    if (bal[1] <= 0) shortfall <- 0
    residual[t] <- avail_int + avail_prin
  }
  list(int = int_paid, prin = prin_paid, residual = residual)
}
wal <- function(prin) colSums(prin * seq_len(nrow(prin))) / colSums(prin) / 12

walk(c(1, 3), \(psa) {
  cf <- collateral(psa)
  add("cmo", "collateral_interest", seq_len(nrow(cf)) - 1L, paste0("psa=", psa), cf$interest)
  add("cmo", "collateral_principal", seq_len(nrow(cf)) - 1L, paste0("psa=", psa), cf$scheduled_principal + cf$prepaid_principal)
  s <- sequential(cf, c(400, 300, 200, 100), rep(0.05, 4), TRUE)
  tag <- paste0("sequential:psa=", psa)
  walk(1:4, \(k) {
    add("cmo", "principal", seq_len(nrow(cf)) - 1L, paste0(tag, ":", k - 1L), s$prin[, k])
    add("cmo", "interest", seq_len(nrow(cf)) - 1L, paste0(tag, ":", k - 1L), s$int[, k])
  })
  add("cmo", "accrued", seq_len(nrow(cf)) - 1L, paste0(tag, ":3"), s$accrued[, 4])
  add("cmo", "residual", seq_len(nrow(cf)) - 1L, tag, s$residual)
  add("cmo", "wal", 0:3, tag, wal(s$prin))
})

low <- collateral(1); high <- collateral(3)
schedule <- pmin(low$scheduled_principal + low$prepaid_principal, high$scheduled_principal + high$prepaid_principal)
add("cmo", "pac_schedule", seq_along(schedule) - 1L, "", schedule)
add("cmo", "pac_schedule_total", 0L, "", sum(schedule))
walk(c(0.5, 1.5, 4), \(psa) {
  cf <- collateral(psa)
  if (!psa %in% c(1, 3)) {
    add("cmo", "collateral_interest", seq_len(nrow(cf)) - 1L, paste0("psa=", psa), cf$interest)
    add("cmo", "collateral_principal", seq_len(nrow(cf)) - 1L, paste0("psa=", psa), cf$scheduled_principal + cf$prepaid_principal)
  }
  r <- pac_support(cf, schedule, 700, 300, 0.05)
  tag <- paste0("pac:psa=", psa)
  walk(1:2, \(k) {
    add("cmo", "principal", seq_len(nrow(cf)) - 1L, paste0(tag, ":", k - 1L), r$prin[, k])
    add("cmo", "interest", seq_len(nrow(cf)) - 1L, paste0(tag, ":", k - 1L), r$int[, k])
  })
  add("cmo", "residual", seq_len(nrow(cf)) - 1L, tag, r$residual)
  add("cmo", "wal", 0:1, tag, wal(r$prin))
})

all_rows <- bind_rows(rows)
dir.create(dirname(out), recursive = TRUE, showWarnings = FALSE)
writeLines(c(
  "# Credit tranches and CMO waterfalls: references in R (tools/golden/p15_tranching.R)",
  sprintf("# R %s, rmql %s (collateral cash flows only)", getRversion(), packageVersion("rmql")),
  "case,kind,i,j,value",
  sprintf("%s,%s,%d,%s,%s", all_rows$case, all_rows$kind, all_rows$i, all_rows$j, fmt(all_rows$value))
), out)
message(sprintf("%d rows -> %s", nrow(all_rows), out))
print(all_rows |> filter(kind %in% c("measure", "expected_tranche_loss", "wal", "pac_schedule_total"), case != "legs" | kind == "measure") |>
        filter(kind != "expected_tranche_loss" | case == "lhp") |> select(case, j, value), n = 80)
