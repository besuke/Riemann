# Golden values for the issuer's call decisions of a callable bond: the `decisions` of LagrangeFinance II-8.4.2
# (`compute_callable_bond_LRL`, its own R functions, read from the chapter without changing it).
# All inputs are synthetic; no market data is used. No QuantLib is needed.
# Usage (from the Riemann root): Rscript tools/golden/p9_hull_white_callable_decisions.R
# Writes tests/golden/hw_callable_decisions_cases.csv and tests/golden/hw_callable_decisions.csv.

suppressMessages({ library(tibble); library(dplyr); library(purrr) })

out_dir <- file.path("tests", "golden")
fmt <- function(x) sprintf("%.17g", x)

chapter <- readLines("../coral-book/part2_quantive_riemann/ch08_early_redemption_models.qmd", encoding = "UTF-8")
chunk <- function(label, stop_at = NULL) {
  start <- which(chapter == paste0("#| label: ", label))
  end <- start + which(startsWith(chapter[(start + 1):length(chapter)], "```"))[1]
  body <- chapter[(start + 1):(end - 1)]
  if (!is.null(stop_at)) body <- body[seq_len(which(startsWith(body, stop_at))[1] - 1L)]
  paste(body, collapse = "\n")
}
environment_lf <- new.env()
eval(parse(text = chunk("hull-white-conditional-discount")), environment_lf)
eval(parse(text = chunk("callable-hull-white-backward", "model_result <-")), environment_lf)

contract <- function(coupon_rate, call_price, first_call, maturity) {
  times <- seq(0.5, maturity, by = 0.5)
  tibble::tibble(
    payment_time = times, coupon = 100 * coupon_rate * 0.5, principal_at_maturity = ifelse(times == maturity, 100, 0),
    issuer_can_call = times >= first_call & times < maturity, call_principal = ifelse(times >= first_call & times < maturity, call_price, NA_real_)
  )
}

# coupon_rate, call_price, first_call, maturity, flat_rate, a, sigma, points, limit, quadrature
cases <- list(
  list(0.03, 100, 1, 5, 0.02, 0.15, 0.012, 321L, 0.13, 24L),
  list(0.03, 100, 1, 5, 0.02, 0.15, 0.012, 161L, 0.13, 16L),
  list(0.045, 101, 2, 7, 0.025, 0.10, 0.015, 321L, 0.13, 24L),
  list(0.02, 100, 1, 3, 0.03, 0.20, 0.008, 321L, 0.13, 24L),
  list(0.06, 100, 1, 5, 0.015, 0.05, 0.018, 321L, 0.13, 24L)
)

cases_path <- file.path(out_dir, "hw_callable_decisions_cases.csv")
writeLines(c("# LagrangeFinance II-8 compute_callable_bond_LRL; synthetic inputs; tools/golden/p9_hull_white_callable_decisions.R",
             paste(c("case", "coupon_rate", "call_price", "first_call", "maturity", "flat_rate", "a", "sigma", "points", "limit", "quadrature"), collapse = ",")), cases_path)
path <- file.path(out_dir, "hw_callable_decisions.csv")
writeLines(c("# LagrangeFinance II-8 compute_callable_bond_LRL decisions; synthetic inputs; tools/golden/p9_hull_white_callable_decisions.R",
             paste(c("case", "payment_time", "state", "short_rate", "continuation_ex_coupon", "call_principal", "exercise_now"), collapse = ",")), path)
for (i in seq_along(cases)) {
  x <- cases[[i]]
  write(paste(c(i, fmt(unlist(x[1:7])), x[[8]], fmt(x[[9]]), x[[10]]), collapse = ","), cases_path, append = TRUE)
  v <- environment_lf$compute_callable_bond_LRL(contract(x[[1]], x[[2]], x[[3]], x[[4]]), flat_rate = x[[5]], mean_reversion = x[[6]], volatility = x[[7]],
                                                number_of_grid_points = x[[8]], grid_limit = x[[9]], quadrature_nodes = x[[10]])
  d <- v$decisions
  lines <- paste(i, fmt(d$payment_time), fmt(d$state), fmt(d$short_rate), fmt(d$continuation_ex_coupon), fmt(d$call_principal), as.integer(d$exercise_now), sep = ",")
  write(lines, path, append = TRUE)
  cat(sprintf("case %d: %d call dates, %d rows, called on %.1f%% of the grid\n", i, length(unique(d$payment_time)), nrow(d), 100 * mean(d$exercise_now)))
}
