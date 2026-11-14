# Recompute the BB price from the published simple yield with RiemannFinMath, and compare with the published price.
#
#   Rscript tools/market/check_bb_price_from_simple.R
#
# Reads market_data/bb_price/bb_price_*.txt and writes market_data/bb_price/check_price_from_simple.txt (both ignored
# by git). Prints counts only, no quotes. Coupon-bearing JGBs only (not TDB, TIPS or when-issued rows).
#
# Remaining years: compute_remaining_years(start, maturity) (whole years plus days / 365, truncated to 7 decimals).
# The start date is tried two ways: the file's reference date (基準日), and the business day before it (the month's
# last business day, the date the closing prices are for).

suppressPackageStartupMessages(library(RiemannFinMath))
dir <- file.path("market_data", "bb_price")
files <- list.files(dir, pattern = "^bb_price_\\d{8}\\.txt$", full.names = TRUE)
rows <- do.call(rbind, lapply(files, function(f) {
  x <- utils::read.delim(f, sep = "|", colClasses = "character")
  x$file <- basename(f)
  x
}))
x <- rows[rows$category %in% c("40Y", "30Y", "20Y", "10Y", "5Y", "2Y") & rows$when_issued == "FALSE" &
          nzchar(rows$price) & nzchar(rows$simple_yield_pct) & nzchar(rows$coupon_pct), ]
x <- x[as.Date(x$maturity_date) > as.Date(x$reference_date), ]
prev_business_day <- function(d) {
  d <- as.Date(d) - 1
  while (format(d, "%u") %in% c("6", "7")) d <- d - 1  # weekends; no Japanese holiday falls on these month ends
  d
}
starts <- list(reference_date = as.Date(x$reference_date),
               last_business_day = as.Date(vapply(x$reference_date, function(d) format(prev_business_day(d)), "")))
out <- x[, c("file", "category", "issue_label", "reference_date", "maturity_date")]
summary <- NULL
for (nm in names(starts)) {
  t <- compute_remaining_years(starts[[nm]], as.Date(x$maturity_date))
  p <- compute_price_from_simple_rate(t, as.numeric(x$coupon_pct), as.numeric(x$simple_yield_pct), 100, digits = 3L)
  d <- p - as.numeric(x$price)
  out[[paste0("t_", nm)]] <- t
  out[[paste0("price_", nm)]] <- p
  out[[paste0("diff_", nm)]] <- round(d, 6)
  summary <- rbind(summary, data.frame(start = nm, rows = length(d), exact = sum(abs(d) < 5e-7),
                                       within_0.001 = sum(abs(d) <= 0.0010001), max_abs_diff = max(abs(d))))
}
utils::write.table(out, file.path(dir, "check_price_from_simple.txt"), sep = "|", quote = FALSE, row.names = FALSE)
print(summary, row.names = FALSE)
by_cat <- aggregate(cbind(exact_ref = abs(out$diff_reference_date) < 5e-7, exact_last = abs(out$diff_last_business_day) < 5e-7) ~ file + category, out, sum)
by_cat$rows <- aggregate(issue_label ~ file + category, out, length)$issue_label
print(by_cat, row.names = FALSE)
