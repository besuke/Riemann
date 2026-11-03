#' Pass-through mortgage-backed securities
#'
#' A pool of level-payment mortgages whose principal and interest pass to the investors, without default.
#' Each month the borrowers prepay a share of the balance, by the rule of LagrangeFinance II-8.8:
#' `CPR = min(cap, psa_multiplier * min(0.002 m, 0.06) * exp(incentive_sensitivity * (mortgage_rate - f - refinancing_cost_rate)))`
#' with `f` the refinancing rate of month `m`, and `SMM = 1 - (1 - CPR)^(1/12)`. This is a behavioural rule
#' of the borrowers, not an optimal exercise, and its parameters are assumptions, not estimates.
#'
#' `create_mbs_pass_through()` checks the terms once. `compute_mbs_cashflows()` gives the monthly cash
#' flows for refinancing rates by month. `compute_mbs_path_values()` values the pool on each row of short
#' rates, discounting month `m` by `exp(-sum((r_i + oas) / 12, i <= m))`, and its mean is the price.
#' `compute_mbs_oas()` finds the spread at which that mean equals a price; with one flat row it is the
#' static spread of II-8.8.1. `generate_hull_white_monthly_rates()` gives monthly Hull-White short rates on
#' a flat curve; its numbers are this package's, not R's.
#'
#' @param original_balance,mortgage_rate,pass_through_rate The pool; annual rates.
#' @param maturity_months Number of monthly payments.
#' @param psa_multiplier 1 is 100% PSA, 0 no prepayment.
#' @param incentive_sensitivity,refinancing_cost_rate,cpr_cap The prepayment rule.
#' @param amortization `"fixed_payment"` keeps the original level payment, so that prepayments shorten
#'   the loan (LagrangeFinance); `"reamortized"` recomputes the payment on the remaining balance and term.
#' @param mbs A pool from `create_mbs_pass_through()`.
#' @param refinance_rate One rate, or one per month.
#' @param short_rates,refinance_rates Matrices with one row per path and one column per month.
#' @param oas Spread added to every short rate.
#' @param price Price to match.
#' @param paths,months,flat_rate,mean_reversion,volatility,seed,antithetic The simulation.
#' @return `create_mbs_pass_through()` a list of class `riemann_mbs`. `compute_mbs_cashflows()` a tibble by
#'   month. `compute_mbs_path_values()` a tibble with `path`, `price` and `weighted_average_life`.
#'   `compute_mbs_oas()` a number. `generate_hull_white_monthly_rates()` a matrix.
#' @examples
#' mbs <- create_mbs_pass_through(100, 0.05, 0.045, refinancing_cost_rate = 0.005)
#' head(compute_mbs_cashflows(mbs, 0.035))
#' r <- generate_hull_white_monthly_rates(20, 360, 0.02, 0.15, 0.012)
#' values <- compute_mbs_path_values(mbs, r, pmax(r + 0.03, 0))
#' mean(values$price)
#' compute_mbs_oas(mbs, 110, r, pmax(r + 0.03, 0))
#' @export
create_mbs_pass_through <- function(original_balance, mortgage_rate, pass_through_rate, maturity_months = 360L, psa_multiplier = 1,
                                    incentive_sensitivity = 20, refinancing_cost_rate = 0, cpr_cap = 0.6, amortization = "fixed_payment") {
  values <- c(original_balance, mortgage_rate, pass_through_rate, maturity_months, psa_multiplier, incentive_sensitivity, refinancing_cost_rate, cpr_cap)
  if (length(values) != 8L || !is.numeric(values)) stop("Each term of a pass-through must be one number.", call. = FALSE)
  checked(.Call(wrap__check_mbs_pass_through, as.numeric(values), as.character(amortization)))
  structure(list(terms = as.numeric(values), amortization = amortization), class = "riemann_mbs")
}

check_mbs <- function(mbs) {
  if (!inherits(mbs, "riemann_mbs")) stop("`mbs` must come from create_mbs_pass_through().", call. = FALSE)
}

check_rate_matrices <- function(short_rates, refinance_rates) {
  short_rates <- as.matrix(short_rates); refinance_rates <- as.matrix(refinance_rates)
  if (!identical(dim(short_rates), dim(refinance_rates))) stop("`short_rates` and `refinance_rates` must have the same dimensions.", call. = FALSE)
  list(short = as.numeric(t(short_rates)), refinance = as.numeric(t(refinance_rates)), paths = nrow(short_rates))
}

#' @rdname create_mbs_pass_through
#' @export
compute_mbs_cashflows <- function(mbs, refinance_rate) {
  check_mbs(mbs)
  r <- checked(.Call(wrap__compute_mbs_cashflows, mbs$terms, mbs$amortization, as.numeric(refinance_rate)))
  tibble::as_tibble(c(list(month = seq_along(r$cashflow)), r))
}

#' @rdname create_mbs_pass_through
#' @export
compute_mbs_path_values <- function(mbs, short_rates, refinance_rates, oas = 0) {
  check_mbs(mbs)
  m <- check_rate_matrices(short_rates, refinance_rates)
  r <- checked(.Call(wrap__compute_mbs_path_values, mbs$terms, mbs$amortization, m$short, m$refinance, as.integer(m$paths), as.numeric(oas)))
  tibble::tibble(path = seq_along(r$price), price = r$price, weighted_average_life = r$weighted_average_life)
}

#' @rdname create_mbs_pass_through
#' @export
compute_mbs_oas <- function(mbs, price, short_rates, refinance_rates) {
  check_mbs(mbs)
  m <- check_rate_matrices(short_rates, refinance_rates)
  checked(.Call(wrap__compute_mbs_oas, mbs$terms, mbs$amortization, m$short, m$refinance, as.integer(m$paths), as.numeric(price)))$value
}

#' @rdname create_mbs_pass_through
#' @export
generate_hull_white_monthly_rates <- function(paths, months, flat_rate, mean_reversion, volatility, seed = 1, antithetic = TRUE) {
  r <- checked(.Call(wrap__generate_hull_white_monthly_rates, as.integer(paths), as.integer(months), as.numeric(flat_rate),
                     as.numeric(mean_reversion), as.numeric(volatility), as.numeric(seed), as_flag(antithetic)))$rates
  matrix(r, nrow = as.integer(paths), byrow = TRUE)
}

#' @export
print.riemann_mbs <- function(x, ...) {
  v <- x$terms
  cat(sprintf("<riemann_mbs> balance %g, mortgage %.4f, pass-through %.4f, %d months, PSA x %g, beta %g, cost %.4f, cap %g, %s\n",
              v[1], v[2], v[3], as.integer(v[4]), v[5], v[6], v[7], v[8], x$amortization))
  invisible(x)
}
