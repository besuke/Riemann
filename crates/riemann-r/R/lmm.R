#' Caplets and correlated shocks: building blocks of the LMM
#'
#' The examples of LagrangeFinance II-7.4. `compute_caplet_value()` values a caplet by Black's formula or
#' Bachelier's, as `notional * accrual * discount_factor * formula(forward, strike, volatility, expiry)`.
#' `compute_caplet_from_normals()` values it from the lognormal marginal of its forward rate under its own
#' forward measure, `L(T) = L(0) exp(-v^2 T / 2 + v sqrt(T) z)`, for the standard normals `z` given.
#' `compute_correlated_normals()` turns rows of independent normals `Z` into `Z %*% chol(correlation)`, and
#' `generate_correlated_normals()` does the same with this package's random numbers.
#'
#' These are not a LIBOR market model: there is no drift under a common measure and no joint path of
#' several forward rates. The random numbers are arguments, so that the same draws give the same values.
#'
#' @param forward,strike,volatility,expiry,accrual,discount_factor,notional The caplet. `volatility` is
#'   lognormal for `"black"` and in rate units for `"bachelier"`; `expiry` and `accrual` in years.
#' @param model `"black"` or `"bachelier"`.
#' @param on_error `"stop"` or `"na"`; see [RiemannFinMath-package].
#' @param normals Standard normal numbers: a vector for `compute_caplet_from_normals()`, a matrix with one
#'   row per draw for `compute_correlated_normals()`.
#' @param correlation A correlation matrix.
#' @param count Number of rows.
#' @param seed A whole number that selects the random numbers.
#' @return `compute_caplet_value()` a numeric vector. `compute_caplet_from_normals()` a one-row tibble with
#'   `value`, `standard_error` and `paths`. The others a matrix with one row per draw.
#' @examples
#' df <- exp(-0.02 * 2.5)
#' compute_caplet_value(0.025, 0.0275, 0.24, 2, 0.5, df)
#' compute_caplet_from_normals(rnorm(10000), 0.025, 0.0275, 0.24, 2, 0.5, df)
#' r <- matrix(c(1, 0.8, 0.8, 1), 2)
#' cor(generate_correlated_normals(10000, r))
#' @export
compute_caplet_value <- function(forward, strike, volatility, expiry, accrual, discount_factor, notional = 1, model = "black", on_error = "stop") {
  i <- tibble::tibble(forward = as.numeric(forward), strike = as.numeric(strike), volatility = as.numeric(volatility), expiry = as.numeric(expiry),
                      accrual = as.numeric(accrual), discount_factor = as.numeric(discount_factor), notional = as.numeric(notional))
  finish_values(checked(.Call(wrap__compute_caplet_value, as.character(model), i$forward, i$strike, i$volatility, i$expiry, i$accrual,
                              i$discount_factor, i$notional)), on_error)
}

#' @rdname compute_caplet_value
#' @export
compute_caplet_from_normals <- function(normals, forward, strike, volatility, expiry, accrual, discount_factor, notional = 1) {
  tibble::as_tibble(checked(.Call(wrap__compute_caplet_from_normals, as.numeric(normals), as.numeric(forward), as.numeric(strike),
                                  as.numeric(volatility), as.numeric(expiry), as.numeric(accrual), as.numeric(discount_factor), as.numeric(notional))))
}

#' @rdname compute_caplet_value
#' @export
compute_correlated_normals <- function(normals, correlation) {
  normals <- as.matrix(normals); correlation <- as.matrix(correlation)
  if (ncol(normals) != ncol(correlation)) stop("`normals` needs one column per row of `correlation`.", call. = FALSE)
  v <- checked(.Call(wrap__compute_correlated_normals, as.numeric(t(normals)), as.numeric(t(correlation)), as.integer(ncol(correlation))))$values
  matrix(v, ncol = ncol(correlation), byrow = TRUE)
}

#' @rdname compute_caplet_value
#' @export
generate_correlated_normals <- function(count, correlation, seed = 1) {
  correlation <- as.matrix(correlation)
  v <- checked(.Call(wrap__generate_correlated_normals, as.numeric(t(correlation)), as.integer(ncol(correlation)), as.numeric(count), as.numeric(seed)))$values
  matrix(v, ncol = ncol(correlation), byrow = TRUE)
}
