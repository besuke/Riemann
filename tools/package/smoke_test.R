# Checks an installed RiemannFinMath in a new R session: the quickstart and one representative of each
# stage from P5. Inputs are synthetic. Paths are few, because this checks the install, not the numbers;
# the comparisons with QuantLib and GaussQuant are in the Rust tests and tools/verify.
#
# Usage: Rscript --vanilla tools/package/smoke_test.R <expected version> [<library>]
# With <library>, only that library is searched, so tibble and its dependencies must be there too.

args <- commandArgs(trailingOnly = TRUE)
if (length(args) < 1L) stop("usage: smoke_test.R <expected version> [<library>]")
if (length(args) >= 2L) .libPaths(c(normalizePath(args[2]), .Library))

failures <- 0L
check <- function(name, ok) {
  ok <- isTRUE(ok)
  cat(sprintf("%-4s %s\n", if (ok) "PASS" else "FAIL", name))
  if (!ok) failures <<- failures + 1L
}

path <- find.package("RiemannFinMath")
cat("Library paths:", paste(.libPaths(), collapse = "; "), "\n")
cat("Loaded from:", path, "\n")
cat("Rust tools on the PATH:", paste(names(Filter(nzchar, Sys.which(c("cargo", "rustc")))), collapse = ", "), "\n")
check("version", as.character(packageVersion("RiemannFinMath")) == args[1])
if (length(args) >= 2L) check("loaded from the given library", startsWith(normalizePath(path), normalizePath(args[2])))

cat("\n--- quickstart\n")
quickstart <- system.file("examples", "quickstart.R", package = "RiemannFinMath")
check("quickstart is installed", nzchar(quickstart))
source(quickstart, echo = FALSE, local = new.env())
check("quickstart ran", TRUE)
check("help is installed", length(utils::help("prdc_value", package = "RiemannFinMath")) == 1L)

cat("\n--- from P5\n")
library(RiemannFinMath, warn.conflicts = FALSE)
today <- "2026-11-02"
tenors <- c("1Y", "2Y", "3Y", "5Y", "7Y", "10Y")
jpy <- build_ois_curve(today, today, "tonar", "actual_365_fixed", ois_instruments(tenors, c(0.01, 0.012, 0.013, 0.015, 0.016, 0.018), 2L, 1L, 0L))
usd <- build_ois_curve(today, today, "sofr", "actual_365_fixed", ois_instruments(tenors, c(0.04, 0.039, 0.038, 0.037, 0.036, 0.035), 2L, 1L, 0L))
bond <- fixed_rate_bond(
  settlement_days = 2L, face_amount = 100, effective_date = "2026-09-30", termination_date = "2033-09-30", tenor = "6M",
  calendar = "japan", convention = "unadjusted", termination_convention = "unadjusted", rule = "backward", end_of_month = FALSE,
  coupon_rate = 0.015, day_count = "actual_365_fixed", payment_convention = "following", redemption = 100
)

# P5: autodiff on bonds
s <- yield_sensitivity(bond, 0.02, "actual_365_fixed", "compounded", 2L, "2026-11-04")
v <- bond_values(bond, 0.02, "actual_365_fixed", "compounded", 2L, "2026-11-04")
check("P5 yield derivative is -duration * price", abs(s$d_price_d_yield / (-v$duration_modified * v$dirty_price) - 1) < 1e-10)
q <- quote_sensitivity(bond, jpy, today)
check("P5 quote sensitivities have one row per quote", nrow(q) == length(tenors))
check("P5 curve node sensitivities have one row per node", nrow(curve_node_sensitivity(bond, jpy, today)) == length(tenors))

# P6: options
call <- european_option("call", 100, 105, 0.03, 0.01, 0.25, 1.5)
put <- european_option("put", 100, 105, 0.03, 0.01, 0.25, 1.5)
check("P6 put-call parity", abs(call$value - put$value - (100 * exp(-0.015) - 105 * exp(-0.045))) < 1e-10)
check("P6 American put is at least the European", {
  x <- binomial_option("put", c("american", "european"), 100, 120, 0.03, 0.01, 0.25, 1.5, 100L)
  x$value[1] > x$value[2]
})
check("P6 knock-in plus knock-out is the vanilla", abs(sum(barrier_option("call", c("down_in", "down_out"), 100, 105, 80, 0, 0.03, 0.01, 0.25, 1.5)$value) - call$value) < 1e-9)
check("P6 implied volatility recovers the input", abs(implied_volatility("call", call$value, 100, 105, 0.03, 0.01, 1.5) - 0.25) < 1e-8)
m <- monte_carlo_european("call", 100, 105, 0.03, 0.01, 0.25, 1.5, paths = 20000, seed = 1, threads = 2L)
check("P6 Monte Carlo is within 5 standard errors", abs(m$value - call$value) < 5 * m$standard_error)
check("P6 forward option runs", forward_option("bachelier", "put", 0.02, 0.025, 0.006, 2, 0.95)$value > 0)

# P7: asset swaps and futures
a <- ois_asset_swap(bond, 101, jpy, today)
check("P7 the fair spread gives a value of 0", abs(ois_asset_swap(bond, 101, jpy, today, spread_bp = a$fair_spread_bp)$npv) < 1e-9)
check("P7 asset swap quote sensitivities run", nrow(asset_swap_quote_sensitivity(bond, 101, jpy, today)) == length(tenors))
foreign_bond <- fixed_rate_bond(
  settlement_days = 2L, face_amount = 100, effective_date = "2026-09-30", termination_date = "2031-09-30", tenor = "6M",
  calendar = "us_sofr", convention = "unadjusted", termination_convention = "unadjusted", rule = "backward", end_of_month = FALSE,
  coupon_rate = 0.04, day_count = "actual_365_fixed", payment_convention = "following", redemption = 100
)
check("P7 yen asset swap runs", is.finite(yen_asset_swap(foreign_bond, 100, usd, jpy, 150, -30, today)$yen_asw_bp))
basket <- future_basket(list(bond, bond), c(95, 99), 130, c(0.70, 0.75), 0.01, "2026-11-10", "2026-12-20")
check("P7 the basket marks one cheapest to deliver", sum(basket$ctd) == 1L)

# P9: Hull-White and SABR
h <- hull_white_swaption(TRUE, 2, 3:7, rep(0.02, 5), 0.1, 0.01, jpy)
check("P9 swaption has positive value and vega", h$value > 0 && h$d_volatility > 0)
check("P9 Bermudan is at least the European", hull_white_bermudan_swaption(TRUE, 2, 3:7, rep(0.02, 5), 2:6, 0.1, 0.01, jpy, points = 321L, limit = 0.2, spacing = 0.1) >= h$value - 1e-3)
check("P9 a call lowers a callable bond", hull_white_callable_bond((1:10) / 2, 1.5, c(rep(0, 9), 100), c(NA, rep(100, 8), NA), 0.15, 0.012, jpy)$issuer_call_value > 0)
k <- 0.03 * c(0.7, 0.85, 1, 1.15, 1.3)
check("P9 SABR calibration recovers its own smile", calibrate_sabr("normal", k, sabr_volatility("normal", k, 0.03, 2, 0.03, 0.5, 0.4, -0.3), 0.03, 2, 0.5)$root_mean_square_error < 1e-8)

# P8: credit default swaps, with both engines
maturity <- as.Date(c("2027-12-20", "2029-12-20", "2031-12-20"))
for (engine in c("midpoint", "isda")) {
  credit <- bootstrap_hazard_curve(today, maturity, c(0.006, 0.009, 0.012), 0.4, jpy, engine)
  swap <- credit_default_swap("buyer", 1e7, 0.01, "2026-11-03", "2031-12-20")
  value <- cds_value(swap, credit, 0.4, jpy, today, engine)
  check(paste("P8", engine, "swap at its fair spread is worth 0"), abs(cds_value(credit_default_swap("buyer", 1e7, value$fair_spread, "2026-11-03", "2031-12-20"), credit, 0.4, jpy, today, engine)$npv) < 1e-3)
  check(paste("P8", engine, "quote sensitivities run"), nrow(cds_quote_sensitivity(swap, credit, jpy, today, engine)) == 3L)
}
check("P8 survival falls", all(diff(survival_probability(hazard_curve(today, "2040-01-01", 0.02), as.Date(c("2028-01-01", "2030-01-01")))) < 0))
check("P8 standard maturity falls on the twentieth", format(cds_maturity(today, "5Y"), "%d") == "20")

# P10: PRDC
terms <- list(150, 0.12, jpy, usd, 1:5, rep(1, 5), 150, 0.8, 0.78, 0, 0.08, notional = 1e8)
closed <- do.call(prdc_value, terms)
path_value <- do.call(prdc_value, c(terms, list(paths = 20000, seed = 1, threads = 2L)))
check("P10 Monte Carlo is within 5 standard errors of the closed form", abs(path_value$summary$npv - closed$summary$npv) < 5 * path_value$summary$standard_error)
check("P10 sensitivities cover both curves", setequal(closed$curve_sensitivity$curve, c("domestic", "foreign")))

cat(sprintf("\n%d failed\n", failures))
if (failures > 0L) quit(save = "no", status = 1L)
