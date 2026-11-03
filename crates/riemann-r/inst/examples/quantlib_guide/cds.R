# QuantLib Guide: a hazard curve bootstrapped from CDS running spreads
#
# What it computes: the "old conventions" curve of "Default-probability curves"
# (https://www.quantlibguide.com/): a piecewise flat hazard curve fitted to six CDS running spreads, its
# nodes, hazard rates and default probabilities. RiemannFinMath's Rust functions do the calculation. The
# table compares them with the Guide's printed values and, when the QuantLib R package is installed, with
# the same calls to the local QuantLib.
#
# Run:   source(system.file("examples", "quantlib_guide", "cds.R", package = "RiemannFinMath"))
# Needs: RiemannFinMath and tibble. QuantLib (the R package) is optional.
#
# Conditions: evaluation date 22 September 2025. Spreads 70, 80, 100, 140, 170 and 210 bp for 1, 2, 3, 5,
# 7 and 10 years; 1 settlement day; US government bond calendar; quarterly premiums; Modified Following;
# date rule CDS; Actual/360 premiums and hazard curve; recovery 40%; risk-free curve flat 3% continuous,
# Actual/365 (Fixed); mid-point engine.
#
# Not reproduced (docs/quantlib_guide_inventory.md): the curves from upfront quotes (UpfrontCdsHelper) and
# the check that reprices one of them, the survival-probability curve given by nodes, the bond priced with
# default risk ("Bonds and CDS curves").

library(RiemannFinMath)
has_quantlib <- requireNamespace("QuantLib", quietly = TRUE)
today <- "2025-09-22"
tenors <- c("1Y", "2Y", "3Y", "5Y", "7Y", "10Y")
spreads <- c(70, 80, 100, 140, 170, 210) * 1e-4
rates <- create_flat_curve(today, 0.03, "actual_365_fixed")
maturities <- do.call(c, lapply(tenors, \(t) compute_cds_maturity(today, t, "cds")))
curve <- bootstrap_hazard_curve(today, maturities, spreads, 0.4, rates, "midpoint", settlement_days = 1L, calendar = "us_government_bond",
                                tenor = "3M", payment_convention = "modified_following", rule = "cds", cds_day_count = "actual_360",
                                day_count = "actual_360")
nodes <- curve$nodes
riemann_default <- 1 - compute_survival_probability(curve, nodes$date)

guide <- tibble::tibble(
  date = as.Date(c("2026-12-21", "2027-12-20", "2028-12-20", "2030-12-20", "2032-12-20", "2035-12-20")),
  hazard_pct = c(1.16, 1.55, 2.46, 3.57, 4.47, 5.84),
  default_pct = c(1.45, 2.98, 5.38, 11.99, 19.63, 32.71)
)
quantlib_hazard <- quantlib_default <- rep(NA_real_, 6)
if (has_quantlib) {
  ql_date <- function(x) QuantLib::Date(as.integer(as.Date(x)) + 25569L)
  invisible(QuantLib::Settings_instance()$setEvaluationDate(ql_date(today)))
  calendar <- QuantLib::UnitedStates("GovernmentBond")
  risk_free <- QuantLib::YieldTermStructureHandle(QuantLib::FlatForward(0L, calendar, 0.03, QuantLib::Actual365Fixed()))
  helpers <- QuantLib::DefaultProbabilityHelperVector()
  for (k in seq_along(tenors)) {
    helpers$append(QuantLib::SpreadCdsHelper(spreads[k], QuantLib::Period(as.integer(sub("Y", "", tenors[k])), "Years"), 1L, calendar,
                                             "Quarterly", "ModifiedFollowing", "CDS", QuantLib::Actual360(), 0.4, risk_free))
  }
  ql_curve <- QuantLib::PiecewiseFlatHazardRate(0L, calendar, helpers, QuantLib::Actual360())
  quantlib_hazard <- vapply(guide$date, \(d) ql_curve$hazardRate(ql_date(d)), 1)
  quantlib_default <- vapply(guide$date, \(d) ql_curve$defaultProbability(ql_date(d)), 1)
}

results <- tibble::tibble(
  node = nodes$date,
  guide_node = guide$date,
  guide_hazard_pct = guide$hazard_pct,
  quantlib_hazard_pct = 100 * quantlib_hazard,
  riemann_hazard_pct = 100 * nodes$hazard,
  guide_default_pct = guide$default_pct,
  quantlib_default_pct = 100 * quantlib_default,
  riemann_default_pct = 100 * riemann_default
)
cat("QuantLib Guide, default-probability curve from CDS running spreads (old conventions)\n")
cat("QuantLib R package:", if (has_quantlib) as.character(utils::packageVersion("QuantLib")) else "not installed", "\n\n")
old <- options(width = 250)
print(as.data.frame(results), digits = 8, row.names = FALSE)
options(old)
cat("\nThe Guide prints 2 decimals. Its hazard rates are the rounded values of these; its default probabilities are",
    "\nabout 0.007 points lower than both QuantLib 1.42 and RiemannFinMath, which agree with each other. The",
    "\ndifference is between the Guide's environment and QuantLib 1.42, and was not traced further.\n")
invisible(results)
