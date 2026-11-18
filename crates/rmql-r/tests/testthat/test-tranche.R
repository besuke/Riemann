# Credit tranches and CMO waterfalls. Small inputs; the comparison with independent R code on LagrangeFinance
# I-10's portfolio is crates/rmql/tests/golden_tranching.rs.

test_that("tranche losses split the portfolio loss", {
  l <- seq(0, 0.2, by = 0.01)
  parts <- purrr::map2(c(0, 0.03, 0.07, 0.10), c(0.03, 0.07, 0.10, 1), \(a, d) compute_tranche_loss(l, a, d))
  expect_equal(Reduce(`+`, parts), l, tolerance = 1e-15)
  expect_error(compute_tranche_loss(0.1, 0.07, 0.03), "A < D")
})

test_that("expected tranche losses add up to the expected portfolio loss", {
  pd <- seq(0.005, 0.05, length.out = 50)
  d <- compute_credit_loss_distribution(pd, 100, 0.6, 0.25, unit = 60)
  r <- compute_tranche_risk(d, 5000, c(0, 0.03, 0.07, 0.10), c(0.03, 0.07, 0.10, 1))
  expect_equal(sum(r$expected_tranche_loss), attr(d, "mean") / 5000, tolerance = 1e-13)
  expect_true(all(diff(r$expected_loss_of_tranche) < 0))
  # the large pool: the expected tranche losses add up to PD LGD
  lhp <- compute_lhp_expected_tranche_loss(0.02, 0.6, 0.25, c(0, 0.03, 0.07, 0.10), c(0.03, 0.07, 0.10, 1))
  expect_equal(sum(lhp), 0.02 * 0.6, tolerance = 1e-12)
})

test_that("the legs: a zero spread when nothing can default, and the equity pays the most", {
  pd <- seq(0.005, 0.05, length.out = 30)
  legs <- purrr::map2(c(0, 0.10), c(0.03, 1), \(a, d) compute_tranche_legs(-log(1 - pd), 100, 0.6, 0.2, 60, seq(0.5, 3, by = 0.5), 0.01, a, d))
  expect_gt(legs[[1]]$fair_spread, legs[[2]]$fair_spread)
  expect_equal(legs[[1]]$loss_leg, sum(legs[[1]]$legs$discount * diff(c(0, legs[[1]]$legs$expected_tranche_loss))), tolerance = 1e-15)
})

test_that("CMO waterfalls pay out exactly the collateral's cash", {
  pool <- compute_mbs_cashflows(create_mbs_pass_through(1000, 0.055, 0.05, psa_multiplier = 2), 0.055)
  s <- compute_sequential_cmo(pool, c(400, 300, 200, 100), 0.05)
  paid <- tapply(s$cashflows$interest + s$cashflows$principal, s$cashflows$month, sum) + s$residual
  expect_equal(as.numeric(paid), pool$interest + pool$scheduled_principal + pool$prepaid_principal, tolerance = 1e-12)
  expect_equal(sum(s$cashflows$principal) - sum(s$cashflows$accrued), 1000, tolerance = 1e-9)
  expect_true(all(diff(s$wal) > 0))
  low <- compute_mbs_cashflows(create_mbs_pass_through(1000, 0.055, 0.05, psa_multiplier = 1), 0.055)
  high <- compute_mbs_cashflows(create_mbs_pass_through(1000, 0.055, 0.05, psa_multiplier = 3), 0.055)
  schedule <- compute_pac_schedule(low, high)
  wal_pac <- purrr::map_dbl(c(1, 2, 3), \(psa) {
    pool <- compute_mbs_cashflows(create_mbs_pass_through(1000, 0.055, 0.05, psa_multiplier = psa), 0.055)
    compute_pac_support_cmo(pool, schedule, c(sum(schedule), 1000 - sum(schedule)), 0.05)$wal[1]
  })
  # inside the band the PAC keeps its schedule, so its average life does not move
  expect_equal(wal_pac[2], wal_pac[1], tolerance = 1e-9)
  expect_equal(wal_pac[3], wal_pac[1], tolerance = 1e-9)
})
