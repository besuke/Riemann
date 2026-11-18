# The choice of collateral currency. Synthetic inputs.

today <- as.Date("2026-11-04")
yen <- create_zero_curve(today + c(0, 365, 1096, 3652), c(0.005, 0.005, 0.008, 0.012), "actual_365_fixed")
usd <- create_zero_curve(today + c(0, 365, 1096, 3652), c(0.009, 0.009, 0.007, 0.006), "actual_365_fixed")

test_that("one curve is reproduced and the choice never discounts less than a single collateral", {
  dates <- today + c(30, 365, 1500, 3652)
  alone <- create_ctd_curve(today, today + 3652, list(JPY = yen))
  expect_equal(compute_discount(alone, dates), compute_discount(yen, dates), tolerance = 1e-13)
  both <- create_ctd_curve(today, today + 3652, list(JPY = yen, USD = usd))
  expect_true(all(compute_discount(both, dates) <= pmin(compute_discount(yen, dates), compute_discount(usd, dates)) + 1e-15))
  expect_equal(both$choices$collateral[c(1, nrow(both$choices))], c("USD", "JPY"))
  expect_equal(sum(both$choices$days), 3652L)
})

test_that("the dollar collateral curve can come from a cross-currency bootstrap", {
  sofr <- create_flat_curve(today, 0.042, "actual_365_fixed", index = "sofr")
  tonar <- create_flat_curve(today, 0.005, "actual_365_fixed", index = "tonar")
  quotes <- create_xccy_instruments("basis_swap", c("2Y", "5Y", "10Y"), c(-0.0030, -0.0040, -0.0045))
  yen_in_usd <- with_extrapolation(bootstrap_xccy_curve(today, sofr, tonar, base_is_collateral = TRUE, fx_spot = 150, instruments = quotes))
  ctd <- create_ctd_curve(today, today + 3000, list(JPY = tonar, USD = yen_in_usd))
  expect_true(all(ctd$choices$advantage >= 0))
  expect_lte(compute_discount(ctd, today + 3000), min(compute_discount(tonar, today + 3000), compute_discount(yen_in_usd, today + 3000)))
  expect_error(create_ctd_curve(today, today + 100, list(tonar)), "named list")
})
