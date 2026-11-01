# Golden values for Riemann P7: OIS asset swaps and the yen-converted asset swap from GaussQuant
# (ois_asset_swap_GQL, ois_asset_swap_jpy_basis_GQL), and futures basis from QuantLib's bond values.
# All quotes and prices are synthetic random numbers; no market data is used.
# Usage (from the Riemann root): Rscript tools/golden/p7_asset_swaps.R
# Writes tests/golden/asw_{nodes,bonds,jpy,futures}.csv. The curves are passed to Riemann as their
# nodes, so this compares the asset swap, not the bootstrap.

suppressMessages({library(QuantLib); library(GaussQuant)})

out_dir <- file.path("tests", "golden")
set.seed(20261102)
fmt <- function(x) sprintf("%.17g", x)
market_date <- "2026-10-06"
tenors <- c("1Y", "2Y", "3Y", "5Y", "7Y", "10Y", "15Y", "20Y", "30Y")
levels <- c(JPY = 0.012, USD = 0.038, EUR = 0.024, GBP = 0.04)
curves <- lapply(names(levels), function(currency) {
  quotes <- tibble::tibble(kind = "ois", tenor = tenors, rate = levels[[currency]] + cumsum(rnorm(length(tenors), 0.0005, 0.001)))
  ir_build_ois_curve_GQH(curve_data = quotes, trade_date = market_date, currency = currency, verbose = FALSE,
                         curve_type = "log_cubic_discount", reference = "evaluation", payment_frequency = "Annual",
                         payment_lag = 0L, payment_convention = "ModifiedFollowing", max_quote_error_bp = 1e-4)
})
names(curves) <- names(levels)

header <- function(path, columns) {
  writeLines(c(paste0("# GaussQuant ", packageVersion("GaussQuant"), " on QuantLib R package ", packageVersion("QuantLib"), "; synthetic inputs; tools/golden/p7_asset_swaps.R"),
               paste(columns, collapse = ",")), path)
}
add <- function(path, values) write(paste(values, collapse = ","), path, append = TRUE)
serial <- function(d) as.integer(as.Date(d)) + 25569L

path <- file.path(out_dir, "asw_nodes.csv")
header(path, c("currency", "date", "discount"))
for (currency in names(curves)) {
  curve <- curves[[currency]]$curve
  dates <- curve$dates()
  for (i in seq_len(dates$size())) {
    d <- QuantLib::DateVector___getitem__(dates, i - 1L)
    add(path, c(currency, QuantLib::Date_serialNumber(d), fmt(curve$discount(d))))
  }
}

calendars <- list(JPY = QuantLib::Japan(), USD = QuantLib::UnitedStates("SOFR"), EUR = QuantLib::TARGET(), GBP = QuantLib::UnitedKingdom("Exchange"))
day_counters <- list(JPY = QuantLib::Actual365Fixed(), USD = QuantLib::Thirty360("BondBasis"), EUR = QuantLib::Thirty360("BondBasis"), GBP = QuantLib::Thirty360("BondBasis"))
bond_of <- function(currency, coupon, years) {
  fixed_rate_bond_GQL(
    issue_date = "2026-09-30", maturity_date = sprintf("%d-09-30", 2026L + years), coupon_rate = coupon, face_amount = 100, redemption = 100,
    settlement_days = 2L, frequency = QuantLib::Period("Semiannual"), calendar = calendars[[currency]], accrual_day_counter = day_counters[[currency]],
    payment_convention = "Following", schedule_convention = "Unadjusted", maturity_convention = "Unadjusted"
  )
}

path <- file.path(out_dir, "asw_bonds.csv")
header(path, c("currency", "coupon", "years", "clean_price", "frequency", "lag", "spread_bp", "accrued", "fair_spread_bp", "annuity", "npv", "bond_leg_pv", "overnight_leg_pv", "principal_pv", "upfront_pv", "fair_price", "effective", "maturity"))
for (currency in names(curves)) {
  for (i in 1:20) {
    coupon <- round(runif(1, 0.005, 0.06), 4)
    years <- sample(c(3L, 5L, 7L, 10L, 15L, 20L), 1)
    clean <- runif(1, 80, 120)
    frequency <- sample(c("Annual", "Semiannual"), 1)
    lag <- sample(c(0L, 2L), 1)
    spread <- sample(c(0, 25), 1)
    set_eval_date_GQL(market_date)
    a <- ois_asset_swap_GQL(bond_of(currency, coupon, years), clean, curves[[currency]], spread_bp = spread, payment_frequency = frequency, payment_lag = lag)
    v <- a$valuation
    add(path, c(currency, fmt(coupon), years, fmt(clean), if (frequency == "Annual") 1L else 2L, lag, fmt(spread),
                fmt(c(v$accrued_per_100, v$fair_spread_bp, v$spread_annuity, v$npv, v$bond_leg_pv, v$overnight_leg_pv, v$principal_pv, v$upfront_pv, ois_asset_swap_fair_price_GQL(a))),
                serial(a$effective_date), serial(a$maturity_date)))
  }
}

path <- file.path(out_dir, "asw_jpy.csv")
header(path, c("currency", "coupon", "years", "clean_price", "fx_spot", "basis_bp", "spread_bp", "native_asw_bp", "jpy_asw_bp", "simple_sum_bp", "npv_jpy", "hedged_bond_pv_jpy", "jpy_base_leg_pv", "upfront_pv_jpy", "annuity_jpy", "z_bp"))
for (currency in c("USD", "EUR", "GBP")) {
  for (i in 1:15) {
    coupon <- round(runif(1, 0.01, 0.06), 4)
    years <- sample(c(3L, 5L, 7L, 10L), 1)
    clean <- runif(1, 85, 115)
    fx <- runif(1, 120, 200)
    basis <- runif(1, -60, 10)
    spread <- sample(c(0, 15), 1)
    set_eval_date_GQL(market_date)
    v <- ois_asset_swap_jpy_basis_GQL(bond_of(currency, coupon, years), clean, curves[[currency]], curves[["JPY"]], fx, basis, currency = currency, spread_bp = spread)$valuation
    add(path, c(currency, fmt(coupon), years, fmt(clean), fmt(fx), fmt(basis), fmt(spread),
                fmt(c(v$native_asw_bp, v$jpy_asw_bp, v$simple_sum_bp, v$npv_jpy, v$hedged_bond_pv_jpy, v$jpy_base_leg_pv, v$upfront_pv_jpy, v$spread_annuity_jpy, v$calibrated_discount_spread_bp))))
  }
}

# Futures basis: the arithmetic of LagrangeFinance II-2.6.3, with QuantLib's accrued amounts
path <- file.path(out_dir, "asw_futures.csv")
header(path, c("coupon", "years", "clean_price", "futures_price", "conversion_factor", "funding_rate", "valuation", "delivery", "accrued_start", "accrued_end", "coupon_income", "gross_basis", "carry", "net_basis", "implied_repo"))
valuation <- "2026-09-10"
delivery <- "2026-12-20"
for (i in 1:60) {
  coupon <- round(runif(1, 0.005, 0.03), 4)
  years <- sample(7:10, 1)
  clean <- runif(1, 80, 105)
  futures <- runif(1, 110, 140)
  cf <- runif(1, 0.65, 0.8)
  funding <- runif(1, 0, 0.05)
  set_eval_date_GQL(valuation)
  bond <- fixed_rate_bond_GQL(
    issue_date = "2026-06-20", maturity_date = sprintf("%d-06-20", 2026L + years), coupon_rate = coupon, face_amount = 100, settlement_days = 0L,
    frequency = period_months_GQL(6), calendar = QuantLib::Japan(), accrual_day_counter = QuantLib::Actual365Fixed(),
    payment_convention = "Following", schedule_convention = "Unadjusted", maturity_convention = "Unadjusted"
  )
  a0 <- bond$accruedAmount(date_GQL(valuation))
  a1 <- bond$accruedAmount(date_GQL(delivery))
  flows <- bond_cashflow_table_GQL(bond)
  income <- sum(flows$amount[as.Date(flows$date) > as.Date(valuation) & as.Date(flows$date) <= as.Date(delivery)])
  year_fraction <- as.numeric(as.Date(delivery) - as.Date(valuation)) / 365
  dirty <- clean + a0
  invoice <- futures * cf
  gross <- clean - invoice
  carry <- income + a1 - a0 - dirty * funding * year_fraction
  add(path, c(fmt(c(coupon)), years, fmt(c(clean, futures, cf, funding)), serial(valuation), serial(delivery),
              fmt(c(a0, a1, income, gross, carry, gross - carry, (invoice + a1 + income - dirty) / dirty / year_fraction))))
}
