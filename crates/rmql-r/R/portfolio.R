#' Constrained portfolio optimization and risk contributions
#'
#' Portfolios of assets from a history of returns, under a sum of the weights and lower and upper bounds
#' per asset (LagrangeFinance I-8). Every method is solved exactly, not by a general-purpose search:
#'
#' - `minimum_variance`, `mean_variance` (`max mu'w - (risk_aversion / 2) w'Sigma w`), `target_return`
#'   (minimum variance with `mu'w >= target_return`): quadratic programs, dual active-set method of
#'   Goldfarb and Idnani.
#' - `maximum_return`: closed form (assets in decreasing order of mean return, up to their bounds).
#' - `maximum_sharpe` (`(mu'w - risk_free_rate) / sd`): the quadratic program in `y = kappa w`.
#' - `minimum_ES`: the linear program of Rockafellar and Uryasev (simplex method).
#' - `risk_budget`: Spinu's convex problem by Newton's method; the risk contributions
#'   `w_i (Sigma w)_i / w'Sigma w` equal `risk_budget` (equal by default). Bounds that bind are not supported.
#' - `target_risk`: the largest return with volatility at most `target_volatility` (bisection on the frontier).
#'
#' The mean and covariance are the sample ones (divisor `T - 1`) of the periods without missing returns.
#' `risk_aversion` is the `gamma` of `mu'w - (gamma / 2) w'Sigma w`; GaussQuant's `risk_aversion` is
#' `lambda` in `mu'w - lambda w'Sigma w`, so GaussQuant's 5 is 10 here.
#'
#' `compute_portfolio_risk()` gives the volatility, normal VaR and ES, historical VaR (R's
#' `quantile(type = 7)` of the losses) and ES, and the Euler contributions `w_i dR/dw_i` of the volatility,
#' the normal VaR and ES and the historical ES, by automatic differentiation; each set adds up to its
#' measure. The historical ES is Rockafellar and Uryasev's: the mean of the largest `(1 - alpha) T` losses,
#' the last counted in part. `tail_mean_es`, the mean of the losses at or above the historical VaR
#' (GaussQuant's definition), is given for comparison.
#'
#' @param returns A matrix or data frame of returns with one column per asset (named), or a long data frame
#'   with the columns `date_col`, `asset_col` and `return_col`. Periods with a missing return are removed.
#' @param assets Asset names, in the order of the weights.
#' @param weight_sum The sum of the weights.
#' @param lower,upper Bounds per asset (recycled); `-Inf` and `Inf` for none.
#' @param constraints From `create_portfolio_constraints()`.
#' @param method One of the methods above.
#' @param risk_aversion,risk_free_rate,confidence_level,target_return,target_volatility,risk_budget The
#'   method's parameters; returns and rates are per period.
#' @param target_returns Target returns of the frontier's points.
#' @param weights Weights, named by asset or in the order of the columns.
#' @param date_col,asset_col,return_col Columns of a long `returns`.
#' @return `create_portfolio_constraints()` a list of class `rmql_portfolio_constraints`.
#'   `compute_return_moments()` a list with `mean`, `covariance`, `periods` and `removed_periods`.
#'   `compute_optimal_portfolio()` a list with `weights` (a tibble of `asset`, `weight`, `at_lower`,
#'   `at_upper`), `statistics` (a tibble of `metric`, `value`), `method`, `solver`, `iterations` and
#'   `stationarity_residual` (`max |G x + a - sum u_i n_i|` of the quadratic program; 0 otherwise).
#'   `compute_efficient_frontier()` a tibble with `target_return`, `expected_return`, `volatility` and the
#'   weights as a list column. `compute_portfolio_risk()` a list with `statistics` and `contributions`
#'   (a tibble by asset).
#' @examples
#' set.seed(1)
#' r <- matrix(rnorm(60 * 4, 0.01, 0.05), 60, dimnames = list(NULL, c("A", "B", "C", "D")))
#' constraints <- create_portfolio_constraints(colnames(r), lower = 0, upper = 0.5)
#' compute_optimal_portfolio(r, constraints, "minimum_variance")$weights
#' compute_optimal_portfolio(r, constraints, "maximum_sharpe")$statistics
#' p <- compute_optimal_portfolio(r, constraints, "minimum_ES", confidence_level = 0.9)
#' compute_portfolio_risk(r, p$weights$weight, confidence_level = 0.9)$contributions
#' @export
create_portfolio_constraints <- function(assets, weight_sum = 1, lower = 0, upper = 1) {
  assets <- as.character(assets)
  if (!length(assets) || anyNA(assets) || anyDuplicated(assets)) stop("`assets` must be distinct names.", call. = FALSE)
  n <- length(assets)
  if (!is.numeric(lower) || !is.numeric(upper) || !length(lower) || !length(upper)) stop("`lower` and `upper` must be numbers.", call. = FALSE)
  lower <- rep_len(as.numeric(lower), n)
  upper <- rep_len(as.numeric(upper), n)
  if (!is.numeric(weight_sum) || length(weight_sum) != 1L) stop("`weight_sum` must be one number.", call. = FALSE)
  checked(.Call(wrap__check_portfolio_constraints, as.integer(n), as.numeric(weight_sum), lower, upper))
  structure(list(assets = assets, weight_sum = as.numeric(weight_sum), lower = stats::setNames(lower, assets),
                 upper = stats::setNames(upper, assets)), class = "rmql_portfolio_constraints")
}

#' @export
print.rmql_portfolio_constraints <- function(x, ...) {
  cat(sprintf("<rmql_portfolio_constraints> %d assets, weights sum to %g, bounds [%g, %g] to [%g, %g]\n",
              length(x$assets), x$weight_sum, min(x$lower), max(x$lower), min(x$upper), max(x$upper)))
  invisible(x)
}

# The returns as a numeric matrix with the assets' columns, without periods that have a missing return.
portfolio_return_matrix <- function(returns, assets = NULL, date_col = "date", asset_col = "asset", return_col = "return") {
  if (is.data.frame(returns) && all(c(date_col, asset_col, return_col) %in% names(returns))) {
    dates <- returns[[date_col]]
    names_long <- as.character(returns[[asset_col]])
    periods <- sort(unique(dates))
    columns <- if (is.null(assets)) unique(names_long) else assets
    m <- matrix(NA_real_, length(periods), length(columns), dimnames = list(NULL, columns))
    keep <- names_long %in% columns
    m[cbind(match(dates[keep], periods), match(names_long[keep], columns))] <- as.numeric(returns[[return_col]][keep])
  } else {
    m <- as.matrix(returns)
    if (!is.numeric(m)) stop("`returns` must be numeric, one column per asset, or long with `", date_col, "`, `", asset_col, "` and `", return_col, "`.", call. = FALSE)
    if (!is.null(assets)) {
      if (is.null(colnames(m))) {
        if (ncol(m) != length(assets)) stop("`returns` has ", ncol(m), " columns for ", length(assets), " assets.", call. = FALSE)
        colnames(m) <- assets
      }
      missing <- setdiff(assets, colnames(m))
      if (length(missing)) stop("`returns` has no column for ", paste(missing, collapse = ", "), ".", call. = FALSE)
      m <- m[, assets, drop = FALSE]
    }
  }
  storage.mode(m) <- "double"
  complete <- stats::complete.cases(m)
  structure(m[complete, , drop = FALSE], removed_periods = sum(!complete))
}

check_portfolio_constraints <- function(constraints) {
  if (!inherits(constraints, "rmql_portfolio_constraints")) stop("`constraints` must come from create_portfolio_constraints().", call. = FALSE)
}

# By rows, as Rust reads them: period 1's assets, then period 2's.
by_rows <- function(m) as.numeric(t(m))

#' @rdname create_portfolio_constraints
#' @export
compute_return_moments <- function(returns, assets = NULL, date_col = "date", asset_col = "asset", return_col = "return") {
  m <- portfolio_return_matrix(returns, assets, date_col, asset_col, return_col)
  r <- checked(.Call(wrap__compute_return_moments, by_rows(m), ncol(m)))
  names_m <- colnames(m)
  list(mean = stats::setNames(r$mean, names_m),
       covariance = matrix(r$covariance, ncol(m), byrow = TRUE, dimnames = list(names_m, names_m)),
       periods = r$periods, removed_periods = attr(m, "removed_periods"))
}

portfolio_methods <- c("minimum_variance", "mean_variance", "maximum_return", "maximum_sharpe", "minimum_ES",
                       "risk_budget", "target_return", "target_risk")

#' @rdname create_portfolio_constraints
#' @export
compute_optimal_portfolio <- function(returns, constraints, method = portfolio_methods, risk_aversion = 1, risk_free_rate = 0,
                                      confidence_level = 0.95, target_return = NULL, target_volatility = NULL, risk_budget = NULL,
                                      date_col = "date", asset_col = "asset", return_col = "return") {
  check_portfolio_constraints(constraints)
  method <- match.arg(method)
  if (method == "target_return" && is.null(target_return)) stop("`target_return` is needed for method \"target_return\".", call. = FALSE)
  if (method == "target_risk" && is.null(target_volatility)) stop("`target_volatility` is needed for method \"target_risk\".", call. = FALSE)
  one <- function(x, arg) {
    if (is.null(x)) return(NA_real_)
    if (!is.numeric(x) || length(x) != 1L) stop("`", arg, "` must be one number.", call. = FALSE)
    as.numeric(x)
  }
  m <- portfolio_return_matrix(returns, constraints$assets, date_col, asset_col, return_col)
  n <- ncol(m)
  budget <- if (is.null(risk_budget)) rep(1, n) else as.numeric(risk_budget)
  parameters <- c(one(risk_aversion, "risk_aversion"), one(risk_free_rate, "risk_free_rate"), one(confidence_level, "confidence_level"),
                  one(target_return, "target_return"), one(target_volatility, "target_volatility"))
  r <- checked(.Call(wrap__compute_optimal_portfolio, by_rows(m), n, constraints$weight_sum, unname(constraints$lower),
                     unname(constraints$upper), method, parameters, budget))
  risk <- compute_portfolio_risk(m, r$weights, confidence_level = if (is.na(parameters[3])) 0.95 else parameters[3])$statistics
  list(
    weights = tibble::tibble(asset = constraints$assets, weight = r$weights, at_lower = r$at_lower, at_upper = r$at_upper),
    statistics = tibble::tibble(
      metric = c("expected_return", "volatility", "variance", "sharpe_ratio", "objective", risk$metric[-(1:2)]),
      value = c(r$expected_return, r$volatility, r$volatility^2, (r$expected_return - parameters[2]) / r$volatility, r$objective,
                risk$value[-(1:2)])
    ),
    method = method, solver = r$solver, iterations = r$iterations, stationarity_residual = r$stationarity_residual,
    periods = nrow(m), removed_periods = attr(m, "removed_periods")
  )
}

#' @rdname create_portfolio_constraints
#' @export
compute_efficient_frontier <- function(returns, constraints, target_returns, date_col = "date", asset_col = "asset", return_col = "return") {
  check_portfolio_constraints(constraints)
  m <- portfolio_return_matrix(returns, constraints$assets, date_col, asset_col, return_col)
  n <- ncol(m)
  r <- checked(.Call(wrap__compute_efficient_frontier, by_rows(m), n, constraints$weight_sum, unname(constraints$lower),
                     unname(constraints$upper), as.numeric(target_returns)))
  w <- matrix(r$weights, ncol = n, byrow = TRUE, dimnames = list(NULL, constraints$assets))
  tibble::tibble(target_return = as.numeric(target_returns), expected_return = r$expected_return, volatility = r$volatility,
                 weights = purrr::map(seq_len(nrow(w)), \(k) w[k, ]))
}

#' @rdname create_portfolio_constraints
#' @export
compute_portfolio_risk <- function(returns, weights, confidence_level = 0.95, date_col = "date", asset_col = "asset", return_col = "return") {
  assets <- if (!is.null(names(weights))) names(weights) else NULL
  m <- portfolio_return_matrix(returns, assets, date_col, asset_col, return_col)
  if (length(weights) != ncol(m)) stop("`weights` has ", length(weights), " values for ", ncol(m), " assets.", call. = FALSE)
  r <- checked(.Call(wrap__compute_portfolio_risk, by_rows(m), ncol(m), as.numeric(weights), as.numeric(confidence_level)))
  stats_names <- c("expected_return", "volatility", "normal_var", "normal_es", "historical_var", "historical_es", "tail_mean_es")
  list(
    statistics = tibble::tibble(metric = stats_names, value = purrr::map_dbl(stats_names, \(k) r[[k]])),
    contributions = tibble::tibble(asset = if (is.null(colnames(m))) as.character(seq_len(ncol(m))) else colnames(m),
                                   weight = as.numeric(weights), volatility = r$volatility_contribution, normal_var = r$normal_var_contribution,
                                   normal_es = r$normal_es_contribution, historical_es = r$historical_es_contribution)
  )
}
