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

#' Values many bonds, one row each. `bonds` has the arguments of `fixed_rate_bond()` as columns
#' (`end_of_month` is logical, without NA),
#' and `rate`, `yield_day_count`, `compounding`, `frequency` and `evaluation_date`. `threads = 0`
#' uses one thread per logical processor. A row that fails has NA and its message in `error`.
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

#' Builds many OIS curves and reads each on its dates, one row per curve and date. `curves` has
#' the columns `evaluation_date`, `reference_date`, `index`, `curve_day_count`, a list column
#' `instruments` of `ois_instruments()` tables and a list column `dates`, and optionally
#' `interpolation` and `accuracy`. A curve that fails has one row with its message in `error`.
#' `max_reproduction_error` shows how far the curve misses its quotes.
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
