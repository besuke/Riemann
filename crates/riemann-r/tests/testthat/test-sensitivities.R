# Sensitivities against numeric differences through the public functions. Synthetic inputs.

bond <- function() {
  fixed_rate_bond(
    settlement_days = 1L, face_amount = 100, effective_date = "2025-03-20", termination_date = "2033-03-20",
    tenor = "6M", calendar = "japan", convention = "following", termination_convention = "following",
    rule = "backward", end_of_month = FALSE, coupon_rate = 0.015, day_count = "actual_365_fixed",
    payment_convention = "following", redemption = 100
  )
}
rates <- c(0.002, 0.004, 0.006, 0.011, 0.016)
curve_of <- function(q) {
  build_ois_curve("2026-11-02", "2026-11-02", "tonar", "actual_365_fixed",
                  ois_instruments(c("ON", "1Y", "2Y", "5Y", "10Y"), q, 2L, 1L, 0L))
}

test_that("the yield derivatives are the duration and convexity", {
  s <- yield_sensitivity(bond(), 0.02, "actual_365_fixed", "compounded", 2L, "2026-11-04")
  v <- bond_values(bond(), 0.02, "actual_365_fixed", "compounded", 2L, "2026-11-04")
  expect_equal(s$dirty_price, v$dirty_price, tolerance = 1e-14)
  expect_equal(s$d_price_d_yield, -v$duration_modified * v$dirty_price, tolerance = 1e-11)
  expect_equal(s$d2_price_d_yield2, v$convexity * v$dirty_price, tolerance = 1e-11)
})

test_that("yield sensitivity keeps failed rows on request", {
  s <- yield_sensitivity(bond(), c(0.02, 0.02), "actual_365_fixed", "compounded", 2L, c("2026-11-04", "2040-01-04"), on_error = "na")
  expect_true(is.na(s$dirty_price[2]) && !is.na(s$error[2]))
  expect_error(yield_sensitivity(bond(), 0.02, "actual_365_fixed", "compounded", 2L, "2040-01-04"), "non tradable")
})

test_that("quote sensitivities agree with rebuilding the curve", {
  s <- quote_sensitivity(bond(), curve_of(rates), "2026-11-02")
  expect_equal(s$tenor, c("ON", "1Y", "2Y", "5Y", "10Y"))
  expect_equal(s$npv[1], value_on_curve(bond(), curve_of(rates), "2026-11-02")$npv)
  for (i in seq_along(rates)) {
    up <- rates; up[i] <- up[i] + 1e-6
    down <- rates; down[i] <- down[i] - 1e-6
    numeric <- (value_on_curve(bond(), curve_of(up), "2026-11-02")$npv -
                  value_on_curve(bond(), curve_of(down), "2026-11-02")$npv) / 2e-6 * 1e-4
    expect_lt(abs(s$npv_per_bp[i] - numeric), 1e-5)
  }
})

test_that("node sensitivities are in the time order of the nodes", {
  s <- curve_node_sensitivity(bond(), curve_of(rates), "2026-11-02")
  expect_equal(nrow(s), length(rates))
  expect_true(all(diff(s$time) > 0))
  # a spline's node can move a value the other way, but a parallel rise lowers the value
  expect_lt(sum(s$npv_per_bp), 0)
})
