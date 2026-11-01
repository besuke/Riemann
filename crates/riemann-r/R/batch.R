# Many evaluations in parallel. On R's main thread, Rust converts the inputs to plain Rust values;
# the threads compute with those values only, without calling R; and the columns are made back on
# R's main thread, in the order of the inputs. The results do not depend on the number of threads.
# Every count runs on worker threads, also 1. They can differ in the last bits from the direct
# functions, which run on R's main thread (docs/spec_r.md, section 5).

bond_batch_columns <- c(
  "settlement_days", "face_amount", "effective_date", "termination_date", "tenor", "calendar", "convention",
  "termination_convention", "rule", "end_of_month", "coupon_rate", "day_count", "payment_convention", "redemption",
  "issue_date", "rate", "yield_day_count", "compounding", "frequency", "evaluation_date"
)

#' Value many bonds in parallel
#'
#' The threads compute with Rust values only and never call R. Results and their order do not
#' depend on the number of threads. Every count runs on worker threads, also 1, so results can
#' differ in the last bits from [bond_values()], which runs on R's main thread.
#'
#' @param bonds A data frame with one row for each evaluation: the arguments of
#'   [fixed_rate_bond()] as columns (`end_of_month` is logical without `NA`, and `issue_date` is
#'   required), and `rate`, `yield_day_count`, `compounding`, `frequency` and `evaluation_date`.
#' @param threads Number of threads. 0 uses one for each logical processor.
#' @return A tibble in the order of `bonds`, with the columns `settlement_date`, `clean_price`,
#'   `dirty_price`, `accrued_amount`, `duration_modified`, `convexity`, `basis_point_value`,
#'   `yield_value_basis_point` and `error`. A row that fails has `NA` and its message.
#' @examples
#' bonds <- data.frame(
#'   settlement_days = 1L, face_amount = 100, effective_date = as.Date("2020-03-20"),
#'   termination_date = as.Date("2030-03-20"), tenor = "6M", calendar = "japan",
#'   convention = "following", termination_convention = "following", rule = "backward",
#'   end_of_month = FALSE, coupon_rate = 0.015, day_count = "act_act_isma",
#'   payment_convention = "following", redemption = 100, issue_date = as.Date("2020-03-20"),
#'   rate = c(0.01, 0.02), yield_day_count = "act_act_isma", compounding = "compounded",
#'   frequency = 2L, evaluation_date = as.Date("2026-11-02")
#' )
#' bond_values_batch(bonds, threads = 2L)
#' @export
bond_values_batch <- function(bonds, threads = 0L) {
  missing_columns <- setdiff(bond_batch_columns, names(bonds))
  if (length(missing_columns)) stop("`bonds` lacks the columns: ", paste(missing_columns, collapse = ", "), call. = FALSE)
  rows <- checked(.Call(
    wrap__bond_values_batch,
    as.integer(bonds$settlement_days), as.numeric(bonds$face_amount), as_serial(bonds$effective_date), as_serial(bonds$termination_date),
    as.character(bonds$tenor), as.character(bonds$calendar), as.character(bonds$convention), as.character(bonds$termination_convention),
    as.character(bonds$rule), flags_for_rust(bonds$end_of_month, "end_of_month"), as.numeric(bonds$coupon_rate), as.character(bonds$day_count),
    as.character(bonds$payment_convention), as.numeric(bonds$redemption), as_serial(bonds$issue_date), as.numeric(bonds$rate),
    as.character(bonds$yield_day_count), as.character(bonds$compounding), as.integer(bonds$frequency), as_serial(bonds$evaluation_date),
    as.integer(threads)
  ))
  rows$error[!nzchar(rows$error)] <- NA_character_
  values <- as_tibble(rows)
  values$settlement_date <- as_date(values$settlement_date)
  values
}

#' Build and read many OIS curves in parallel
#'
#' The threads compute with Rust values only and never call R. Results and their order do not
#' depend on the number of threads. Every count runs on worker threads, also 1, so results can
#' differ in the last bits from [discount()] and the other direct functions, which run on R's main
#' thread.
#'
#' @param curves A data frame with one row for each curve: `evaluation_date`, `reference_date`,
#'   `index`, `curve_day_count`, a list column `instruments` of [ois_instruments()] tables, a list
#'   column `dates` of dates to read, and optionally `interpolation` and `accuracy`.
#' @param threads Number of threads. 0 uses one for each logical processor.
#' @return A tibble in the order of the curves and of their dates, with the columns `curve` (the
#'   row of `curves`), `date`, `discount`, `zero_continuous`, `forward_1d_simple`,
#'   `max_reproduction_error` and `error`. A curve that fails has one row, without a date, with its
#'   message. `max_reproduction_error` shows how far the curve misses its quotes.
#' @examples
#' instruments <- ois_instruments(c("ON", "1Y", "5Y"), c(0.002, 0.004, 0.009), 2L, 1L, 0L)
#' dates <- as.Date("2026-11-02") + c(365L, 1000L)
#' curves <- tibble::tibble(
#'   evaluation_date = as.Date("2026-11-02"), reference_date = as.Date("2026-11-02"),
#'   index = c("tonar", "sofr"), curve_day_count = "actual_365_fixed",
#'   instruments = list(instruments, instruments), dates = list(dates, dates)
#' )
#' curve_values_batch(curves, threads = 2L)
#' @export
curve_values_batch <- function(curves, threads = 0L) {
  n <- nrow(curves)
  interpolation <- if ("interpolation" %in% names(curves)) as.character(curves$interpolation) else rep("monotonic_log_cubic", n)
  accuracy <- if ("accuracy" %in% names(curves)) as.numeric(curves$accuracy) else rep(1e-12, n)
  ids <- seq_len(n)
  instruments <- do.call(rbind, Map(function(id, x) tibble(curve = id, x), ids, curves$instruments))
  date_serials <- lapply(curves$dates, as_serial)
  rows <- checked(.Call(
    wrap__curve_values_batch,
    as_serial(curves$evaluation_date), as_serial(curves$reference_date), as.character(curves$index), as.character(curves$curve_day_count),
    interpolation, accuracy,
    instruments$curve, instruments$tenor, instruments$rate, instruments$settlement_days, instruments$payment_frequency, instruments$payment_lag,
    rep(ids, lengths(date_serials)), as.integer(unlist(date_serials)),
    as.integer(threads)
  ))
  rows$error[!nzchar(rows$error)] <- NA_character_
  values <- as_tibble(rows)
  values$date <- as_date(ifelse(values$date == 0L, NA_integer_, values$date))
  values
}
