# Credit default swaps through the public functions. Synthetic inputs.

cds_rates <- function() {
  build_ois_curve("2026-11-02", "2026-11-02", "tonar", "actual_365_fixed",
                  ois_instruments(c("1Y", "5Y", "10Y", "20Y"), c(0.01, 0.015, 0.02, 0.022), 2L, 1L, 0L))
}

test_that("the fair spread makes the value 0 in both engines", {
  hazard <- hazard_curve("2026-11-02", "2040-01-01", 0.02)
  for (engine in c("midpoint", "isda")) {
    v <- cds_value(credit_default_swap("buyer", 1e7, 0.01, "2026-11-03", "2031-12-20"), hazard, 0.4, cds_rates(), "2026-11-02", engine)
    fair <- cds_value(credit_default_swap("buyer", 1e7, v$fair_spread, "2026-11-03", "2031-12-20"), hazard, 0.4, cds_rates(), "2026-11-02", engine)
    expect_lt(abs(fair$npv), 1e-3)
    seller <- cds_value(credit_default_swap("seller", 1e7, 0.01, "2026-11-03", "2031-12-20"), hazard, 0.4, cds_rates(), "2026-11-02", engine)
    expect_equal(seller$npv, -v$npv, tolerance = 1e-10)
  }
})

test_that("survival falls and a bootstrap reprices its spreads", {
  m <- as.Date(c("2027-12-20", "2029-12-20", "2031-12-20"))
  s <- c(0.006, 0.009, 0.012)
  curve <- bootstrap_hazard_curve("2026-11-02", m, s, 0.4, cds_rates(), "midpoint")
  expect_true(all(diff(survival_probability(curve, m)) < 0))
  expect_equal(nrow(curve$nodes), 3)
  for (i in seq_along(m)) {
    v <- cds_value(credit_default_swap("buyer", 100, 0.01, "2026-11-02", m[i], rule = "backward"), curve, 0.4, cds_rates(), "2026-11-02", "midpoint")
    # the quotes rebate one day of accrual and this swap, traded the day before, rebates none
    expect_lt(abs(v$fair_spread - s[i]), 5e-5)
  }
})

test_that("quote sensitivities agree with rebuilding the curve", {
  m <- as.Date(c("2027-12-20", "2029-12-20", "2031-12-20"))
  s <- c(0.006, 0.009, 0.012)
  cds <- credit_default_swap("buyer", 1e7, 0.01, "2026-11-03", "2030-12-20")
  value <- function(q, r = 0.4) cds_value(cds, bootstrap_hazard_curve("2026-11-02", m, q, 0.4, cds_rates(), "midpoint"), r, cds_rates(), "2026-11-02", "midpoint")$npv
  x <- cds_quote_sensitivity(cds, bootstrap_hazard_curve("2026-11-02", m, s, 0.4, cds_rates(), "midpoint"), cds_rates(), "2026-11-02", "midpoint")
  for (i in seq_along(s)) {
    up <- s; up[i] <- up[i] + 1e-5
    down <- s; down[i] <- down[i] - 1e-5
    numeric <- (value(up) - value(down)) / 2e-5 * 1e-4
    expect_lt(abs(x$npv_per_bp[i] - numeric), 1e-3)
  }
  expect_lt(abs(x$npv_per_recovery_percent[1] - (value(s, 0.41) - value(s, 0.39)) / 2), 1e-2)
})

test_that("maturities are on the twentieth, and bad names fail", {
  expect_equal(format(cds_maturity("2026-11-02", "5Y"), "%d"), "20")
  expect_error(credit_default_swap("broker", 1e7, 0.01, "2026-11-03", "2031-12-20"), "protection side")
  expect_error(credit_default_swap("buyer", 1e7, 0.01, "2026-11-03", "2031-12-20", rebates_accrual = NA), "TRUE or FALSE")
})
