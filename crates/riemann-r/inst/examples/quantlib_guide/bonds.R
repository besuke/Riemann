# QuantLib Guide: fixed-rate bonds priced from a yield
#
# What it computes: the cash flows, settlement dates, accrued amounts, prices from a yield, a yield from a
# price, durations and convexity of the fixed-rate bonds in the QuantLib Guide pages "A taste of QuantLib",
# "Cash flows and bonds", "Cash-flow analysis" and "Vanilla bonds" (https://www.quantlibguide.com/).
# The calculations are RiemannFinMath's Rust functions. The table compares them with the values printed in
# the Guide and, when the QuantLib R package is installed, with the same calls to the local QuantLib.
#
# Run:   source(system.file("examples", "quantlib_guide", "bonds.R", package = "RiemannFinMath"))
# Needs: RiemannFinMath and tibble. QuantLib (the R package) is optional.
#
# Not reproduced here, because RiemannFinMath lacks the feature (docs/quantlib_guide_inventory.md):
# the yield with
# SimpleThenCompounded compounding, step-up and amortizing bonds, zero-coupon bonds on a curve, floaters.

library(RiemannFinMath)
has_quantlib <- requireNamespace("QuantLib", quietly = TRUE)
rows <- list()
add <- function(page, item, guide, riemann, quantlib = NA_real_) {
  rows[[length(rows) + 1L]] <<- tibble::tibble(
    page = page, item = item, guide = guide, quantlib_local = quantlib, riemann = riemann,
    riemann_minus_guide = riemann - guide, riemann_minus_quantlib = riemann - quantlib
  )
}
ql_date <- function(x) QuantLib::Date(as.integer(as.Date(x)) + 25569L)
ql_bond <- function(eval, effective, termination, face, coupon, first = NULL, issue = NULL) {
  invisible(QuantLib::Settings_instance()$setEvaluationDate(ql_date(eval)))
  schedule <- if (is.null(first)) {
    QuantLib::Schedule(ql_date(effective), ql_date(termination), QuantLib::Period(6L, "Months"), QuantLib::TARGET(),
                       "Following", "Following", "Backward", FALSE)
  } else {
    QuantLib::Schedule(ql_date(effective), ql_date(termination), QuantLib::Period(6L, "Months"), QuantLib::TARGET(),
                       "Following", "Following", "Backward", FALSE, ql_date(first))
  }
  QuantLib::FixedRateBond(3L, face, schedule, coupon, QuantLib::Thirty360("BondBasis"), "Following", 100,
                          if (is.null(issue)) QuantLib::Date() else ql_date(issue))
}
thirty360 <- "thirty_360_bond_basis"

# ---- "A taste of QuantLib": evaluation date 8 December 2022 -----------------------------------------
taste <- create_fixed_rate_bond(3L, 1e6, "2020-08-26", "2031-05-26", "6M", "target", "following", "following", "backward",
                                FALSE, 0.02, thirty360, "following", 100, first_date = "2021-05-26")
settle <- compute_settlement_date(taste, "2022-12-08")
taste_clean <- compute_clean_price(taste, 0.025, thirty360, "compounded", 1L, settle)
taste_accrued <- compute_accrued_amount(taste, settle)
q <- c(NA_real_, NA_real_)
if (has_quantlib) {
  b <- ql_bond("2022-12-08", "2020-08-26", "2031-05-26", 1e6, 0.02, first = "2021-05-26")
  s <- QuantLib::Bond_settlementDate(b)
  q <- c(QuantLib::BondFunctions_cleanPrice(b, 0.025, QuantLib::Thirty360("BondBasis"), "Compounded", "Annual", s),
         QuantLib::Bond_accruedAmount(b, s))
}
add("A taste of QuantLib", "Clean price at 2.5%, compounded annually (Guide prints 4 decimals)", 96.3256, taste_clean, q[1])
add("A taste of QuantLib", "Accrued amount per 100 (Guide prints 5 decimals)", 0.08333, taste_accrued, q[2])

# ---- "Cash flows and bonds": evaluation date 15 January 2024 ------------------------------------------
cfb <- create_fixed_rate_bond(3L, 10000, "2021-02-08", "2026-02-08", "6M", "target", "following", "following", "backward",
                              FALSE, 0.03, thirty360, "following", 100, issue_date = "2021-02-05")
cfb_flows <- get_bond_cashflows(cfb)
guide_amounts <- c(150.833, 149.167, 150.000, 150.000, 150.000, 150.000, 150.000, 151.667, 148.333, 150.833, 10000)
add("Cash flows and bonds", "Largest difference in the 11 cash-flow amounts (Guide prints 3 decimals)", 0,
    max(abs(cfb_flows$amount - guide_amounts)))
settle <- compute_settlement_date(cfb, "2024-01-15")
cfb_yield <- compute_yield_from_price(cfb, 98, "clean", thirty360, "compounded", 2L, settle)
q <- NA_real_
if (has_quantlib) {
  b <- ql_bond("2024-01-15", "2021-02-08", "2026-02-08", 10000, 0.03, issue = "2021-02-05")
  q <- QuantLib::BondFunctions_yield(b, QuantLib::BondPrice(98, "Clean"), QuantLib::Thirty360("BondBasis"), "Compounded", "Semiannual",
                                     QuantLib::Bond_settlementDate(b))
}
add("Cash flows and bonds", "Yield at a clean price of 98, compounded semiannually", 0.04021360410513353, cfb_yield, q)

# ---- "Cash-flow analysis": evaluation date 28 July 2025 -------------------------------------------------
cfa <- create_fixed_rate_bond(3L, 10000, "2025-04-08", "2030-04-08", "6M", "target", "following", "following", "backward",
                              FALSE, 0.03, thirty360, "following", 100)
cfa_amounts <- c(150.00, 150.00, 150.00, 150.00, 150.00, 151.67, 149.17, 150.00, 149.17, 150.00, 10000.00)
add("Cash-flow analysis", "Largest difference in the 11 cash-flow amounts (Guide prints 2 decimals)", 0,
    max(abs(get_bond_cashflows(cfa)$amount - cfa_amounts)))
cfa_value <- compute_cashflow_npv(get_bond_cashflows(cfa), create_flat_curve("2025-07-28", 0.04, "actual_360"),
                                  nominal = 10000, day_count = thirty360)
add("Cash-flow analysis", "CashFlows.npv on a flat 4% Actual/360 curve", 9625.543550654686, cfa_value$npv)
add("Cash-flow analysis", "CashFlows.bps on the same curve", 4.535150508988854, cfa_value$bps)
add("Cash-flow analysis", "NPV of the redemption (Guide prints 2 decimals)", 8265.00, tail(cfa_value$flows[[1]]$npv, 1))

# ---- "Vanilla bonds": evaluation date 2 May 2024 ---------------------------------------------------------
vanilla <- create_fixed_rate_bond(3L, 10000, "2023-02-08", "2028-02-08", "6M", "target", "following", "following", "backward",
                                  FALSE, 0.03, thirty360, "following", 100)
settle <- compute_settlement_date(vanilla, "2024-05-02")
later <- compute_settlement_date(vanilla, "2027-05-31")
vanilla_amounts <- c(150, 150, 150, 151.67, 148.33, 150.83, 150.83, 148.33, 150.83, 149.17, 10000)
add("Vanilla bonds", "Largest difference in the 11 cash-flow amounts (Guide prints 2 decimals)", 0,
    max(abs(get_bond_cashflows(vanilla)$amount - vanilla_amounts)))
add("Vanilla bonds", "Settlement date for 2 May 2024 (days since 1970-01-01)", as.numeric(as.Date("2024-05-07")), as.numeric(settle))
add("Vanilla bonds", "Settlement date for 31 May 2027 (days since 1970-01-01)", as.numeric(as.Date("2027-06-03")), as.numeric(later))
q <- rep(NA_real_, 6)
if (has_quantlib) {
  b <- ql_bond("2024-05-02", "2023-02-08", "2028-02-08", 10000, 0.03)
  s <- QuantLib::Bond_settlementDate(b)
  q <- c(QuantLib::Bond_accruedAmount(b, s), QuantLib::Bond_accruedAmount(b, ql_date("2027-05-09")),
         QuantLib::BondFunctions_cleanPrice(b, 0.035, QuantLib::Thirty360("BondBasis"), "Compounded", "Semiannual", s),
         QuantLib::BondFunctions_duration(b, 0.035, QuantLib::Actual360(), "Compounded", "Semiannual", "Macaulay", s),
         QuantLib::BondFunctions_duration(b, 0.035, QuantLib::Actual360(), "Compounded", "Semiannual", "Modified", s),
         QuantLib::BondFunctions_convexity(b, 0.035, QuantLib::Actual360(), "Compounded", "Semiannual", s))
}
add("Vanilla bonds", "Accrued amount per 100 at settlement", 0.7416666666666627, compute_accrued_amount(vanilla, settle), q[1])
add("Vanilla bonds", "Accrued amount per 100 for settlement on 9 May 2027", 0.7583333333333275, compute_accrued_amount(vanilla, "2027-05-09"), q[2])
add("Vanilla bonds", "Clean price at 3.5%, 30/360, compounded semiannually", 98.25259209893974,
    compute_clean_price(vanilla, 0.035, thirty360, "compounded", 2L, settle), q[3])
add("Vanilla bonds", "Macaulay duration at 3.5%, Actual/360, semiannual", 3.6046062651997737,
    compute_duration(vanilla, 0.035, "actual_360", "compounded", 2L, settle, duration_type = "macaulay"), q[4])
add("Vanilla bonds", "Modified duration at 3.5%, Actual/360, semiannual", 3.542610580048917,
    compute_duration(vanilla, 0.035, "actual_360", "compounded", 2L, settle, duration_type = "modified"), q[5])
add("Vanilla bonds", "Convexity at 3.5%, Actual/360, semiannual", 14.76085604114641,
    compute_convexity(vanilla, 0.035, "actual_360", "compounded", 2L, settle), q[6])

# ---- "Vanilla bonds" on a flat 4% Actual/360 curve, built on each evaluation date -------------------------
# QuantLib's FlatForward(0, TARGET, ...) moves its reference date with the evaluation date; here a curve is
# built on each date instead.
on_curve <- function(eval) compute_bond_value_on_curve(vanilla, create_flat_curve(eval, 0.04, "actual_360"), eval)
v0 <- on_curve("2024-05-02")
add("Vanilla bonds", "Clean price on the curve, 2 May 2024", 96.19296369916307, v0$clean_price)
add("Vanilla bonds", "Dirty price on the curve, 2 May 2024", 96.93463036582973, v0$dirty_price)
add("Vanilla bonds", "NPV on the curve, 2 May 2024", 9688.079274968233, v0$npv)
v1 <- on_curve("2024-08-07")
add("Vanilla bonds", "Dirty price on the curve, 7 August 2024", 96.48435372417977, v1$dirty_price)
add("Vanilla bonds", "NPV / 100 on the curve, 7 August 2024", 97.93059953484016, v1$npv / 100)
add("Vanilla bonds", "Clean price on the curve, 1 February 2028", 99.9882359483094, on_curve("2028-02-01")$clean_price)
v3 <- on_curve("2028-02-04")
add("Vanilla bonds", "Clean price, 4 February 2028 (settlement after maturity)", 0, v3$clean_price)
add("Vanilla bonds", "NPV, 4 February 2028", 10144.656928164271, v3$npv)

results <- do.call(rbind, rows)
cat("QuantLib Guide, fixed-rate bonds priced from a yield\n")
cat("QuantLib R package:", if (has_quantlib) as.character(utils::packageVersion("QuantLib")) else "not installed", "\n\n")
old <- options(width = 250)
print(as.data.frame(results), digits = 12, row.names = FALSE, right = FALSE)
options(old)
cat("\nThe Guide prints some values rounded; their differences show only that rounding.\n")
invisible(results)
