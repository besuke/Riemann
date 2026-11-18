# IRRBB interest rate shock scenarios (Basel Framework SRP31.90-31.93, in force from 1 January 2026). Calculations are in
# Rust (crates/rmql/src/irrbb.rs).

irrbb_size_args <- function(currency, sizes) {
  s <- if (is.null(sizes)) get_irrbb_shock_sizes(currency) else sizes
  missing_names <- setdiff(c("parallel_bp", "short_bp", "long_bp"), names(s))
  if (length(missing_names)) stop("`sizes` lacks: ", paste(missing_names, collapse = ", "), call. = FALSE)
  s
}

#' IRRBB interest rate shock scenarios
#'
#' The six standardised shock scenarios of the Basel Framework, SRP31.90-31.93 (the version in force from
#' 1 January 2026): parallel up and down, steepener, flattener, short rates up and down. For a tenor `t` in
#' years and `x = 4`: parallel `+-P`; short `+-S exp(-t/x)`; long `+-L (1 - exp(-t/x))`; steepener
#' `-0.65 |short| + 0.9 |long|`; flattener `+0.8 |short| - 0.6 |long|`. The yen example of SRP31.92 gives
#' +25.4 bp for the steepener at 3.5 years.
#'
#' `get_irrbb_shock_sizes()` returns the parallel, short and long shock sizes of SRP31.90 Table 2 for the
#' currencies read from the table (JPY, USD, EUR, GBP, CHF, CAD, CNY, ZAR); other currencies need `sizes`.
#'
#' `create_irrbb_shocked_curve()` takes the base curve's zero rates (Actual/365 Fixed, continuously
#' compounded) at `dates`, adds the shock at each date's tenor, applies the floor, and returns a zero curve
#' on those dates (linear in the zero rates). `scenario = "base"` rebuilds the base curve on the same
#' dates, so that differences in value come from the shock alone. The floor (SRP31.93: set by national
#' supervisors, not greater than zero) is `NULL` for none; a rate already below the floor is not raised:
#' `max(base + shock, min(base, floor))`. `compute_irrbb_curve_points()` returns the same points as a table.
#'
#' @param currency A currency code of SRP31.90 Table 2.
#' @param times Tenors in years.
#' @param scenario `"parallel_up"`, `"parallel_down"`, `"steepener"`, `"flattener"`, `"short_up"`,
#'   `"short_down"` (or `"base"` for the curve functions).
#' @param sizes A list with `parallel_bp`, `short_bp` and `long_bp`, or `NULL` for the table of `currency`.
#' @param x The decay of the short shock (4 in SRP31.91).
#' @param curve A curve (e.g. from `bootstrap_ois_curve()`).
#' @param dates Dates of the shocked curve: the curve's reference date first, increasing.
#' @param floor Floor on the post-shock zero rates as a decimal (not greater than 0), or `NULL`.
#' @return `get_irrbb_shock_sizes()`: a list of the three sizes in bp. `compute_irrbb_shocks()`: shocks in
#'   bp. `compute_irrbb_curve_points()`: a tibble. `create_irrbb_shocked_curve()`: a curve.
#' @examples
#' get_irrbb_shock_sizes("JPY")
#' compute_irrbb_shocks(3.5, "steepener", get_irrbb_shock_sizes("JPY"))
#' @export
get_irrbb_shock_sizes <- function(currency) {
  r <- checked(.Call(wrap__get_irrbb_shock_sizes, currency))
  list(parallel_bp = r$parallel_bp, short_bp = r$short_bp, long_bp = r$long_bp)
}

#' @rdname get_irrbb_shock_sizes
#' @export
compute_irrbb_shocks <- function(times, scenario, sizes, x = 4) {
  s <- irrbb_size_args(NULL, sizes)
  checked(.Call(wrap__compute_irrbb_shocks, as.numeric(times), scenario, as.numeric(s$parallel_bp), as.numeric(s$short_bp),
                as.numeric(s$long_bp), as.numeric(x)))$shock_bp
}

#' @rdname get_irrbb_shock_sizes
#' @export
compute_irrbb_curve_points <- function(curve, scenario, dates, currency = "JPY", sizes = NULL, x = 4, floor = NULL) {
  s <- irrbb_size_args(currency, sizes)
  r <- checked(.Call(wrap__compute_irrbb_curve_points, curve$pointer, scenario, as_serial(dates), as.numeric(s$parallel_bp),
                     as.numeric(s$short_bp), as.numeric(s$long_bp), as.numeric(x), if (is.null(floor)) numeric() else as.numeric(floor)))
  tibble(date = as_date(r$date), time = r$time, base_zero = r$base_zero, shock_bp = r$shock * 1e4, shocked_zero = r$shocked_zero,
         floored = r$floored)
}

#' @rdname get_irrbb_shock_sizes
#' @export
create_irrbb_shocked_curve <- function(curve, scenario, dates, currency = "JPY", sizes = NULL, x = 4, floor = NULL) {
  if (!inherits(curve, "rmql_ois_curve")) stop("`curve` must be a curve.", call. = FALSE)
  s <- irrbb_size_args(currency, sizes)
  pointer <- checked(.Call(wrap__create_irrbb_shocked_curve, curve$pointer, scenario, as_serial(dates), as.numeric(s$parallel_bp),
                           as.numeric(s$short_bp), as.numeric(s$long_bp), as.numeric(x),
                           if (is.null(floor)) numeric() else as.numeric(floor)))$value
  given(pointer, paste0("irrbb-", scenario), curve$index)
}
