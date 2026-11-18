# Golden values for constrained portfolio optimization and risk contributions (docs/spec_portfolio.md).
#
# Usage (from the rmql root): Rscript tools/golden/p12_portfolio.R
# Writes tests/golden/portfolio.csv: case, kind, i, j, value.
#
# Cases
#   lf_i8   LagrangeFinance I-8: the first rebalance year's 20 largest firms, their complete months
#           (anonymized public data, read from ../coral-book, formerly LagrangeFinance), weights 0 to 0.20 summing to 1.
#   syn_1.. synthetic returns (set.seed below): without bounds (closed forms) and with bounds.
#
# References, all computed here in R, independently of rmql:
#   quadprog::solve.QP     minimum variance, mean-variance, target return, maximum Sharpe (on the problem
#                          in y = kappa w), target volatility (bisection on the target return)
#   Rglpk                  maximum return, minimum ES (Rockafellar-Uryasev linear program)
#   closed forms           unbounded minimum variance, mean-variance, tangency and frontier
#   Newton in R            risk budget (Spinu's convex problem) where the bounds do not bind
#   GaussQuant             portfolio_optimize_GQL() with the manuscript's arguments (kinds gq_*)
# Mean-variance uses mu'w - (gamma/2) w'Sigma w (the manuscript's formula). GaussQuant's risk_aversion = 5
# is mu'w - 5 w'Sigma w, that is gamma = 10.
#
# Kinds: return (i = month, j = asset), lower, upper (i = asset), parameter (i = name in `name`), and per
# method `weight` (i = asset) and `value` (i = statistic). The CSV is long: method and statistic are in `j`.

suppressPackageStartupMessages({
  library(dplyr)
  library(tidyr)
  library(purrr)
})

out <- file.path("tests", "golden", "portfolio.csv")
fmt <- function(x) sprintf("%.17g", x)
lagrange <- file.path("..", "coral-book", "data", "derived", "portfolios")  # Coral Book, formerly LagrangeFinance

# ---- references -------------------------------------------------------------------------------------------

moments <- function(r) list(mu = colMeans(r), sigma = stats::cov(r))

# min (1/2) x'Dx - d'x  s.t. A'x >= b (first meq equalities); bounds as rows
qp <- function(D, d, eq = NULL, eq_rhs = NULL, ineq = NULL, ineq_rhs = NULL, lower, upper) {
  n <- length(d)
  keep_l <- is.finite(lower); keep_u <- is.finite(upper)
  A <- cbind(if (!is.null(eq)) t(eq), if (!is.null(ineq)) t(ineq), diag(n)[, keep_l, drop = FALSE], -diag(n)[, keep_u, drop = FALSE])
  b <- c(eq_rhs, ineq_rhs, lower[keep_l], -upper[keep_u])
  quadprog::solve.QP(D, d, A, b, meq = if (is.null(eq)) 0 else nrow(eq))$solution
}

minimum_variance <- function(m, lower, upper, total = 1)
  qp(2 * m$sigma, rep(0, length(m$mu)), eq = matrix(1, 1, length(m$mu)), eq_rhs = total, lower = lower, upper = upper)

mean_variance <- function(m, gamma, lower, upper, total = 1)
  qp(gamma * m$sigma, m$mu, eq = matrix(1, 1, length(m$mu)), eq_rhs = total, lower = lower, upper = upper)

target_return <- function(m, target, lower, upper, total = 1)
  qp(2 * m$sigma, rep(0, length(m$mu)), eq = matrix(1, 1, length(m$mu)), eq_rhs = total,
     ineq = matrix(m$mu, 1), ineq_rhs = target, lower = lower, upper = upper)

# max (mu - rf)'w / sd: min y'Sigma y, (mu - rf)'y = 1, y_i - l_i 1'y >= 0, u_i 1'y - y_i >= 0, w = y / 1'y
maximum_sharpe <- function(m, rf, lower, upper) {
  n <- length(m$mu); one <- matrix(1, 1, n)
  rows_l <- if (any(is.finite(lower))) (diag(n) - lower %*% one)[is.finite(lower), , drop = FALSE]
  rows_u <- if (any(is.finite(upper))) (upper %*% one - diag(n))[is.finite(upper), , drop = FALSE]
  ineq <- rbind(rows_l, rows_u, one)
  y <- qp(2 * m$sigma, rep(0, n), eq = matrix(m$mu - rf, 1), eq_rhs = 1, ineq = ineq, ineq_rhs = rep(0, nrow(ineq)),
          lower = rep(-Inf, n), upper = rep(Inf, n))
  y / sum(y)
}

maximum_return <- function(m, lower, upper, total = 1) {
  n <- length(m$mu)
  s <- Rglpk::Rglpk_solve_LP(m$mu, matrix(1, 1, n), "==", total, max = TRUE,
                             bounds = list(lower = list(ind = seq_len(n), val = lower), upper = list(ind = seq_len(n), val = upper)))
  stopifnot(s$status == 0)
  s$solution
}

# Rockafellar-Uryasev: min zeta + sum(u) / ((1 - alpha) T), u_t >= -r_t'w - zeta, u >= 0
minimum_es <- function(r, alpha, lower, upper, total = 1) {
  n <- ncol(r); nt <- nrow(r)
  obj <- c(rep(0, n), 1, rep(1 / ((1 - alpha) * nt), nt))
  mat <- rbind(cbind(r, 1, diag(nt)), c(rep(1, n), 0, rep(0, nt)))
  bounds <- list(lower = list(ind = seq_len(n + 1), val = c(lower, -Inf)), upper = list(ind = seq_len(n), val = upper))
  s <- Rglpk::Rglpk_solve_LP(obj, mat, c(rep(">=", nt), "=="), c(rep(0, nt), total), bounds = bounds)
  stopifnot(s$status == 0)
  list(weights = s$solution[seq_len(n)], value = s$optimum)
}

# Spinu: min (1/2) y'Sigma y - sum(b log y), then w = y / sum(y); the risk contributions are b
risk_budget <- function(m, b) {
  y <- b / sqrt(diag(m$sigma))
  for (k in 1:100) {
    g <- as.numeric(m$sigma %*% y) - b / y
    h <- m$sigma + diag(b / y^2, length(y))
    step <- solve(h, g)
    t <- 1
    while (any(y - t * step <= 0)) t <- t / 2
    y <- y - t * step
    if (max(abs(g)) < 1e-15) break
  }
  y / sum(y)
}

target_volatility <- function(m, target, lower, upper) {
  sd <- function(w) sqrt(as.numeric(t(w) %*% m$sigma %*% w))
  w_min <- minimum_variance(m, lower, upper); w_max <- maximum_return(m, lower, upper)
  if (sd(w_max) <= target) return(w_max)
  lo <- sum(m$mu * w_min); hi <- sum(m$mu * w_max)
  for (k in 1:200) {
    mid <- (lo + hi) / 2
    if (sd(target_return(m, mid, lower, upper)) > target) hi <- mid else lo <- mid
  }
  target_return(m, lo, lower, upper)
}

# risk measures and their Euler contributions in closed form
risk <- function(w, r, alpha) {
  m <- moments(r); sw <- as.numeric(m$sigma %*% w); sd <- sqrt(sum(w * sw)); mu_p <- sum(w * m$mu)
  z <- stats::qnorm(alpha); k_es <- stats::dnorm(z) / (1 - alpha)
  loss <- -as.numeric(r %*% w); nt <- length(loss)
  ord <- order(loss, decreasing = TRUE); kk <- nt * (1 - alpha); fl <- floor(kk)
  tail_weight <- numeric(nt); tail_weight[ord[seq_len(fl)]] <- 1; if (kk > fl) tail_weight[ord[fl + 1]] <- kk - fl
  q7 <- stats::quantile(loss, alpha, type = 7, names = FALSE)
  list(
    stat = c(expected_return = mu_p, volatility = sd, normal_var = -mu_p + z * sd, normal_es = -mu_p + k_es * sd,
             historical_var = q7, historical_es = sum(tail_weight * loss) / kk, tail_mean_es = mean(loss[loss >= q7])),
    contribution = list(volatility = w * sw / sd, normal_var = w * (-m$mu + z * sw / sd), normal_es = w * (-m$mu + k_es * sw / sd),
                        historical_es = w * as.numeric(colSums(-r * tail_weight)) / kk)
  )
}

# ---- cases ------------------------------------------------------------------------------------------------

lf_case <- function() {
  history <- arrow::read_parquet(file.path(lagrange, "asset_return_history.parquet"))
  universe <- arrow::read_parquet(file.path(lagrange, "investment_universe.parquet"))
  first_year <- min(universe$rebalance_year, na.rm = TRUE)
  assets <- universe |> filter(rebalance_year == first_year) |> arrange(desc(market_equity)) |> slice_head(n = 20) |>
    pull(firm_ID) |> as.character()
  long <- history |> filter(firm_ID %in% as.integer(assets)) |>
    transmute(date = month_ID, asset = as.character(firm_ID), return = as.numeric(return)) |> arrange(date, asset)
  wide <- long |> pivot_wider(names_from = asset, values_from = return) |> arrange(date)
  r <- as.matrix(wide[, assets]); r <- r[stats::complete.cases(r), , drop = FALSE]
  list(id = "lf_i8", r = r, lower = rep(0, 20), upper = rep(0.2, 20), gamma = 10, rf = 0, alpha = 0.95, long = long, assets = assets)
}

synthetic_case <- function(id, n, nt, bounded) {
  a <- matrix(stats::rnorm(n * n, sd = 0.03), n)
  mu <- stats::runif(n, -0.005, 0.02)
  r <- sweep(matrix(stats::rnorm(nt * n), nt) %*% a, 2, mu, "+")
  list(id = id, r = r, lower = if (bounded) rep(-0.1, n) else rep(-Inf, n), upper = if (bounded) rep(0.4, n) else rep(Inf, n),
       gamma = 4, rf = 0.001, alpha = 0.95)
}

set.seed(20261114)
cases <- c(list(lf_case()), list(
  synthetic_case("syn_1", 3, 40, FALSE), synthetic_case("syn_2", 8, 90, FALSE), synthetic_case("syn_3", 12, 120, FALSE),
  synthetic_case("syn_4", 6, 60, TRUE), synthetic_case("syn_5", 10, 100, TRUE)
))

rows_of_case <- function(cs) {
  r <- cs$r; n <- ncol(r); m <- moments(r); lower <- cs$lower; upper <- cs$upper
  unbounded <- all(!is.finite(c(lower, upper)))
  w_minvar <- minimum_variance(m, lower, upper)
  w_maxret <- if (unbounded) NULL else maximum_return(m, lower, upper)
  mid_return <- if (unbounded) sum(m$mu * w_minvar) + 0.004 else mean(c(sum(m$mu * w_minvar), sum(m$mu * w_maxret)))
  sd <- function(w) sqrt(as.numeric(t(w) %*% m$sigma %*% w))
  mid_vol <- if (unbounded) sd(w_minvar) * 1.3 else mean(c(sd(w_minvar), sd(w_maxret)))
  es <- if (unbounded) NULL else minimum_es(r, cs$alpha, lower, upper)

  methods <- list(
    minimum_variance = w_minvar,
    mean_variance = mean_variance(m, cs$gamma, lower, upper),
    maximum_sharpe = maximum_sharpe(m, cs$rf, lower, upper),
    target_return = target_return(m, mid_return, lower, upper),
    risk_budget = risk_budget(m, rep(1 / n, n))
  )
  if (!unbounded) {
    methods$maximum_return <- w_maxret
    methods$minimum_ES <- es$weights
    methods$target_risk <- target_volatility(m, mid_vol, lower, upper)
  } else {
    # closed forms without bounds
    s_inv <- solve(m$sigma); one <- rep(1, n)
    a <- as.numeric(t(one) %*% s_inv %*% one); b <- as.numeric(t(one) %*% s_inv %*% m$mu); cc <- as.numeric(t(m$mu) %*% s_inv %*% m$mu)
    closed <- list(
      minimum_variance = as.numeric(s_inv %*% one) / a,
      mean_variance = as.numeric(s_inv %*% one) / a + as.numeric(s_inv %*% m$mu - (b / a) * s_inv %*% one) / cs$gamma,
      maximum_sharpe = as.numeric(s_inv %*% (m$mu - cs$rf)) / sum(s_inv %*% (m$mu - cs$rf)),
      target_return = as.numeric(((cc - b * mid_return) * s_inv %*% one + (a * mid_return - b) * s_inv %*% m$mu) / (a * cc - b^2))
    )
    walk(names(closed), \(k) stopifnot(max(abs(closed[[k]] - methods[[k]])) < 1e-9))
  }
  # equal risk contributions hold where the bounds do not bind
  rb <- methods$risk_budget; rc <- rb * as.numeric(m$sigma %*% rb)
  stopifnot(max(abs(rc / sum(rc) - 1 / n)) < 1e-12, all(rb >= lower), all(rb <= upper))

  frontier_targets <- if (unbounded) NULL else seq(sum(m$mu * w_minvar), sum(m$mu * w_maxret), length.out = 31)[1:30]
  frontier <- map(frontier_targets, \(t) target_return(m, t, lower, upper))

  parameters <- c(risk_aversion = cs$gamma, risk_free_rate = cs$rf, confidence_level = cs$alpha, target_return = mid_return,
                  target_volatility = mid_vol, weight_sum = 1, minimum_es_value = if (is.null(es)) NA else es$value)

  base <- list(
    tibble(kind = "return", i = rep(seq_len(nrow(r)) - 1L, n), j = as.character(rep(seq_len(n) - 1L, each = nrow(r))), value = as.numeric(r)),
    tibble(kind = "lower", i = seq_len(n) - 1L, j = "", value = lower),
    tibble(kind = "upper", i = seq_len(n) - 1L, j = "", value = upper),
    tibble(kind = "parameter", i = 0L, j = names(parameters), value = unname(parameters)) |> filter(!is.na(value))
  )
  weights <- imap(methods, \(w, k) tibble(kind = "weight", i = seq_len(n) - 1L, j = k, value = w))
  measures <- imap(methods, \(w, k) {
    x <- risk(w, r, cs$alpha)
    contributions <- imap(x$contribution, \(v, nm) tibble(kind = "contribution", i = seq_len(n) - 1L, j = paste(k, nm, sep = ":"), value = v))
    bind_rows(c(list(tibble(kind = "risk", i = 0L, j = paste(k, names(x$stat), sep = ":"), value = unname(x$stat))), unname(contributions)))
  })
  front <- imap(frontier, \(w, k) tibble(kind = "frontier", i = seq_len(n) - 1L, j = fmt(frontier_targets[k]), value = w))

  gq <- if (cs$id == "lf_i8") {
    spec <- GaussQuant::portfolio_spec_GQL(assets = cs$assets) |>
      GaussQuant::portfolio_constraint_GQL(type = "weight_sum", min_sum = 1, max_sum = 1) |>
      GaussQuant::portfolio_constraint_GQL(type = "box", min = 0, max = 0.2)
    args <- list(minimum_variance = list(), mean_variance = list(risk_aversion = 5), maximum_return = list(),
                 maximum_sharpe = list(risk_free_rate = 0), minimum_ES = list(confidence_level = 0.95), risk_budget = list(),
                 target_return = list(target_return = mid_return), target_risk = list(target_volatility = mid_vol))
    imap(args, \(a, k) {
      res <- do.call(GaussQuant::portfolio_optimize_GQL, c(list(tbl_returns = cs$long, portfolio = spec, method = k), a))
      w <- res$weights$weight[match(cs$assets, res$weights$asset)]
      bind_rows(tibble(kind = "gq_weight", i = seq_len(n) - 1L, j = k, value = w),
                tibble(kind = "gq_value", i = 0L, j = paste(k, res$statistics$metric, sep = ":"), value = res$statistics$value))
    })
  }
  bind_rows(c(base, unname(weights), unname(measures), unname(front), unname(gq))) |> mutate(case = cs$id, .before = 1)
}

rows <- map(cases, rows_of_case) |> bind_rows()
dir.create(dirname(out), recursive = TRUE, showWarnings = FALSE)
writeLines(c(
  "# Constrained portfolio optimization: references from quadprog, Rglpk, closed forms and GaussQuant (tools/golden/p12_portfolio.R)",
  sprintf("# R %s, quadprog %s, Rglpk %s, GaussQuant %s", getRversion(), packageVersion("quadprog"), packageVersion("Rglpk"), packageVersion("GaussQuant")),
  "case,kind,i,j,value",
  sprintf("%s,%s,%d,%s,%s", rows$case, rows$kind, rows$i, rows$j, ifelse(is.infinite(rows$value), ifelse(rows$value > 0, "inf", "-inf"), fmt(rows$value)))
), out)
message(sprintf("%d rows -> %s", nrow(rows), out))
rows |> filter(kind %in% c("risk", "gq_value"), grepl("volatility|expected_shortfall|historical_es|sharpe", j)) |>
  filter(case == "lf_i8") |> print(n = 60)
