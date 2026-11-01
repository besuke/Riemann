# Asset swaps and futures basis through the public functions. Synthetic inputs.

asw_bond <- function(coupon = 0.015, years = 7L, calendar = "japan") {
  fixed_rate_bond(
    settlement_days = 2L, face_amount = 100, effective_date = "2026-09-30", termination_date = sprintf("%d-09-30", 2026L + years),
    tenor = "6M", calendar = calendar, convention = "unadjusted", termination_convention = "unadjusted", rule = "backward",
    end_of_month = FALSE, coupon_rate = coupon, day_count = "actual_365_fixed", payment_convention = "following", redemption = 100
  )
}
asw_curve <- function(index, q = c(0.006, 0.0085, 0.011, 0.0125, 0.0145)) {
  build_ois_curve("2026-10-06", "2026-10-06", index, "actual_365_fixed", ois_instruments(c("1Y", "3Y", "5Y", "7Y", "10Y"), q, 2L, 1L, 0L))
}

test_that("the fair spread makes the value 0 and the fair price undoes the spread", {
  a <- ois_asset_swap(asw_bond(), 101, asw_curve("tonar"), "2026-10-06")
  b <- ois_asset_swap(asw_bond(), 101, asw_curve("tonar"), "2026-10-06", spread_bp = a$fair_spread_bp)
  expect_lt(abs(b$npv), 1e-9)
  c <- ois_asset_swap(asw_bond(), 101, asw_curve("tonar"), "2026-10-06", spread_bp = 25)
  expect_equal(ois_asset_swap(asw_bond(), c$fair_clean_price, asw_curve("tonar"), "2026-10-06", spread_bp = 25)$npv, 0, tolerance = 1e-9)
})

test_that("a higher price gives a lower asset swap spread", {
  x <- ois_asset_swap(asw_bond(), c(95, 105), asw_curve("tonar"), "2026-10-06")
  expect_gt(x$fair_spread_bp[1], x$fair_spread_bp[2])
})

test_that("quote sensitivities agree with rebuilding the curve", {
  q <- c(0.006, 0.0085, 0.011, 0.0125, 0.0145)
  s <- asset_swap_quote_sensitivity(asw_bond(), 101, asw_curve("tonar", q), "2026-10-06")
  expect_equal(s$tenor, c("1Y", "3Y", "5Y", "7Y", "10Y"))
  for (i in seq_along(q)) {
    up <- q; up[i] <- up[i] + 1e-6
    down <- q; down[i] <- down[i] - 1e-6
    numeric <- (ois_asset_swap(asw_bond(), 101, asw_curve("tonar", up), "2026-10-06")$fair_spread_bp -
                  ois_asset_swap(asw_bond(), 101, asw_curve("tonar", down), "2026-10-06")$fair_spread_bp) / 2e-6 * 1e-4
    expect_lt(abs(s$spread_bp_per_quote_bp[i] - numeric), 1e-6)
  }
})

test_that("the yen asset swap is close to the simple sum and repeats the native spread", {
  y <- yen_asset_swap(asw_bond(calendar = "us_sofr"), 100, asw_curve("sofr", c(0.038, 0.037, 0.037, 0.038, 0.039)), asw_curve("tonar"), 150, -30, "2026-10-06")
  expect_equal(y$native_asw_bp, ois_asset_swap(asw_bond(calendar = "us_sofr"), 100, asw_curve("sofr", c(0.038, 0.037, 0.037, 0.038, 0.039)), "2026-10-06")$fair_spread_bp, tolerance = 1e-10)
  expect_lt(abs(y$yen_asw_bp - y$simple_sum_bp), 30)
})

test_that("the basket marks the smallest net basis", {
  b <- list(asw_bond(0.01), asw_bond(0.02), asw_bond(0.03))
  x <- future_basket(b, c(92, 99, 106), 130, c(0.70, 0.75, 0.80), 0.01, "2026-11-10", "2026-12-20")
  expect_equal(sum(x$ctd), 1L)
  expect_equal(x$net_basis[x$ctd], min(x$net_basis))
  one <- future_basis(b[[2]], 99, 130, 0.75, 0.01, "2026-11-10", "2026-12-20")
  expect_equal(x$net_basis[2], one$net_basis)
  expect_error(future_basket(b, 92, 130, 0.7, 0.01, "2026-11-10", "2026-12-20"), "same length")
})
