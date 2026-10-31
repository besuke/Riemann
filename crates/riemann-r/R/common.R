# Conversions at the border between R and Rust. Calculations are in Rust.

serial_of_epoch <- 25569L

# A Date or ISO strings (YYYY-MM-DD) to QuantLib serial numbers.
as_serial <- function(x, arg = deparse(substitute(x))) {
  if (inherits(x, "Date")) {
    dates <- x
  } else if (is.character(x)) {
    dates <- as.Date(x, format = "%Y-%m-%d")
    bad <- is.na(dates) & !is.na(x)
    if (any(bad)) stop("`", arg, "` is not an ISO date: ", x[bad][1], call. = FALSE)
  } else {
    stop("`", arg, "` must be a Date or an ISO date string (YYYY-MM-DD).", call. = FALSE)
  }
  if (anyNA(dates)) stop("`", arg, "` has a missing date.", call. = FALSE)
  as.integer(unclass(dates)) + serial_of_epoch
}

# NULL stands for no date, which is 0 in Rust.
as_optional_serial <- function(x, arg = deparse(substitute(x))) {
  if (is.null(x)) 0L else as_serial(x, arg)
}

as_date <- function(serial) {
  as.Date(serial - serial_of_epoch, origin = "1970-01-01")
}

# Logical arguments are logical, and NA is an error: a missing choice is never read as FALSE.
as_flag <- function(x, arg = deparse(substitute(x))) {
  if (!is.logical(x) || length(x) != 1L || is.na(x)) stop("`", arg, "` must be TRUE or FALSE.", call. = FALSE)
  x
}

# The same for a column. The integers go only to Rust; extendr has no vector of bool.
flags_for_rust <- function(x, arg = deparse(substitute(x))) {
  if (!is.logical(x)) stop("`", arg, "` must be logical.", call. = FALSE)
  if (anyNA(x)) stop("`", arg, "` has a missing value in row ", which(is.na(x))[1], ".", call. = FALSE)
  as.integer(x)
}

# Turns an expected failure, returned as a value by Rust, into an R error. A panic in Rust is a
# defect and does not come through here.
checked <- function(result) {
  if (!is.null(result$failure)) stop(result$failure, call. = FALSE)
  result
}

# Stops on the first row that failed in Rust, or keeps NA and the message in a column.
finish_rows <- function(rows, on_error) {
  on_error <- match.arg(on_error, c("stop", "na"))
  failed <- nzchar(rows$error)
  if (on_error == "stop" && any(failed)) {
    stop("row ", which(failed)[1], ": ", rows$error[failed][1], call. = FALSE)
  }
  rows$error[!failed] <- NA_character_
  rows
}

# A value column, for the functions that return one number per row.
finish_values <- function(rows, on_error) {
  on_error <- match.arg(on_error, c("stop", "na"))
  rows <- finish_rows(rows, on_error)
  if (on_error == "na" && any(!is.na(rows$error))) {
    warning(sum(!is.na(rows$error)), " rows failed; the first: ", rows$error[!is.na(rows$error)][1], call. = FALSE)
  }
  rows$value
}
