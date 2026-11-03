# The Riemann library from the Positron R console, through the R package RiemannFinMath. Run it line by line, or with:
#   source(system.file("examples", "quickstart.R", package = "RiemannFinMath"))
# All values are synthetic, and the conventions are examples, not market conventions.
# Every convention is an explicit argument, and there is no global evaluation date.

library(tibble)

# ---- a fixed-rate bond ------------------------------------------------------------------------

bond <- RiemannFinMath::create_fixed_rate_bond(
  settlement_days = 1L, face_amount = 100,
  effective_date = "2020-03-20", termination_date = as.Date("2030-03-20"), # ISO strings or Dates
  tenor = "6M", calendar = "japan",
  convention = "following", termination_convention = "following", rule = "backward", end_of_month = FALSE,
  coupon_rate = 0.015, day_count = "act_act_isma", payment_convention = "following", redemption = 100,
  issue_date = "2020-03-20"
)
print(bond)

settlement <- RiemannFinMath::compute_settlement_date(bond, "2026-11-02")
cat("Settlement date:", format(settlement), "\n")

# Prices and risk at several yields, one row each.
print(RiemannFinMath::compute_bond_values(bond, rate = c(0.01, 0.02, 0.03), day_count = "act_act_isma", compounding = "compounded",
                           frequency = 2L, settlement_date = settlement))

# The yield of a clean price.
y <- RiemannFinMath::compute_yield_from_price(bond, price = 98.5, price_type = "clean", day_count = "act_act_isma",
                               compounding = "compounded", frequency = 2L, settlement_date = settlement)
cat("Yield of a clean price of 98.5:", y, "\n")

# ---- an OIS curve; index is one of tonar, estr, sofr, sonia --------------------------------------

instruments <- RiemannFinMath::create_ois_instruments(
  tenor = c("ON", "1M", "3M", "6M", "1Y", "2Y", "3Y", "5Y", "7Y", "10Y", "20Y", "30Y"),
  rate = c(0.002, 0.0025, 0.003, 0.0035, 0.004, 0.005, 0.0065, 0.009, 0.011, 0.014, 0.018, 0.019),
  settlement_days = 2L, payment_frequency = 1L, payment_lag = 2L
)
curve <- RiemannFinMath::bootstrap_ois_curve(
  evaluation_date = "2026-11-02", reference_date = "2026-11-02", index = "tonar",
  curve_day_count = "actual_365_fixed", instruments = instruments
)
print(curve)

# How the curve reprices its quotes. Check this: quotes that swing widely can be missed.
print(RiemannFinMath::get_quote_repricing(curve))

dates <- as.Date("2026-11-02") + 365L * c(1L, 2L, 5L, 10L, 20L)
print(RiemannFinMath::compute_curve_values(curve, dates, day_count = "actual_365_fixed"))

# The same bond on the curve.
print(RiemannFinMath::compute_bond_value_on_curve(bond, curve, "2026-11-02"))

# Its cash flows.
print(RiemannFinMath::get_bond_cashflows(bond))

# With 50 bp on the curve's zero rates, and the spread that reprices a clean price.
print(RiemannFinMath::compute_bond_value_with_zero_spread(bond, curve, 0.005, "compounded", 2L, "2026-11-02"))
z <- RiemannFinMath::compute_z_spread_from_price(bond, curve, price = 98.5, price_type = "clean", compounding = "compounded",
                                         frequency = 2L, settlement_date = settlement)
cat("Zero spread of a clean price of 98.5:", z, "\n")

# ---- an overnight indexed swap on the curve -----------------------------------------------------

swap <- RiemannFinMath::create_ois_swap("payer", nominal = 1e7, effective_date = "2026-11-04", termination_date = "2031-11-04",
                                 index = "tonar", fixed_rate = 0.012)
print(swap)
print(RiemannFinMath::compute_ois_swap_value(swap, curve, "2026-11-02"))
print(RiemannFinMath::compute_ois_swap_cashflows(swap, curve, "2026-11-02"))

# After the start, the past fixings are arguments: there is no global store.
fixings <- tibble(date = as.Date(c("2026-11-04", "2026-11-05")), rate = c(0.002, 0.0021))
print(RiemannFinMath::compute_ois_swap_value(swap, curve, "2026-11-06", fixings))

# ---- calendars, for bond settlement and payment dates ------------------------------------------

print(RiemannFinMath::generate_holidays("brazil_settlement", "2026-01-01", "2026-12-31"))
print(RiemannFinMath::is_business_day("turkey", c("2026-05-19", "2026-05-20")))

# ---- Hull-White: fit the mean reversion and the volatility to option prices --------------------

hw_curve <- RiemannFinMath::bootstrap_ois_curve("2026-11-02", "2026-11-02", "tonar", "actual_365_fixed", instruments)
hw_options <- tibble(option_type = "call", strike = c(0.98, 0.95, 0.9), maturity = c(0.5, 1, 2), bond_maturity = c(2, 3, 5))
hw_options$price <- RiemannFinMath::compute_hull_white_bond_option_value(
  hw_options$option_type, hw_options$strike, hw_options$maturity, hw_options$bond_maturity, 0.08, 0.012, hw_curve
)
hw_fit <- RiemannFinMath::calibrate_hull_white(hw_curve, bond_options = hw_options)
print(hw_fit[c("mean_reversion", "volatility", "stop_reason")])

# ---- many evaluations in parallel; the results do not depend on the number of threads ----------

curves <- tibble(
  evaluation_date = as.Date("2026-11-02"), reference_date = as.Date("2026-11-02"),
  index = c("tonar", "estr", "sofr", "sonia"),
  curve_day_count = "actual_365_fixed",
  instruments = list(instruments, instruments, instruments, instruments),
  dates = list(dates, dates, dates, dates)
)
print(RiemannFinMath::compute_curve_values_batch(curves, threads = 4L))
