# Recompute the BB price from the published simple yield with RiemannFinMath, and compare with the published price.
#
#   Rscript tools/market/check_bb_price_from_simple.R
#
# Reads market_data/bb_price/bb_price_*.txt and writes market_data/bb_price/check_price_from_simple.txt (both ignored
# by git). Prints counts only, no quotes. Coupon-bearing JGBs only (not TDB, TIPS or when-issued rows).
#
# Remaining years: compute_remaining_years(start, maturity) (whole years plus days / 365, truncated to 7 decimals).
# The start date is tried two ways: the file's reference date, and the business day before it (the month's last
# business day, the date the closing prices are for). The price is truncated to 3 decimals, as published.

suppressPackageStartupMessages({
  library(dplyr)
  library(purrr)
  library(tidyr)
  library(readr)
  library(RiemannFinMath)
})

in_dir <- file.path("market_data", "bb_price")

# The business day before a date; weekends only, as no Japanese holiday falls on these month ends.
previous_business_day <- function(date) {
  map_vec(date, \(d) {
    d <- d - 1
    while (format(d, "%u") %in% c("6", "7")) d <- d - 1
    d
  })
}

bonds <- list.files(in_dir, pattern = "^bb_price_\\d{8}\\.txt$", full.names = TRUE) |>
  set_names(basename) |>
  map(\(path) read_delim(path, delim = "|", col_types = cols(.default = col_character()), na = "")) |>
  list_rbind(names_to = "file") |>
  filter(
    category %in% c("40Y", "30Y", "20Y", "10Y", "5Y", "2Y"),
    when_issued == "FALSE",
    !is.na(price), !is.na(simple_yield_pct), !is.na(coupon_pct)
  ) |>
  mutate(
    across(c(reference_date, maturity_date), as.Date),
    across(c(coupon_pct, simple_yield_pct, price), as.numeric)
  ) |>
  filter(maturity_date > reference_date)

checked <- bonds |>
  mutate(last_business_day = previous_business_day(reference_date)) |>
  pivot_longer(c(reference_date, last_business_day), names_to = "start", values_to = "start_date") |>
  mutate(
    t = compute_remaining_years(start_date, maturity_date),
    price_from_simple = compute_price_from_simple_rate(t, coupon_pct, simple_yield_pct, 100, digits = 3L),
    difference = round(price_from_simple - price, 6)
  )

checked |>
  select(file, category, issue_label, start, start_date, maturity_date, t, price_from_simple, difference) |>
  write_delim(file.path(in_dir, "check_price_from_simple.txt"), delim = "|", na = "", eol = "\n")

checked |>
  group_by(start) |>
  summarise(
    rows = n(),
    exact = sum(abs(difference) < 5e-7),
    within_0.001 = sum(abs(difference) <= 0.0010001),
    max_abs_diff = max(abs(difference)),
    .groups = "drop"
  ) |>
  print()

checked |>
  group_by(file, category, start) |>
  summarise(exact = sum(abs(difference) < 5e-7), rows = n(), .groups = "drop") |>
  pivot_wider(names_from = start, values_from = exact, names_prefix = "exact_") |>
  print(n = Inf)
