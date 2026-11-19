# The compounding conventions and the step-up and amortizing bonds. Synthetic inputs.

bond <- function(...) create_fixed_rate_bond(3L, 10000, "2018-02-08", "2028-02-08", "1Y", "target", "following", "following",
                                            "backward", FALSE, ...)

test_that("SimpleThenCompounded is simple up to one period and compounded beyond it", {
  curve <- create_flat_curve("2024-01-02", 0.04, "actual_365_fixed", "simple_then_compounded", 2L)
  expect_equal(compute_discount(curve, as.Date("2024-04-01")), 1 / (1 + 0.04 * 90 / 365), tolerance = 1e-14)
  expect_equal(compute_discount(curve, as.Date("2026-01-01")), (1 + 0.04 / 2)^(-2 * 730 / 365), tolerance = 1e-14)
  later <- create_flat_curve("2024-01-02", 0.04, "actual_365_fixed", "compounded_then_simple", 2L)
  expect_equal(compute_discount(later, as.Date("2026-01-01")), 1 / (1 + 0.04 * 730 / 365), tolerance = 1e-14)
  expect_error(create_flat_curve("2024-01-02", 0.04, "actual_365_fixed", "simple_then_compounded", 0L), "frequency")
})

test_that("a yield in SimpleThenCompounded prices back", {
  b <- bond(0.03, "thirty_360_bond_basis", "following", 100)
  s <- compute_settlement_date(b, "2024-05-02")
  y <- compute_yield_from_price(b, 98.08, "clean", "thirty_360_bond_basis", "simple_then_compounded", 2L, s)
  # the solver stops at an accuracy of 1e-10 in the yield, as QuantLib does
  expect_equal(compute_clean_price(b, y, "thirty_360_bond_basis", "simple_then_compounded", 2L, s), 98.08, tolerance = 1e-8)
})

test_that("a step-up bond pays each period's rate", {
  b <- bond(c(0.01, 0.02, 0.03), "thirty_360_bond_basis", "following", 100)
  coupons <- get_bond_cashflows(b)
  coupons <- coupons[coupons$kind == "coupon", ]
  accrual <- compute_year_fraction("thirty_360_bond_basis", coupons$accrual_start, coupons$accrual_end)
  expect_equal(coupons$amount[c(1, 2, 3, 10)], 10000 * c(0.01, 0.02, 0.03, 0.03) * accrual[c(1, 2, 3, 10)], tolerance = 1e-12)
  expect_error(bond(rep(0.01, 11), "thirty_360_bond_basis", "following", 100), "notionals and rates")
})

test_that("an amortizing bond repays each fall and prices per 100 of the outstanding notional", {
  b <- bond(0.03, "thirty_360_bond_basis", "following", 100, notionals = c(10000, 8000, 6000))
  flows <- get_bond_cashflows(b)
  expect_equal(sum(flows$amount[flows$kind == "redemption"]), 10000)
  expect_equal(flows$amount[flows$kind == "redemption"][1:2], c(2000, 2000))
  curve <- create_flat_curve("2024-05-02", 0.0, "actual_360")
  v <- compute_bond_value_on_curve(b, curve, "2024-05-02")
  # at zero rates the dirty price is the remaining payments per 100 of the outstanding 6000
  remaining <- flows$payment_date > compute_settlement_date(b, "2024-05-02")
  expect_equal(v$dirty_price, sum(flows$amount[remaining]) / 6000 * 100, tolerance = 1e-12)
})
