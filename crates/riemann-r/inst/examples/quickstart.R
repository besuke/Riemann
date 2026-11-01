# The Riemann library from the Positron R console, through the R package RiemannFinMath. Run it line by line, or with:
#   source(system.file("examples", "quickstart.R", package = "RiemannFinMath"))
# All values are synthetic, and the conventions are examples, not market conventions.
# Every convention is an explicit argument, and there is no global evaluation date.

library(tibble)

# ---- a fixed-rate bond ------------------------------------------------------------------------

bond <- RiemannFinMath::fixed_rate_bond(
  settlement_days = 1L, face_amount = 100,
  effective_date = "2020-03-20", termination_date = as.Date("2030-03-20"), # ISO strings or Dates
  tenor = "6M", calendar = "japan",
  convention = "following", termination_convention = "following", rule = "backward", end_of_month = FALSE,
  coupon_rate = 0.015, day_count = "act_act_isma", payment_convention = "following", redemption = 100,
  issue_date = "2020-03-20"
)
print(bond)

settlement <- RiemannFinMath::settlement_date(bond, "2026-11-02")
cat("Settlement date:", format(settlement), "\n")

# Prices and risk at several yields, one row each.
print(RiemannFinMath::bond_values(bond, rate = c(0.01, 0.02, 0.03), day_count = "act_act_isma", compounding = "compounded",
                           frequency = 2L, settlement_date = settlement))

# The yield of a clean price.
y <- RiemannFinMath::yield_from_price(bond, price = 98.5, price_type = "clean", day_count = "act_act_isma",
                               compounding = "compounded", frequency = 2L, settlement_date = settlement)
cat("Yield of a clean price of 98.5:", y, "\n")

# ---- an OIS curve; index is one of tonar, estr, sofr, sonia --------------------------------------

instruments <- RiemannFinMath::ois_instruments(
  tenor = c("ON", "1M", "3M", "6M", "1Y", "2Y", "3Y", "5Y", "7Y", "10Y", "20Y", "30Y"),
  rate = c(0.002, 0.0025, 0.003, 0.0035, 0.004, 0.005, 0.0065, 0.009, 0.011, 0.014, 0.018, 0.019),
  settlement_days = 2L, payment_frequency = 1L, payment_lag = 2L
)
curve <- RiemannFinMath::build_ois_curve(
  evaluation_date = "2026-11-02", reference_date = "2026-11-02", index = "tonar",
  curve_day_count = "actual_365_fixed", instruments = instruments
)
print(curve)

# How the curve reprices its quotes. Check this: quotes that swing widely can be missed.
print(RiemannFinMath::quote_repricing(curve))

dates <- as.Date("2026-11-02") + 365L * c(1L, 2L, 5L, 10L, 20L)
print(RiemannFinMath::curve_values(curve, dates, day_count = "actual_365_fixed"))

# The same bond on the curve.
print(RiemannFinMath::value_on_curve(bond, curve, "2026-11-02"))

# ---- calendars, for bond settlement and payment dates ------------------------------------------

print(RiemannFinMath::holidays("brazil_settlement", "2026-01-01", "2026-12-31"))
print(RiemannFinMath::is_business_day("turkey", c("2026-05-19", "2026-05-20")))

# ---- many evaluations in parallel; the results do not depend on the number of threads ----------

curves <- tibble(
  evaluation_date = as.Date("2026-11-02"), reference_date = as.Date("2026-11-02"),
  index = c("tonar", "estr", "sofr", "sonia"),
  curve_day_count = "actual_365_fixed",
  instruments = list(instruments, instruments, instruments, instruments),
  dates = list(dates, dates, dates, dates)
)
print(RiemannFinMath::curve_values_batch(curves, threads = 4L))
