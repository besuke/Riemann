# Golden values for market VaR, ES and their backtests (LagrangeFinance I-9; docs/spec_market_risk.md).
#
# Usage (from the Riemann root): Rscript tools/golden/p13_market_risk.R
# Writes tests/golden/market_risk.csv: case, kind, i, j, value.
#
# Case lf_i9: the manuscript's equal-weight portfolio of the first rebalance year's 20 largest firms, the mean of
# the available returns each month (I-9.3; anonymized public data read from ../coral-book, formerly LagrangeFinance), and its
# losses L_t = -R_t. References, all in R:
#   Gaussian VaR and ES      -mu + qnorm(alpha) sd, -mu + dnorm(qnorm(alpha)) / (1 - alpha) sd
#   historical VaR           quantile(type = 7) (the manuscript) and type 1
#   historical ES            mean of the losses at or above the VaR (the manuscript) and Rockafellar-Uryasev
#   Monte Carlo VaR          the manuscript's chunk as written (furrr, 20 x 5000 normal draws, seed 20260918):
#                            a different random stream from RiemannFinMath's, so compared within the sampling error
#   backtest                 exceptions L_t > VaR, Kupiec's proportion of failures, Christoffersen's independence
#                            and conditional coverage (likelihood ratios and pchisq)
#   rolling backtest         VaR_t from the previous 36 losses (historical type 7 and Gaussian), then the same tests
#   traffic light            the Basel zones for 250 days at 99%: green while pbinom(k) < 0.95, yellow while < 0.9999
# Case lf_i8_moments: the mean and covariance of I-8's 20 assets (complete months), for the multivariate Monte
# Carlo VaR of the equal-weight portfolio against its Gaussian VaR.

suppressPackageStartupMessages({
  library(dplyr)
  library(tidyr)
  library(purrr)
})

out <- file.path("tests", "golden", "market_risk.csv")
fmt <- function(x) sprintf("%.17g", x)
lagrange <- file.path("..", "coral-book", "data", "derived", "portfolios")  # Coral Book, formerly LagrangeFinance
history <- arrow::read_parquet(file.path(lagrange, "asset_return_history.parquet"))
universe <- arrow::read_parquet(file.path(lagrange, "investment_universe.parquet"))

# ---- the manuscript's portfolio (I-9.3) ----------------------------------------------------------------------
risk_year <- universe |> summarise(risk_year = min(rebalance_year, na.rm = TRUE)) |> pull(risk_year)
assets <- universe |> filter(rebalance_year == !!risk_year) |> arrange(desc(market_equity)) |> slice_head(n = 20) |> pull(firm_ID)
portfolio <- history |> filter(firm_ID %in% assets) |> group_by(month_ID) |>
  summarise(portfolio_return = mean(return, na.rm = TRUE), .groups = "drop") |> arrange(month_ID)
losses <- -portfolio$portfolio_return

# ---- references ------------------------------------------------------------------------------------------------
gaussian <- function(r, alpha) {
  m <- mean(r); s <- stats::sd(r); z <- stats::qnorm(alpha)
  c(var = -m + z * s, es = -m + stats::dnorm(z) / (1 - alpha) * s)
}
ru_es <- function(loss, alpha) {
  sorted <- sort(loss, decreasing = TRUE); k <- length(loss) * (1 - alpha); fl <- floor(k)
  (sum(sorted[seq_len(fl)]) + if (k > fl) (k - fl) * sorted[fl + 1] else 0) / k
}
historical <- function(loss, alpha) {
  v7 <- stats::quantile(loss, alpha, type = 7, names = FALSE)
  c(var_type7 = v7, var_type1 = stats::quantile(loss, alpha, type = 1, names = FALSE),
    tail_mean_es = mean(loss[loss >= v7]), ru_es = ru_es(loss, alpha))
}
xlogy <- function(x, y) ifelse(x == 0, 0, x * log(y))
backtest <- function(loss, var, alpha) {
  hit <- as.integer(loss > var); n <- length(hit); x <- sum(hit); p <- 1 - alpha
  lr_pof <- -2 * (xlogy(n - x, 1 - p) + xlogy(x, p)) + 2 * (xlogy(n - x, 1 - x / n) + xlogy(x, x / n))
  a <- hit[-n]; b <- hit[-1]
  n00 <- sum(a == 0 & b == 0); n01 <- sum(a == 0 & b == 1); n10 <- sum(a == 1 & b == 0); n11 <- sum(a == 1 & b == 1)
  pi0 <- if (n00 + n01) n01 / (n00 + n01) else 0; pi1 <- if (n10 + n11) n11 / (n10 + n11) else 0; pi <- (n01 + n11) / (n - 1)
  lr_ind <- -2 * (xlogy(n00 + n10, 1 - pi) + xlogy(n01 + n11, pi)) +
    2 * (xlogy(n00, 1 - pi0) + xlogy(n01, pi0) + xlogy(n10, 1 - pi1) + xlogy(n11, pi1))
  c(observations = n, exceptions = x, exception_rate = x / n, expected_rate = p,
    kupiec_lr = lr_pof, kupiec_p_value = stats::pchisq(lr_pof, 1, lower.tail = FALSE),
    n00 = n00, n01 = n01, n10 = n10, n11 = n11,
    independence_lr = lr_ind, independence_p_value = stats::pchisq(lr_ind, 1, lower.tail = FALSE),
    conditional_coverage_lr = lr_pof + lr_ind, conditional_coverage_p_value = stats::pchisq(lr_pof + lr_ind, 2, lower.tail = FALSE))
}

# The manuscript's Monte Carlo chunk (I-9.6), as written, on one worker (furrr's seeds do not depend on the plan)
manuscript_monte_carlo <- function(alpha) {
  mean_return <- mean(portfolio$portfolio_return); volatility <- stats::sd(portfolio$portfolio_return)
  future::plan(future::sequential)
  set.seed(20260918)
  n_scenarios <- 100000; n_batches <- 20; n_per_batch <- n_scenarios / n_batches
  draws <- furrr::future_map(seq_len(n_batches), \(batch_id) tibble(simulated_return = rnorm(n = n_per_batch, mean = mean_return, sd = volatility)),
                             .options = furrr::furrr_options(seed = TRUE)) |> bind_rows()
  stats::quantile(-draws$simulated_return, probs = alpha, na.rm = TRUE, names = FALSE)
}

rows <- list()
add <- function(case, kind, i, j, value) rows[[length(rows) + 1L]] <<- tibble(case = case, kind = kind, i = as.integer(i), j = as.character(j), value = as.numeric(value))

add("lf_i9", "loss", seq_along(losses) - 1L, "", losses)
walk(c(0.95, 0.975, 0.99), \(alpha) {
  g <- gaussian(-losses, alpha); h <- historical(losses, alpha)
  add("lf_i9", "measure", 0L, paste0(c("gaussian_var", "gaussian_es", names(h)), "@", alpha), c(g, h))
})
add("lf_i9", "measure", 0L, "manuscript_monte_carlo_var@0.95", manuscript_monte_carlo(0.95))

# the manuscript's static backtest (I-9.10) and the tests on it
static <- backtest(losses, stats::quantile(losses, 0.95, type = 7, names = FALSE), 0.95)
add("lf_i9", "static_backtest", 0L, names(static), static)

# rolling: VaR_t from the 36 losses before t
window <- 36L
rolling <- map(c(historical = "historical", gaussian = "gaussian"), \(method) {
  var_t <- map_dbl((window + 1L):length(losses), \(t) {
    past <- losses[(t - window):(t - 1L)]
    if (method == "historical") stats::quantile(past, 0.95, type = 7, names = FALSE) else gaussian(-past, 0.95)[["var"]]
  })
  list(var = var_t, test = backtest(losses[(window + 1L):length(losses)], var_t, 0.95))
})
iwalk(rolling, \(x, method) {
  add("lf_i9", "rolling_var", seq_along(x$var) - 1L, method, x$var)
  add("lf_i9", "rolling_backtest", 0L, paste0(method, ":", names(x$test)), x$test)
})
add("lf_i9", "parameter", 0L, "window", window)

# Basel traffic light, 250 observations at 99%
k <- 0:15
cum <- stats::pbinom(k, 250, 0.01)
add("basel", "traffic_light_cumulative", k, "", cum)
add("basel", "traffic_light_zone", k, "", ifelse(cum < 0.95, 0, ifelse(cum < 0.9999, 1, 2)))

# ---- I-8's assets for the multivariate Monte Carlo ----------------------------------------------------------
first_year <- min(universe$rebalance_year, na.rm = TRUE)
i8_assets <- universe |> filter(rebalance_year == first_year) |> arrange(desc(market_equity)) |> slice_head(n = 20) |> pull(firm_ID) |> as.character()
wide <- history |> filter(firm_ID %in% as.integer(i8_assets)) |> transmute(date = month_ID, asset = as.character(firm_ID), return = as.numeric(return)) |>
  pivot_wider(names_from = asset, values_from = return) |> arrange(date)
r <- as.matrix(wide[, i8_assets]); r <- r[stats::complete.cases(r), , drop = FALSE]
mu <- colMeans(r); sigma <- stats::cov(r); w <- rep(1 / 20, 20)
add("lf_i8_moments", "mean", seq_along(mu) - 1L, "", mu)
add("lf_i8_moments", "covariance", rep(seq_along(mu) - 1L, 20), rep(seq_along(mu) - 1L, each = 20), as.numeric(sigma))
sd_p <- sqrt(as.numeric(t(w) %*% sigma %*% w)); mu_p <- sum(w * mu)
walk(c(0.95, 0.99), \(alpha) add("lf_i8_moments", "measure", 0L, paste0(c("gaussian_var", "gaussian_es"), "@", alpha),
                                 c(-mu_p + stats::qnorm(alpha) * sd_p, -mu_p + stats::dnorm(stats::qnorm(alpha)) / (1 - alpha) * sd_p)))

all_rows <- bind_rows(rows)
dir.create(dirname(out), recursive = TRUE, showWarnings = FALSE)
writeLines(c(
  "# Market VaR, ES and backtests: references in R (tools/golden/p13_market_risk.R)",
  sprintf("# R %s, furrr %s, future %s", getRversion(), packageVersion("furrr"), packageVersion("future")),
  "case,kind,i,j,value",
  sprintf("%s,%s,%d,%s,%s", all_rows$case, all_rows$kind, all_rows$i, all_rows$j, fmt(all_rows$value))
), out)
message(sprintf("%d rows -> %s (%d losses)", nrow(all_rows), out, length(losses)))
print(all_rows |> filter(kind %in% c("measure", "static_backtest")) |> select(j, value), n = 60)
print(all_rows |> filter(kind == "rolling_backtest") |> select(j, value), n = 40)
