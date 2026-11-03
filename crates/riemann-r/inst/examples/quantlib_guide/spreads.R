# QuantLib Guide: the z-spread of a bond over a SOFR curve
#
# What it computes: the 20-year Treasury-like bond of "Spread calculations"
# (https://www.quantlibguide.com/), its z-spread over a SOFR curve bootstrapped from 15 OIS quotes, and the
# curve's discount factors. RiemannFinMath's Rust functions do the calculation. The table compares them with
# the Guide's printed values and, when the QuantLib R package is installed, with the same calls to the
# local QuantLib.
#
# Run:   source(system.file("examples", "quantlib_guide", "spreads.R", package = "RiemannFinMath"))
# Needs: RiemannFinMath and tibble. QuantLib (the R package) is optional.
#
# Conditions: evaluation date 16 March 2026. Bond: 26 May 2020 to 26 May 2040, annual, US government bond
# calendar, Following, backward, 3 settlement days, coupon 2%, 30/360 (Bond Basis), clean price 98.08.
# Curve: SOFR OIS, 2 settlement days, annual payments, Actual/360, monotonic log-cubic discounts. Spread:
# compounded annually, 30/360.
#
# Not reproduced (docs/quantlib_guide_inventory.md): the yield with SimpleThenCompounded compounding and the
# i-spread built on it; InterestRate.equivalentRate; the OAS of the callable version on a Hull-White tree.

library(RiemannFinMath)
has_quantlib <- requireNamespace("QuantLib", quietly = TRUE)
today <- "2026-03-16"
tenors <- c("1Y", "2Y", "3Y", "5Y", "8Y", "10Y", "12Y", "15Y", "20Y", "25Y", "30Y", "35Y", "40Y", "45Y", "50Y")
quotes <- c(0.137, 0.409, 0.674, 1.004, 1.258, 1.359, 1.420, 1.509, 1.574, 1.586, 1.579, 1.559, 1.514, 1.446, 1.425) / 100

bond <- create_fixed_rate_bond(3L, 1e6, "2020-05-26", "2040-05-26", "1Y", "us_government_bond", "following", "following",
                               "backward", FALSE, 0.02, "thirty_360_bond_basis", "following", 100)
curve <- bootstrap_ois_curve(today, today, "sofr", "actual_360", create_ois_instruments(tenors, quotes, 2L, 1L, 0L))
settlement <- compute_settlement_date(bond, today)
z <- compute_z_spread_from_price(bond, curve, 98.08, "clean", "compounded", 1L, settlement)

q_z <- NA_real_
q_discounts <- rep(NA_real_, 3)
check_dates <- as.Date(c("2031-03-18", "2040-05-29", "2056-03-17"))
if (has_quantlib) {
  ql_date <- function(x) QuantLib::Date(as.integer(as.Date(x)) + 25569L)
  invisible(QuantLib::Settings_instance()$setEvaluationDate(ql_date(today)))
  calendar <- QuantLib::UnitedStates("GovernmentBond")
  helpers <- QuantLib::RateHelperVector()
  index <- QuantLib::Sofr()
  for (i in seq_along(tenors)) {
    QuantLib::RateHelperVector_append(helpers, QuantLib::OISRateHelper(2L, QuantLib::Period(as.integer(sub("Y", "", tenors[i])), "Years"), QuantLib::QuoteHandle(QuantLib::SimpleQuote(quotes[i])),
                                                                        index, QuantLib::YieldTermStructureHandle(), FALSE, 0L, "Following", "Annual"))
  }
  ql_curve <- QuantLib::PiecewiseLogCubicDiscount(0L, calendar, helpers, QuantLib::Actual360())
  schedule <- QuantLib::Schedule(ql_date("2020-05-26"), ql_date("2040-05-26"), QuantLib::Period(1L, "Years"), calendar, "Following", "Following", "Backward", FALSE)
  ql_bond <- QuantLib::FixedRateBond(3L, 1e6, schedule, 0.02, QuantLib::Thirty360("BondBasis"))
  q_z <- QuantLib::BondFunctions_zSpread(ql_bond, QuantLib::BondPrice(98.08, "Clean"), ql_curve, QuantLib::Thirty360("BondBasis"), "Compounded", "Annual",
                                         QuantLib::Bond_settlementDate(ql_bond))
  q_discounts <- vapply(check_dates, \(d) QuantLib::YieldTermStructure_discount(ql_curve, ql_date(d)), 1)
}

results <- rbind(
  tibble::tibble(item = "Z-spread, bp (compounded annually, 30/360)", guide = 64.75664508570601, quantlib_local = q_z * 1e4, riemann = z * 1e4),
  tibble::tibble(item = paste("Discount factor on", check_dates), guide = NA_real_, quantlib_local = q_discounts,
                 riemann = compute_discount(curve, check_dates))
)
results$riemann_minus_guide <- results$riemann - results$guide
results$riemann_minus_quantlib <- results$riemann - results$quantlib_local
cat("QuantLib Guide, spread calculations\n")
cat("QuantLib R package:", if (has_quantlib) as.character(utils::packageVersion("QuantLib")) else "not installed", "\n\n")
old <- options(width = 250)
print(as.data.frame(results), digits = 12, row.names = FALSE, right = FALSE)
options(old)
cat("\nCurve repricing of the 15 OIS quotes, largest absolute error:", format(max(abs(get_quote_repricing(curve)$reproduction_error)), digits = 3), "\n")
invisible(results)
