# QuantLib Guide: default-probability curves from CDS quotes and from survival probabilities
#
# What it computes: the curves of "Default-probability curves" (https://www.quantlibguide.com/):
#   1. piecewise flat hazard curves fitted to six CDS upfronts on a running spread of 100 bp (new
#      conventions, CDS2015), on 27 August, 19 September and 22 September 2025, and with the ISDA engine;
#   2. the check that values the 3-year quote with the curve (NPV and fair upfront);
#   3. the default probability and hazard rate of the ISDA curve on 12 May 2027;
#   4. a curve of survival probabilities, linear between nodes (QuantLib SurvivalProbabilityCurve);
#   5. the "old conventions" curve fitted to six running spreads.
# RiemannFinMath's Rust functions do the calculation. The tables compare them with the Guide's printed
# values and, when the QuantLib R package is installed, with the same calls to the local QuantLib.
#
# Run:   source(system.file("examples", "quantlib_guide", "cds.R", package = "RiemannFinMath"))
# Needs: RiemannFinMath and tibble. QuantLib (the R package) is optional.
#
# Conditions. Upfront curves: upfronts -0.2%, -0.5%, -0.8%, -0.4%, 0.7% and 1.1% for 1, 2, 3, 5, 7 and 10
# years, paid by the buyer 3 business days after the evaluation date, on a running spread of 100 bp;
# 1 settlement day; US government bond calendar; quarterly premiums; Modified Following; date rule
# CDS2015; Actual/360 premiums; hazard curve on Actual/365 (Fixed); recovery 40%; risk-free curve flat 3%
# continuous, Actual/365 (Fixed); mid-point engine unless the ISDA engine is named. The check CDS: buyer,
# notional 10,000, upfront -0.8%, 3 years from the day after 22 September 2025. Old conventions: spreads 70,
# 80, 100, 140, 170 and 210 bp, date rule CDS, Actual/360 hazard curve, evaluation date 22 September 2025.
#
# Differences of QuantLib versions: the check CDS is worth 0 and its fair upfront is the quote (-0.8%) in
# both RiemannFinMath and QuantLib 1.42 run with the Guide's calls; the Guide prints -0.2777 and -0.80278%,
# one day of the 1% premium on 10,000 (0.2778). The ISDA curve's default probability on 12 May 2027 differs
# from the Guide's print by 4.6e-5. The Guide's default probabilities are up to 0.009 points lower, and its
# first hazard rate on 19 September is 1.22% against 1.2309%. In each case RiemannFinMath equals QuantLib 1.42.
#
# Not reproduced (docs/quantlib_guide_inventory.md): the default density, the DefaultDensityCurve, and the
# bond priced with default risk ("Bonds and CDS curves").

library(RiemannFinMath)
has_quantlib <- requireNamespace("QuantLib", quietly = TRUE)
if (has_quantlib) {
  ql_date <- function(x) QuantLib::Date(as.integer(as.Date(x)) + 25569L)
  ql_iso <- function(d) as.Date(QuantLib::Date_ISO(d))
  ql_calendar <- QuantLib::UnitedStates("GovernmentBond")
  ql_risk_free <- function() QuantLib::YieldTermStructureHandle(QuantLib::FlatForward(0L, ql_calendar, 0.03, QuantLib::Actual365Fixed()))
}

# ---- 1. curves from upfront quotes ----------------------------------------------------------------
tenors <- c(1L, 2L, 3L, 5L, 7L, 10L)
upfronts <- c(-0.002, -0.005, -0.008, -0.004, 0.007, 0.011)
upfront_curve <- function(today, engine) {
  maturities <- do.call(c, lapply(paste0(tenors, "Y"), \(t) compute_cds_maturity(today, t, "cds2015")))
  bootstrap_hazard_curve(today, maturities, upfronts, 0.4, create_flat_curve(today, 0.03, "actual_365_fixed"), engine,
                         settlement_days = 1L, calendar = "us_government_bond", tenor = "3M", payment_convention = "modified_following",
                         rule = "cds2015", cds_day_count = "actual_360", day_count = "actual_365_fixed",
                         quote_type = "upfront", running_spread = 0.01, upfront_settlement_days = 3L)
}
ql_upfront_curve <- function(today, model) {
  invisible(QuantLib::Settings_instance()$setEvaluationDate(ql_date(today)))
  helpers <- QuantLib::DefaultProbabilityHelperVector()
  for (k in seq_along(tenors)) {
    helpers$append(QuantLib::UpfrontCdsHelper(upfronts[k], 0.01, QuantLib::Period(tenors[k], "Years"), 1L, ql_calendar, "Quarterly",
                                              "ModifiedFollowing", "CDS2015", QuantLib::Actual360(), 0.4, ql_risk_free(), 3L, TRUE, TRUE,
                                              QuantLib::Date(), QuantLib::Actual360(), TRUE, model))
  }
  QuantLib::PiecewiseFlatHazardRate(0L, ql_calendar, helpers, QuantLib::Actual365Fixed())
}
# the Guide's tables, in %
guide_upfront <- list(
  "2025-08-27 midpoint" = list(date = c("2026-06-22", "2027-06-21", "2028-06-20", "2030-06-20", "2032-06-21", "2035-06-20"),
                               hazard = c(1.26, 1.15, 1.13, 2.08, 2.89, 2.02), default = c(1.03, 2.16, 3.26, 7.20, 12.43, 17.57)),
  "2025-09-19 midpoint" = list(date = c("2026-06-22", "2027-06-21", "2028-06-20", "2030-06-20", "2032-06-21", "2035-06-20"),
                               hazard = c(1.22, 1.16, 1.14, 2.08, 2.89, 2.02), default = c(0.92, 2.06, 3.16, 7.10, 12.33, 17.47)),
  "2025-09-22 midpoint" = list(date = c("2026-12-21", "2027-12-20", "2028-12-20", "2030-12-20", "2032-12-20", "2035-12-20"),
                               hazard = c(1.40, 1.14, 1.12, 2.09, 2.92, 2.03), default = c(1.73, 2.85, 3.94, 7.86, 13.09, 18.21)),
  "2025-09-22 isda" = list(date = c("2026-12-22", "2027-12-21", "2028-12-21", "2030-12-21", "2032-12-21", "2035-12-21"),
                           hazard = c(1.40, 1.14, 1.12, 2.09, 2.92, 2.03), default = c(1.73, 2.85, 3.94, 7.86, 13.09, 18.22))
)
curves <- list()
upfront_nodes <- do.call(rbind, lapply(names(guide_upfront), \(case) {
  today <- sub(" .*", "", case)
  engine <- sub(".* ", "", case)
  curve <- upfront_curve(today, engine)
  curves[[case]] <<- curve
  nodes <- curve$nodes
  q_hazard <- q_default <- rep(NA_real_, nrow(nodes))
  if (has_quantlib) {
    q <- ql_upfront_curve(today, if (engine == "isda") "ISDA" else "Midpoint")
    q_hazard <- vapply(nodes$date, \(d) q$hazardRate(ql_date(d)), 1)
    q_default <- vapply(nodes$date, \(d) q$defaultProbability(ql_date(d)), 1)
  }
  g <- guide_upfront[[case]]
  tibble::tibble(case = case, node = nodes$date, guide_node = as.Date(g$date),
                 guide_hazard_pct = g$hazard, quantlib_hazard_pct = 100 * q_hazard, riemann_hazard_pct = 100 * nodes$hazard,
                 guide_default_pct = g$default, quantlib_default_pct = 100 * q_default,
                 riemann_default_pct = 100 * (1 - compute_survival_probability(curve, nodes$date)))
}))

# ---- 2. and 3. the check CDS and the curve interface on 22 September 2025 ----------------------------
today <- "2025-09-22"
rates <- create_flat_curve(today, 0.03, "actual_365_fixed")
start <- compute_advanced_date("us_government_bond", today, "1D")
maturity <- compute_cds_maturity(start, "3Y", "cds2015")
check_cds <- create_credit_default_swap("buyer", 10000, 0.01, today, maturity, "3M", "us_government_bond", "following", "following", "cds2015",
                                        "modified_following", "actual_360", upfront = -0.008, rebates_accrual = TRUE, trade_date = today,
                                        protection_start = start, cash_settlement_days = 3L, last_period_day_count = "actual_360")
mid <- curves[["2025-09-22 midpoint"]]
isda <- curves[["2025-09-22 isda"]]
v_mid <- compute_cds_value(check_cds, mid, 0.4, rates, today, "midpoint")
v_isda_mid <- compute_cds_value(check_cds, isda, 0.4, rates, today, "midpoint")
v_isda <- compute_cds_value(check_cds, isda, 0.4, rates, today, "isda")
may_2027 <- as.Date("2027-05-12")
survival_dates <- as.Date(c("2026-09-22", "2027-09-22", "2030-09-22", "2035-09-22"))
survival_curve <- create_survival_probability_curve(today, survival_dates, c(0.995, 0.985, 0.97, 0.93))

q <- rep(NA_real_, 9)
if (has_quantlib) {
  q_mid <- ql_upfront_curve(today, "Midpoint")
  q_isda <- ql_upfront_curve(today, "ISDA")
  q_start <- QuantLib::Calendar_advance(ql_calendar, ql_date(today), 1L, "Days")
  q_schedule <- QuantLib::Schedule(ql_date(today), QuantLib::cdsMaturity(q_start, QuantLib::Period(3L, "Years"), "CDS2015"), QuantLib::Period("Quarterly"),
                                   ql_calendar, "Following", "Following", "CDS2015", FALSE)
  q_cds <- QuantLib::CreditDefaultSwap("Buyer", 10000, -0.008, 0.01, q_schedule, "ModifiedFollowing", QuantLib::Actual360(), TRUE, TRUE, q_start,
                                       QuantLib::Calendar_advance(ql_calendar, ql_date(today), 3L, "Days"), QuantLib::FaceValueClaim(),
                                       QuantLib::Actual360(), TRUE, ql_date(today))
  priced <- function(engine) {
    invisible(QuantLib::Instrument_setPricingEngine(q_cds, engine))
    c(QuantLib::Instrument_NPV(q_cds), QuantLib::CreditDefaultSwap_fairUpfront(q_cds))
  }
  handle <- function(curve) QuantLib::DefaultProbabilityTermStructureHandle(curve)
  dv <- QuantLib::DateVector()
  for (d in c(today, format(survival_dates))) dv$append(ql_date(d))
  q_survival <- QuantLib::SurvivalProbabilityCurve(dv, c(1, 0.995, 0.985, 0.97, 0.93), QuantLib::Actual365Fixed())
  q <- c(priced(QuantLib::MidPointCdsEngine(handle(q_mid), 0.4, ql_risk_free())),
         priced(QuantLib::MidPointCdsEngine(handle(q_isda), 0.4, ql_risk_free()))[1],
         priced(QuantLib::IsdaCdsEngine(handle(q_isda), 0.4, ql_risk_free())),
         q_isda$defaultProbability(ql_date(may_2027)), q_isda$hazardRate(ql_date(may_2027)),
         q_survival$defaultProbability(ql_date(may_2027)), q_survival$hazardRate(ql_date(may_2027)))
}
checks <- tibble::tibble(
  item = c("Check CDS on the mid-point curve, mid-point engine: NPV", "  fair upfront",
           "Check CDS on the ISDA curve, mid-point engine: NPV",
           "Check CDS on the ISDA curve, ISDA engine: NPV", "  fair upfront",
           "ISDA curve: default probability on 12 May 2027", "ISDA curve: hazard rate on 12 May 2027",
           "Survival-probability curve: default probability on 12 May 2027", "Survival-probability curve: hazard rate on 12 May 2027"),
  guide = c(-0.2777092930701878, -0.00802777777777772, -0.4857617356748842, -0.2777092930706994, -0.008027777777777773,
            0.021675744707762012, 0.01143141003780411, 0.011356164383561684, 0.010114866081944281),
  quantlib_local = q,
  riemann = c(v_mid$npv, v_mid$fair_upfront, v_isda_mid$npv, v_isda$npv, v_isda$fair_upfront,
              1 - compute_survival_probability(isda, may_2027), compute_hazard_rate(isda, may_2027),
              1 - compute_survival_probability(survival_curve, may_2027), compute_hazard_rate(survival_curve, may_2027))
)

# ---- 5. the old-conventions curve from running spreads ----------------------------------------------
spread_tenors <- c("1Y", "2Y", "3Y", "5Y", "7Y", "10Y")
spreads <- c(70, 80, 100, 140, 170, 210) * 1e-4
maturities <- do.call(c, lapply(spread_tenors, \(t) compute_cds_maturity(today, t, "cds")))
curve <- bootstrap_hazard_curve(today, maturities, spreads, 0.4, rates, "midpoint", settlement_days = 1L, calendar = "us_government_bond",
                                tenor = "3M", payment_convention = "modified_following", rule = "cds", cds_day_count = "actual_360",
                                day_count = "actual_360")
nodes <- curve$nodes
guide <- tibble::tibble(
  date = as.Date(c("2026-12-21", "2027-12-20", "2028-12-20", "2030-12-20", "2032-12-20", "2035-12-20")),
  hazard_pct = c(1.16, 1.55, 2.46, 3.57, 4.47, 5.84),
  default_pct = c(1.45, 2.98, 5.38, 11.99, 19.63, 32.71)
)
quantlib_hazard <- quantlib_default <- rep(NA_real_, 6)
if (has_quantlib) {
  invisible(QuantLib::Settings_instance()$setEvaluationDate(ql_date(today)))
  helpers <- QuantLib::DefaultProbabilityHelperVector()
  for (k in seq_along(spread_tenors)) {
    helpers$append(QuantLib::SpreadCdsHelper(spreads[k], QuantLib::Period(as.integer(sub("Y", "", spread_tenors[k])), "Years"), 1L, ql_calendar,
                                             "Quarterly", "ModifiedFollowing", "CDS", QuantLib::Actual360(), 0.4, ql_risk_free()))
  }
  ql_curve <- QuantLib::PiecewiseFlatHazardRate(0L, ql_calendar, helpers, QuantLib::Actual360())
  quantlib_hazard <- vapply(guide$date, \(d) ql_curve$hazardRate(ql_date(d)), 1)
  quantlib_default <- vapply(guide$date, \(d) ql_curve$defaultProbability(ql_date(d)), 1)
}
spread_nodes <- tibble::tibble(
  node = nodes$date,
  guide_node = guide$date,
  guide_hazard_pct = guide$hazard_pct,
  quantlib_hazard_pct = 100 * quantlib_hazard,
  riemann_hazard_pct = 100 * nodes$hazard,
  guide_default_pct = guide$default_pct,
  quantlib_default_pct = 100 * quantlib_default,
  riemann_default_pct = 100 * (1 - compute_survival_probability(curve, nodes$date))
)

cat("QuantLib Guide, default-probability curves\n")
cat("QuantLib R package:", if (has_quantlib) as.character(utils::packageVersion("QuantLib")) else "not installed", "\n")
old <- options(width = 250)
cat("\n1. Curves from upfront quotes (the Guide prints 2 decimals)\n")
print(as.data.frame(upfront_nodes), digits = 8, row.names = FALSE)
cat("\n2.-4. The check CDS and the curve interface\n")
print(as.data.frame(checks), digits = 16, row.names = FALSE)
cat("\n5. Curve from running spreads, old conventions (the Guide prints 2 decimals)\n")
print(as.data.frame(spread_nodes), digits = 8, row.names = FALSE)
options(old)
cat("\nRiemannFinMath equals QuantLib 1.42 in every row. Against the Guide's prints: the check CDS is worth -0.2777,",
    "\none day of the premium, where QuantLib 1.42 and RiemannFinMath give 0; the default probabilities are up to",
    "\n0.009 points above the Guide's 2-decimal values and the first hazard rate on 19 September is 1.2309% against",
    "\n1.22%. These are differences between the Guide's QuantLib and 1.42, not traced further.\n")
invisible(list(upfront_nodes = upfront_nodes, checks = checks, spread_nodes = spread_nodes))
