# FX smiles and PRDC notes on them. Synthetic inputs.

smile_curves <- function() {
  list(jpy = create_flat_curve("2026-11-02", 0.012, "actual_365_fixed", "continuous"),
       usd = create_flat_curve("2026-11-02", 0.035, "actual_365_fixed", "continuous"))
}

smile_vols <- function() {
  rbind(c(0.150, 0.130, 0.115, 0.118, 0.135),
        c(0.156, 0.134, 0.118, 0.121, 0.140),
        c(0.161, 0.138, 0.121, 0.124, 0.146))
}

test_that("strikes and deltas invert each other", {
  for (dt in c("spot", "forward", "premium_adjusted_spot", "premium_adjusted_forward")) {
    k <- compute_fx_strike_from_delta(c(-0.1, -0.25, 0.25, 0.1), c("put", "put", "call", "call"), dt, 150, 0.99, 0.96, 0.12)
    back <- compute_fx_delta_from_strike(k, c("put", "put", "call", "call"), dt, 150, 0.99, 0.96, 0.12)
    expect_equal(back, c(-0.1, -0.25, 0.25, 0.1), tolerance = 1e-9)
  }
  # the spot delta by its formula: phi P_f N(phi d1); the strike uses QuantLib's inverse normal
  # distribution (Acklam's, relative error below 1.15e-9), so the delta comes back to about 1e-10
  k <- compute_fx_strike_from_delta(0.25, "call", "spot", 150, 0.99, 0.96, 0.12)
  f <- 150 * 0.96 / 0.99
  expect_equal(0.96 * pnorm(log(f / k) / 0.12 + 0.06), 0.25, tolerance = 1e-8)
  expect_equal(compute_fx_atm_strike("delta_neutral", "spot", 150, 0.99, 0.96, 0.12), f * exp(0.12^2 / 2), tolerance = 1e-14)
  expect_equal(compute_fx_atm_strike("delta_neutral", "premium_adjusted_forward", 150, 0.99, 0.96, 0.12), f * exp(-0.12^2 / 2), tolerance = 1e-14)
  expect_error(compute_fx_atm_strike("put_call_50", "spot", 150, 0.99, 0.96, 0.12), "forward delta")
  expect_error(compute_fx_strike_from_delta(0.25, "put", "spot", 150, 0.99, 0.96, 0.12), "incoherent")
})

test_that("the smile is interpolated as a natural spline or linearly, with its extrapolations", {
  k <- smile_curves()
  spline <- create_fx_smile(150, k$jpy, k$usd, c(1, 2, 5), c(-0.1, -0.25), c(0.25, 0.1), smile_vols(), interpolation = "cubic_spline")
  points <- compute_fx_smile_strikes(spline, 1.5)
  expect_equal(nrow(points), 5L)
  expect_true(all(diff(points$strike) > 0))
  # the natural spline through the smile's points, by R
  inside <- seq(min(points$strike) + 0.5, max(points$strike) - 0.5, length.out = 25)
  by_r <- stats::splinefun(points$strike, points$volatility, method = "natural")(inside)
  expect_equal(compute_fx_smile_volatility(spline, 1.5, inside), by_r, tolerance = 1e-12)
  # flat beyond the lowest and highest strikes
  flat <- create_fx_smile(150, k$jpy, k$usd, c(1, 2, 5), c(-0.1, -0.25), c(0.25, 0.1), smile_vols(), flat_strike_extrapolation = TRUE)
  edges <- compute_fx_smile_strikes(flat, 1.5)
  expect_equal(compute_fx_smile_volatility(flat, 1.5, c(50, 400)), edges$volatility[c(1, 5)], tolerance = 1e-14)
  # linear: the end segments extended
  linear <- create_fx_smile(150, k$jpy, k$usd, c(1, 2, 5), c(-0.1, -0.25), c(0.25, 0.1), smile_vols())
  e <- compute_fx_smile_strikes(linear, 1.5)
  low <- e$volatility[1] + (e$volatility[2] - e$volatility[1]) / (e$strike[2] - e$strike[1]) * (110 - e$strike[1])
  expect_equal(compute_fx_smile_volatility(linear, 1.5, 110), low, tolerance = 1e-13)
  # on an expiry the ATM pillar is its own volatility, and the variance is linear in time between expiries
  atm <- compute_fx_smile_strikes(linear, 2)$strike[3]
  expect_equal(compute_fx_smile_volatility(linear, 2, atm), 0.118, tolerance = 1e-14)
  expect_error(create_fx_smile(150, k$jpy, k$usd, c(1, 2, 5), c(-0.1, -0.25), c(0.25, 0.1), smile_vols()[, 1:4]), "volatilities")
  expect_error(create_fx_smile(150, k$jpy, k$usd, c(2, 1, 5), c(-0.1, -0.25), c(0.25, 0.1), smile_vols()), "increasing")
})

test_that("a PRDC note on the smile is two calls per coupon", {
  k <- smile_curves()
  smile <- create_fx_smile(150, k$jpy, k$usd, c(1, 2, 5), c(-0.1, -0.25), c(0.25, 0.1), smile_vols())
  r <- compute_prdc_smile_value(smile, 1:5, rep(1, 5), 150, 0.8, 0.78, 0, 0.08, notional = 1)
  black <- function(f, x, v, t) {
    d1 <- (log(f / x) + v^2 * t / 2) / (v * sqrt(t))
    f * pnorm(d1) - x * pnorm(d1 - v * sqrt(t))
  }
  cp <- r$coupons
  rate <- 0.8 / 150 * (black(cp$forward, cp$floor_strike, cp$floor_volatility, cp$time) - black(cp$forward, cp$cap_strike, cp$cap_volatility, cp$time))
  expect_equal(cp$rate, rate, tolerance = 1e-13)
  expect_equal(cp$floor_strike, rep(0.78 * 150 / 0.8, 5), tolerance = 1e-14)
  expect_equal(cp$forward, 150 * exp((0.012 - 0.035) * (1:5)), tolerance = 1e-14)
  expect_equal(r$summary$value, sum(cp$value) + exp(-0.012 * 5), tolerance = 1e-14)
  expect_equal(nrow(r$pillar_vega), 15L)
  expect_equal(sum(r$pillar_vega$d_npv), r$summary$fx_vega, tolerance = 1e-6)
  # one volatility everywhere: the note of compute_prdc_value()
  flat <- create_fx_smile(150, k$jpy, k$usd, c(1, 2, 5), c(-0.1, -0.25), c(0.25, 0.1), matrix(0.12, 3, 5))
  a <- compute_prdc_smile_value(flat, 1:5, rep(1, 5), 150, 0.8, 0.78, 0, 0.08)
  b <- compute_prdc_value(150, 0.12, k$jpy, k$usd, 1:5, rep(1, 5), 150, 0.8, 0.78, 0, 0.08)
  expect_equal(a$summary$value, b$summary$npv, tolerance = 1e-13)
  expect_equal(a$summary$fx_delta, b$summary$fx_delta, tolerance = 1e-6)
  expect_equal(a$summary$fx_vega, b$summary$fx_vega, tolerance = 1e-6)
  expect_error(compute_prdc_smile_value(list(), 1, 1, 150, 0.8, 0.78), "create_fx_smile")
})
