#' Credit portfolio losses: one-factor Gaussian model, ASRF and Basel IRB capital
#'
#' Obligor `i` defaults when `sqrt(rho_i) Z + sqrt(1 - rho_i) eps_i < qnorm(PD_i)` and then loses `EAD_i LGD_i`
#' (LagrangeFinance I-10). `compute_conditional_pd()` is the default probability given `Z = z`,
#' `pnorm((qnorm(PD) - sqrt(rho) z) / sqrt(1 - rho))`.
#'
#' `compute_credit_loss_distribution()` gives the exact distribution of the portfolio loss when every `EAD_i
#' LGD_i` is a whole number of `unit`: given `z` the defaults are independent and the distribution follows by
#' recursion over the obligors; the integral over `z` is composite 16-point Gauss-Legendre quadrature of the normal
#' density on `[-10, 10]` with `panels` panels. `compute_credit_risk()` reads VaR and ES off a distribution:
#' `var = min{l : P(L <= l) >= alpha}`, `es` of Rockafellar and Uryasev, and `tail_mean_es = E[L | L >= VaR]`
#' (the manuscript's tail mean).
#'
#' `simulate_credit_losses()` draws the portfolio loss in Monte Carlo scenarios with this package's counter-based
#' random numbers (not R's), the same on any number of threads; `compute_loss_risk()` summarizes them.
#'
#' `compute_asrf_var()` is the asymptotic single risk factor quantile
#' `sum EAD LGD pnorm((qnorm(PD) + sqrt(rho) qnorm(alpha)) / sqrt(1 - rho))`, the limit for a portfolio in
#' which no obligor carries a noticeable share. `compute_irb_capital()` is the Basel IRB formula for corporates
#' (CRE31): correlation `R(PD)`, maturity adjustment `b(PD)`, capital `K` per unit of EAD and risk-weighted
#' assets `12.5 K EAD`. The PD is used as given; no floor is applied.
#'
#' @param pd,ead,lgd,rho Per obligor (recycled to the longest).
#' @param z Values of the common factor.
#' @param unit The loss lattice: every `EAD LGD` must be a whole number of it.
#' @param panels Panels of the quadrature over `z`.
#' @param distribution From `compute_credit_loss_distribution()`.
#' @param confidence_level Confidence level(s).
#' @param scenarios,seed,threads The simulation; `threads = 0` uses every logical processor.
#' @param maturity Effective maturity in years.
#' @return `compute_conditional_pd()`, `simulate_credit_losses()`, `compute_asrf_var()` numeric vectors.
#'   `compute_credit_loss_distribution()` a tibble with `loss`, `probability` and `cumulative_probability`, with
#'   attributes `mean` and `standard_deviation`. `compute_credit_risk()` and `compute_irb_capital()` tibbles.
#' @examples
#' pd <- seq(0.005, 0.05, length.out = 100)
#' d <- compute_credit_loss_distribution(pd, ead = 100, lgd = 0.6, rho = 0.2, unit = 60)
#' compute_credit_risk(d, c(0.99, 0.999))
#' compute_asrf_var(pd, 100, 0.6, 0.2, 0.999)
#' losses <- simulate_credit_losses(pd, 100, 0.6, 0.2, scenarios = 20000, seed = 1)
#' compute_loss_risk(losses, 0.99)
#' compute_irb_capital(c(0.001, 0.01, 0.05), lgd = 0.45)
#' @export
compute_conditional_pd <- function(pd, rho, z) {
  n <- max(length(pd), length(rho), length(z))
  checked(.Call(wrap__compute_conditional_pd, rep_len(as.numeric(pd), n), rep_len(as.numeric(rho), n), rep_len(as.numeric(z), n)))$value
}

credit_obligors <- function(pd, ead, lgd, rho) {
  n <- max(length(pd), length(ead), length(lgd), length(rho))
  list(pd = rep_len(as.numeric(pd), n), ead = rep_len(as.numeric(ead), n), lgd = rep_len(as.numeric(lgd), n), rho = rep_len(as.numeric(rho), n))
}

#' @rdname compute_conditional_pd
#' @export
compute_credit_loss_distribution <- function(pd, ead, lgd, rho, unit, panels = 200L) {
  o <- credit_obligors(pd, ead, lgd, rho)
  r <- checked(.Call(wrap__compute_credit_loss_distribution, o$pd, o$ead, o$lgd, o$rho, as.numeric(unit), as.integer(panels)))
  out <- tibble::tibble(loss = (seq_along(r$probability) - 1) * as.numeric(unit), probability = r$probability,
                        cumulative_probability = cumsum(r$probability))
  structure(out, mean = r$mean, standard_deviation = r$standard_deviation, unit = as.numeric(unit))
}

#' @rdname compute_conditional_pd
#' @export
compute_credit_risk <- function(distribution, confidence_level = c(0.95, 0.99, 0.999)) {
  unit <- attr(distribution, "unit")
  if (is.null(unit)) stop("`distribution` must come from compute_credit_loss_distribution().", call. = FALSE)
  r <- checked(.Call(wrap__compute_distribution_risk, distribution$probability, unit, as.numeric(confidence_level)))
  el <- attr(distribution, "mean")
  tibble::tibble(confidence_level = as.numeric(confidence_level), expected_loss = el, var = r$var, unexpected_loss = r$var - el,
                 es = r$es, tail_mean_es = r$tail_mean_es)
}

#' @rdname compute_conditional_pd
#' @export
simulate_credit_losses <- function(pd, ead, lgd, rho, scenarios = 100000, seed = 1, threads = 0L) {
  o <- credit_obligors(pd, ead, lgd, rho)
  checked(.Call(wrap__simulate_credit_losses, o$pd, o$ead, o$lgd, o$rho, as.numeric(scenarios), as.numeric(seed), as.integer(threads)))$loss
}

#' @rdname compute_conditional_pd
#' @export
compute_asrf_var <- function(pd, ead, lgd, rho, confidence_level = 0.999) {
  o <- credit_obligors(pd, ead, lgd, rho)
  checked(.Call(wrap__compute_asrf_var, o$pd, o$ead, o$lgd, o$rho, as.numeric(confidence_level)))$var
}

#' @rdname compute_conditional_pd
#' @export
compute_irb_capital <- function(pd, lgd, maturity = 2.5, ead = 1) {
  n <- max(length(pd), length(lgd), length(maturity), length(ead))
  args <- lapply(list(pd, lgd, maturity, ead), \(x) rep_len(as.numeric(x), n))
  r <- checked(.Call(wrap__compute_irb_capital, args[[1]], args[[2]], args[[3]], args[[4]]))
  tibble::tibble(pd = args[[1]], lgd = args[[2]], maturity = args[[3]], ead = args[[4]], correlation = r$correlation,
                 maturity_adjustment = r$maturity_adjustment, capital = r$capital, risk_weighted_assets = r$risk_weighted_assets)
}
