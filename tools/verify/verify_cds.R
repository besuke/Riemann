# Credit default swaps through the R package against the QuantLib golden values. Synthetic inputs.

verify_cds_sensitivity <- function() {
  quotes <- RiemannFinMath::ois_instruments(c("1Y", "5Y", "10Y", "20Y"), c(0.01, 0.015, 0.02, 0.022), 2L, 1L, 0L)
  rates <- RiemannFinMath::build_ois_curve("2026-11-02", "2026-11-02", "tonar", "actual_365_fixed", quotes)
  maturity <- as.Date(c("2027-12-20", "2029-12-20", "2031-12-20", "2036-12-20"))
  spread <- c(0.006, 0.009, 0.012, 0.015)
  purrr::map_dfr(c("midpoint", "isda"), function(engine) {
    swap <- RiemannFinMath::credit_default_swap("buyer", 1e7, 0.01, "2026-11-03", "2031-12-20")
    credit <- RiemannFinMath::bootstrap_hazard_curve("2026-11-02", maturity, spread, 0.4, rates, engine)
    s <- RiemannFinMath::cds_quote_sensitivity(swap, credit, rates, "2026-11-02", engine)
    value <- function(q) {
      RiemannFinMath::cds_value(swap, RiemannFinMath::bootstrap_hazard_curve("2026-11-02", maturity, q, 0.4, rates, engine), 0.4, rates, "2026-11-02", engine)$npv
    }
    numeric <- vapply(seq_along(spread), function(i) {
      up <- spread; up[i] <- up[i] + 1e-6
      down <- spread; down[i] <- down[i] - 1e-6
      (value(up) - value(down)) / 2e-6 * 1e-4
    }, 0)
    tibble(engine = engine, maturity = s$maturity, spread_bp = s$spread * 1e4, npv_per_bp_ad = s$npv_per_bp, npv_per_bp_rebuilt = numeric, diff = abs(s$npv_per_bp - numeric))
  })
}
