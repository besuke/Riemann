# Cross-currency curves and the yen asset swap on them. Synthetic inputs.

today <- "2026-11-04"
sofr <- create_flat_curve(today, 0.042, "actual_365_fixed", index = "sofr")
tonar <- create_flat_curve(today, 0.005, "actual_365_fixed", index = "tonar")

test_that("a cross-currency curve reprices its FX swaps and basis swaps", {
  quotes <- create_xccy_instruments(c("fx_swap", "fx_swap", "basis_swap", "basis_swap", "basis_swap"), c("3M", "6M", "2Y", "5Y", "10Y"),
                                    c(-1.62, -3.2, -0.0042, -0.0050, -0.0048))
  curve <- bootstrap_xccy_curve(today, sofr, tonar, base_is_collateral = FALSE, fx_spot = 150, instruments = quotes)
  expect_s3_class(curve, "riemann_ois_curve")
  expect_lt(max(abs(curve$repricing$implied_quote - curve$repricing$quote)), 1e-9)
  # a more negative basis on the yen leg lowers the yen leg's value, so at par the dollar leg is worth
  # less: the dollar discount rate under yen collateral rises
  deeper <- bootstrap_xccy_curve(today, sofr, tonar, FALSE, 150, create_xccy_instruments("basis_swap", "5Y", -0.0080))
  shallow <- bootstrap_xccy_curve(today, sofr, tonar, FALSE, 150, create_xccy_instruments("basis_swap", "5Y", -0.0020))
  expect_lt(compute_discount(deeper, as.Date("2031-11-04")), compute_discount(shallow, as.Date("2031-11-04")))
  mtm <- bootstrap_xccy_curve(today, sofr, tonar, FALSE, 150, create_xccy_instruments("basis_swap", c("2Y", "5Y"), c(-0.0042, -0.0050), resetting = "base"))
  expect_lt(max(abs(mtm$repricing$implied_quote - mtm$repricing$quote)), 1e-9)
  expect_error(bootstrap_xccy_curve(today, sofr, tonar, FALSE, 150, create_xccy_instruments("deposit", "1Y", 0.01)), "fx_swap or basis_swap")
  expect_error(bootstrap_xccy_curve(today, sofr, tonar, FALSE, -1, quotes), "positive")
})

test_that("the yen asset swap on a curve equal to the constant-z forward gives the constant-z result", {
  bond <- create_fixed_rate_bond(2L, 100, "2026-11-06", "2033-11-06", "6M", "us_government_bond", "unadjusted", "unadjusted", "backward", FALSE,
                                 0.04, "thirty_360_bond_basis", "following", 100)
  constant <- compute_yen_asset_swap(bond, 99.5, sofr, tonar, fx_spot = 150, basis_bp = -45, evaluation_date = today)
  shifted <- create_zero_spreaded_curve(sofr, constant$calibrated_discount_spread_bp / 1e4)
  on_curve <- compute_yen_asset_swap_on_basis_curve(bond, 99.5, sofr, tonar, shifted, fx_spot = 150, evaluation_date = today)
  expect_equal(on_curve$yen_asw_bp, constant$yen_asw_bp, tolerance = 1e-10)
  expect_equal(on_curve$native_asw_bp, constant$native_asw_bp, tolerance = 1e-12)
  # on a bootstrapped basis curve the yen spread moves with the basis, about one for one
  curve_of <- \(basis) bootstrap_xccy_curve(today, sofr, tonar, FALSE, 150, create_xccy_instruments("basis_swap", c("2Y", "7Y"), basis))
  moves <- purrr::map_dbl(list(c(-0.0040, -0.0045), c(-0.0050, -0.0055)), \(b) {
    compute_yen_asset_swap_on_basis_curve(bond, 99.5, sofr, tonar, curve_of(b), 150, today)$yen_asw_bp
  })
  expect_equal(moves[1] - moves[2], 10, tolerance = 0.05)
})
