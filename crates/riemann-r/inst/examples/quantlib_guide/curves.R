# QuantLib Guide: term structures given directly
#
# What it computes: discount factors, forward rates and a spreaded curve from the pages "Term structures and
# their reference dates" and "The common interface of interest-rate term structures"
# (https://www.quantlibguide.com/), and a bond priced on each curve. RiemannFinMath's Rust functions do the
# calculation; the table compares them with the Guide's printed values.
#
# Run:   source(system.file("examples", "quantlib_guide", "curves.R", package = "RiemannFinMath"))
# Needs: RiemannFinMath and tibble.
#
# Conditions: "Term structures", evaluation date 3 October 2024, a flat 5% continuous Actual/360 curve.
# "Common interface", evaluation date 2 May 2019: a flat 1% continuous Actual/365 (Fixed) curve, and
# discount factors at yearly dates for 10 years, Actual/360, log-linear; the spread is 0.1% continuous; the
# forward rate is 30/360 (Bond Basis), compounded semiannually; the bond runs from 8 February 2013 to
# 8 February 2023, 6-monthly, TARGET, Following, coupon 3%, 30/360 (Bond Basis), 3 settlement days.
#
# Not reproduced (docs/quantlib_guide_inventory.md): a reference date that moves with the evaluation date
# (RiemannFinMath curves keep the date they are given), the Observer notifications, the Euribor forecasts.

library(RiemannFinMath)
rows <- list()
add <- function(page, item, guide, riemann, note = "") {
  rows[[length(rows) + 1L]] <<- tibble::tibble(page = page, item = item, guide = guide, riemann = riemann,
                                               riemann_minus_guide = riemann - guide, note = note)
}

page <- "Term structures"
flat <- create_flat_curve("2024-10-03", 0.05, "actual_360")
add(page, "Discount factor on 3 October 2029", 0.7759935186328056, compute_discount(flat, as.Date("2029-10-03")))
moved <- create_flat_curve("2024-09-19", 0.05, "actual_360")
add(page, "Same, curve built on 19 September 2024", 0.7744861083592839, compute_discount(moved, as.Date("2029-10-03")),
    "QuantLib moves the reference date with the evaluation date; here the curve is built on the new date")

page <- "Common interface"
today <- as.Date("2019-05-02")
curve_1 <- create_flat_curve(today, 0.01, "actual_365_fixed")
curve_2 <- create_discount_curve(seq(today, by = "year", length.out = 11),
                                 c(1.0, 0.99, 0.98, 0.96, 0.95, 0.93, 0.92, 0.90, 0.88, 0.87, 0.86), "actual_360")
add(page, "Flat curve: discount factor on 15 June 2020", 0.9888299764864131, compute_discount(curve_1, as.Date("2020-06-15")))
add(page, "Discount curve: discount factor on 15 June 2020", 0.9887891320662344, compute_discount(curve_2, as.Date("2020-06-15")))
add(page, "Flat curve: forward rate 2020-2021, %", 1.002504,
    100 * compute_forward_rate(curve_1, "2020-06-15", "2021-06-15", "thirty_360_bond_basis", "compounded", 2L), "Guide prints 6 decimals")
add(page, "Discount curve: forward rate 2020-2021, %", 1.144677,
    100 * compute_forward_rate(curve_2, "2020-06-15", "2021-06-15", "thirty_360_bond_basis", "compounded", 2L), "Guide prints 6 decimals")
curve_3 <- create_zero_spreaded_curve(curve_2, 0.001)
add(page, "Discount curve: discount factor on 15 June 2021", 0.9775671243960207, compute_discount(curve_2, as.Date("2021-06-15")))
add(page, "Spreaded by 0.1%: discount factor on 15 June 2021", 0.9754649032337227, compute_discount(curve_3, as.Date("2021-06-15")))
bond <- create_fixed_rate_bond(3L, 100, "2013-02-08", "2023-02-08", "6M", "target", "following", "following", "backward", FALSE,
                               0.03, "thirty_360_bond_basis", "following", 100)
add(page, "Bond clean price on the discount curve", 106.21125788897203, compute_bond_value_on_curve(bond, curve_2, today)$clean_price)
add(page, "Bond clean price on the flat curve", 107.32867290510617, compute_bond_value_on_curve(bond, curve_1, today)$clean_price)

results <- do.call(rbind, rows)
cat("QuantLib Guide, term structures given directly\n\n")
old <- options(width = 250)
print(as.data.frame(results), digits = 12, row.names = FALSE, right = FALSE)
options(old)
invisible(results)
