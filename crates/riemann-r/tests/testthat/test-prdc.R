# PRDC through the public functions. Synthetic inputs.

prdc_curves <- function() {
  list(
    jpy = bootstrap_ois_curve("2026-11-02", "2026-11-02", "tonar", "actual_365_fixed", create_ois_instruments(c("1Y", "3Y", "5Y", "7Y"), c(0.01, 0.012, 0.014, 0.016), 2L, 1L, 0L)),
    usd = bootstrap_ois_curve("2026-11-02", "2026-11-02", "sofr", "actual_365_fixed", create_ois_instruments(c("1Y", "3Y", "5Y", "7Y"), c(0.04, 0.038, 0.036, 0.034), 2L, 1L, 0L))
  )
}

test_that("Monte Carlo agrees with the closed form and does not depend on the threads", {
  k <- prdc_curves()
  a <- compute_prdc_value(150, 0.12, k$jpy, k$usd, 1:5, rep(1, 5), 150, 0.8, 0.78, 0, 0.08, notional = 1e8)
  m1 <- compute_prdc_value(150, 0.12, k$jpy, k$usd, 1:5, rep(1, 5), 150, 0.8, 0.78, 0, 0.08, notional = 1e8, paths = 2e5, seed = 3, threads = 1L)
  m4 <- compute_prdc_value(150, 0.12, k$jpy, k$usd, 1:5, rep(1, 5), 150, 0.8, 0.78, 0, 0.08, notional = 1e8, paths = 2e5, seed = 3, threads = 4L)
  expect_identical(m1$summary, m4$summary)
  expect_lt(abs(m1$summary$npv - a$summary$npv), 5 * m1$summary$standard_error)
  expect_lt(abs(m1$summary$fx_delta - a$summary$fx_delta), 0.03 * abs(a$summary$fx_delta))
  expect_equal(a$summary$standard_error, 0)
  expect_setequal(a$curve_sensitivity$curve, c("domestic", "foreign"))
})

test_that("fixed coupons have no FX risk, and bad terms fail", {
  k <- prdc_curves()
  x <- compute_prdc_value(150, 0.12, k$jpy, k$usd, 1:5, rep(1, 5), 150, 0.8, 0.78, fixed_rate = rep(0.03, 5))
  expect_equal(x$summary$fx_delta, 0)
  y <- compute_prdc_value(150, 0.12, k$jpy, k$usd, 1:5, rep(1, 5), 150, 0.8, 0.78, fixed_rate = c(0.03, NA, NA, NA, NA))
  expect_gt(y$summary$fx_delta, 0)
  expect_error(compute_prdc_value(150, 0.12, k$jpy, k$usd, 1:5, rep(1, 5), 150, 0.8, 0.78, floor = 0.1, cap = 0.05), "below the floor")
  expect_error(compute_prdc_value(150, 0.12, k$jpy, k$usd, 1:5, rep(1, 5), 150, 0.8, 0.78, antithetic = NA), "TRUE or FALSE")
})

test_that("the three-factor model against an Euler simulation written here", {
  # flat continuous curves, so that the Hull-White drift is explicit:
  # phi(t) = r + sigma^2 / (2 a^2) (1 - exp(-a t))^2
  today <- "2026-11-02"
  rd <- 0.012; rf <- 0.035
  jpy <- create_flat_curve(today, rd, "actual_365_fixed", "continuous")
  usd <- create_flat_curve(today, rf, "actual_365_fixed", "continuous")
  a <- c(0.03, 0.05); s <- c(0.01, 0.012); sx <- 0.11; rho <- c(0.3, -0.2, 0.4)
  r <- compute_prdc_three_factor_value(150, sx, jpy, usd, c(a[1], s[1]), c(a[2], s[2]), rho, 1:5, rep(1, 5), 150, 0.8, 0.78, 0, 0.08,
                                       notional = 1, paths = 20000L, steps_per_year = 12L, seed = 3)
  expect_lt(abs(r$summary$value - r$summary$analytic), 4 * r$summary$standard_error)
  # Euler on (r_d, r_f, ln S) with 100 steps a year, its own normal numbers
  set.seed(20261107)
  n <- 20000L; dt <- 0.01; steps <- 500L
  phi <- function(r, a, s, t) r + s^2 / (2 * a^2) * (1 - exp(-a * t))^2
  L <- t(chol(matrix(c(1, rho[1], rho[2], rho[1], 1, rho[3], rho[2], rho[3], 1), 3)))
  xd <- xf <- numeric(n); lns <- rep(log(150), n); integral <- numeric(n); value <- numeric(n)
  purrr::walk(seq_len(steps), \(k) {
    t0 <- (k - 1) * dt
    z <- matrix(rnorm(3 * n), n) %*% t(L)
    r_d <- xd + phi(rd, a[1], s[1], t0); r_f <- xf + phi(rf, a[2], s[2], t0)
    lns <<- lns + (r_d - r_f - sx^2 / 2) * dt + sx * sqrt(dt) * z[, 3]
    integral <<- integral + r_d * dt
    xd <<- xd - a[1] * xd * dt + s[1] * sqrt(dt) * z[, 1]
    xf <<- xf + (-a[2] * xf - rho[3] * s[2] * sx) * dt + s[2] * sqrt(dt) * z[, 2]
    if (k %% 100L == 0L) {
      coupon <- pmin(pmax(0.8 * exp(lns) / 150 - 0.78, 0), 0.08)
      value <<- value + exp(-integral) * (coupon + (k == steps))
    }
  })
  euler <- mean(value); euler_se <- sd(value) / sqrt(n)
  expect_lt(abs(euler - r$summary$analytic), 4 * euler_se + 2e-4)
  # no rate volatility: the simple model
  simple <- compute_prdc_value(150, sx, jpy, usd, 1:5, rep(1, 5), 150, 0.8, 0.78, 0, 0.08)$summary$npv
  flat <- compute_prdc_three_factor_value(150, sx, jpy, usd, c(a[1], 0), c(a[2], 0), rho, 1:5, rep(1, 5), 150, 0.8, 0.78, 0, 0.08)
  expect_equal(flat$summary$value, simple, tolerance = 1e-13)
  expect_error(compute_prdc_three_factor_value(150, sx, jpy, usd, c(0.03, 0.01), c(0.05, 0.01), c(0.9, 0.9, -0.9), 1:5, rep(1, 5),
                                               150, 0.8, 0.78), "correlation")
})

test_that("a callable note in the three-factor model", {
  today <- "2026-11-02"
  jpy <- create_flat_curve(today, 0.012, "actual_365_fixed", "continuous")
  usd <- create_flat_curve(today, 0.035, "actual_365_fixed", "continuous")
  call <- function(dates, price) compute_prdc_three_factor_value(158, 0.11, jpy, usd, c(0.03, 0.01), c(0.05, 0.012), c(0.3, -0.2, 0.4), 1:5,
                                                                 rep(1, 5), 150, 0.8, 0.78, 0, 0.08, paths = 5000L, seed = 3,
                                                                 call_dates = dates, call_price = price)
  r <- call(1:4, 1)
  expect_true(r$summary$hindsight_value <= r$summary$value && r$summary$value < r$summary$noncallable_value)
  expect_equal(nrow(r$calls), 4L)
  expect_lte(sum(r$calls$call_probability), 1)
  never <- call(1:4, 1e9)
  expect_equal(never$summary$value, never$summary$noncallable_value, tolerance = 1e-12)
  expect_error(call(5, 1), "before the last")
})

test_that("the three-factor closed form's derivatives", {
  today <- "2026-11-02"
  jpy <- create_flat_curve(today, 0.012, "actual_365_fixed", "continuous")
  usd <- create_flat_curve(today, 0.035, "actual_365_fixed", "continuous")
  value <- function(spot = 150, sx = 0.11, sd = 0.01, rho = c(0.3, -0.2, 0.4)) {
    compute_prdc_three_factor_value(spot, sx, jpy, usd, c(0.03, sd), c(0.05, 0.012), rho, 1:5, rep(1, 5), 150, 0.8, 0.78, 0, 0.08)
  }
  r <- value()
  d <- setNames(r$sensitivity$d_npv, r$sensitivity$parameter)
  expect_equal(d[["spot"]], (value(spot = 150.001)$summary$value - value(spot = 149.999)$summary$value) / 0.002, tolerance = 1e-5)
  expect_equal(d[["domestic_volatility"]], (value(sd = 0.010001)$summary$value - value(sd = 0.009999)$summary$value) / 2e-6, tolerance = 1e-5)
  expect_equal(d[["rho_foreign_fx"]], (value(rho = c(0.3, -0.2, 0.400001))$summary$value - value(rho = c(0.3, -0.2, 0.399999))$summary$value) / 2e-6,
               tolerance = 1e-5)
  # flat curves have no nodes; the nodes of other curves are checked against rebuilt curves in Rust
  expect_named(r$curve_sensitivity, c("curve", "date", "time", "d_npv", "npv_per_bp"))
})

test_that("a callable note's derivatives", {
  today <- "2026-11-02"
  jpy <- create_flat_curve(today, 0.012, "actual_365_fixed", "continuous")
  usd <- create_flat_curve(today, 0.035, "actual_365_fixed", "continuous")
  r <- compute_prdc_three_factor_value(158, 0.11, jpy, usd, c(0.03, 0.01), c(0.05, 0.012), c(0.3, -0.2, 0.4), 1:5, rep(1, 5), 150,
                                       0.8, 0.78, 0, 0.08, paths = 5000L, seed = 3, call_dates = 1:4, call_price = 1)
  expect_equal(r$call_sensitivity$parameter, r$sensitivity$parameter)
  expect_true(all(r$call_sensitivity$standard_error >= 0))
  # the call takes away the upside: the callable delta is below the note's
  expect_lt(r$call_sensitivity$d_npv[1], 0.7 * r$sensitivity$d_npv[1])
})

test_that("a target redemption note against an Euler simulation written here", {
  today <- "2026-11-02"
  rd <- 0.012; rf <- 0.035
  jpy <- create_flat_curve(today, rd, "actual_365_fixed", "continuous")
  usd <- create_flat_curve(today, rf, "actual_365_fixed", "continuous")
  a <- c(0.03, 0.05); s <- c(0.01, 0.012); sx <- 0.11; rho <- c(0.3, -0.2, 0.4); target <- 0.1
  r <- compute_prdc_three_factor_value(158, sx, jpy, usd, c(a[1], s[1]), c(a[2], s[2]), rho, 1:5, rep(1, 5), 150, 0.8, 0.78, 0, 0.08,
                                       paths = 20000L, seed = 3, target = target, target_coupon = "capped", make_whole = TRUE)
  expect_equal(nrow(r$endings), 5L)
  expect_lte(sum(r$endings$target_probability), 1)
  # Euler on (r_d, r_f, ln S), 100 steps a year, with the target rule on the coupon dates
  set.seed(20261108)
  n <- 20000L; dt <- 0.01
  phi <- function(r, a, s, t) r + s^2 / (2 * a^2) * (1 - exp(-a * t))^2
  L <- t(chol(matrix(c(1, rho[1], rho[2], rho[1], 1, rho[3], rho[2], rho[3], 1), 3)))
  xd <- xf <- numeric(n); lns <- rep(log(158), n); integral <- numeric(n); value <- numeric(n)
  total <- numeric(n); alive <- rep(TRUE, n)
  purrr::walk(seq_len(500L), \(k) {
    z <- matrix(rnorm(3 * n), n) %*% t(L)
    t0 <- (k - 1) * dt
    r_d <- xd + phi(rd, a[1], s[1], t0); r_f <- xf + phi(rf, a[2], s[2], t0)
    lns <<- lns + (r_d - r_f - sx^2 / 2) * dt + sx * sqrt(dt) * z[, 3]
    integral <<- integral + r_d * dt
    xd <<- xd - a[1] * xd * dt + s[1] * sqrt(dt) * z[, 1]
    xf <<- xf + (-a[2] * xf - rho[3] * s[2] * sx) * dt + s[2] * sqrt(dt) * z[, 2]
    if (k %% 100L == 0L) {
      rate <- pmin(pmax(0.8 * exp(lns) / 150 - 0.78, 0), 0.08)
      hit <- alive & total + rate >= target
      paid <- ifelse(hit, target - total + 1, rate)
      if (k == 500L) paid <- ifelse(alive & !hit, rate + 1 + pmax(target - total - rate, 0), paid)
      value <<- value + ifelse(alive, exp(-integral) * paid, 0)
      total <<- total + rate
      alive <<- alive & !hit
    }
  })
  euler <- mean(value); euler_se <- sd(value) / sqrt(n)
  expect_lt(abs(r$summary$value - euler), 4 * sqrt(r$summary$standard_error^2 + euler_se^2) + 2e-4)
  expect_error(compute_prdc_three_factor_value(158, sx, jpy, usd, c(a[1], s[1]), c(a[2], s[2]), rho, 1:5, rep(1, 5), 150, 0.8, 0.78,
                                               paths = 100L, target = 0.1, call_dates = 1:2), "no call dates")
})

test_that("a knock-out note against an Euler simulation written here", {
  today <- "2026-11-02"
  rd <- 0.012; rf <- 0.035
  jpy <- create_flat_curve(today, rd, "actual_365_fixed", "continuous")
  usd <- create_flat_curve(today, rf, "actual_365_fixed", "continuous")
  a <- c(0.03, 0.05); s <- c(0.01, 0.012); sx <- 0.11; rho <- c(0.3, -0.2, 0.4); level <- 155
  r <- compute_prdc_three_factor_value(150, sx, jpy, usd, c(a[1], s[1]), c(a[2], s[2]), rho, 1:5, rep(1, 5), 150, 0.8, 0.78, 0, 0.08,
                                       paths = 20000L, seed = 3, knock_out_level = level)
  expect_equal(r$endings$knock_out_probability[5], 0)
  expect_gt(sum(r$endings$knock_out_probability), 0)
  set.seed(20261109)
  n <- 20000L; dt <- 0.01
  phi <- function(r, a, s, t) r + s^2 / (2 * a^2) * (1 - exp(-a * t))^2
  L <- t(chol(matrix(c(1, rho[1], rho[2], rho[1], 1, rho[3], rho[2], rho[3], 1), 3)))
  xd <- xf <- numeric(n); lns <- rep(log(150), n); integral <- numeric(n); value <- numeric(n); alive <- rep(TRUE, n)
  purrr::walk(seq_len(500L), \(k) {
    z <- matrix(rnorm(3 * n), n) %*% t(L)
    t0 <- (k - 1) * dt
    r_d <- xd + phi(rd, a[1], s[1], t0); r_f <- xf + phi(rf, a[2], s[2], t0)
    lns <<- lns + (r_d - r_f - sx^2 / 2) * dt + sx * sqrt(dt) * z[, 3]
    integral <<- integral + r_d * dt
    xd <<- xd - a[1] * xd * dt + s[1] * sqrt(dt) * z[, 1]
    xf <<- xf + (-a[2] * xf - rho[3] * s[2] * sx) * dt + s[2] * sqrt(dt) * z[, 2]
    if (k %% 100L == 0L) {
      rate <- pmin(pmax(0.8 * exp(lns) / 150 - 0.78, 0), 0.08)
      out <- if (k == 500L) rep(TRUE, n) else exp(lns) >= level
      value <<- value + ifelse(alive, exp(-integral) * (rate + out), 0)
      alive <<- alive & !out
    }
  })
  euler <- mean(value); euler_se <- sd(value) / sqrt(n)
  expect_lt(abs(r$summary$value - euler), 4 * sqrt(r$summary$standard_error^2 + euler_se^2) + 2e-4)
})

test_that("the three-factor model is calibrated to swaptions and an FX option", {
  today <- "2026-11-02"
  jpy <- create_flat_curve(today, 0.012, "actual_365_fixed", "continuous")
  usd <- create_flat_curve(today, 0.035, "actual_365_fixed", "continuous")
  # swaption prices from known Hull-White parameters, fitted back
  quotes <- function(curve, r, a, sigma) {
    out <- purrr::map(c(0.5, 1, 2, 3), \(e) {
      p <- exp(-r * c(e, e + 1, e + 2))
      rate <- (p[1] - p[3]) / (p[2] + p[3])
      tibble::tibble(payer = TRUE, maturity = e, payment_times = list(c(e + 1, e + 2)), coupons = list(c(rate, rate)))
    }) |> purrr::list_rbind()
    out$price <- purrr::map_dbl(seq_len(nrow(out)), \(k) compute_hull_white_swaption_value(TRUE, out$maturity[k], out$payment_times[[k]], out$coupons[[k]], a, sigma, curve)$value)
    out
  }
  fit <- calibrate_prdc_three_factor(150, jpy, usd, quotes(jpy, 0.012, 0.03, 0.008), quotes(usd, 0.035, 0.06, 0.011), c(0.3, -0.2, 0.4),
                                     fx_options = tibble::tibble(maturity = 3, volatility = 0.12))
  expect_equal(fit$domestic_rate, c(0.03, 0.008), tolerance = 1e-5)
  expect_equal(fit$foreign_rate, c(0.06, 0.011), tolerance = 1e-5)
  expect_equal(fit$fx_fit$model_volatility, 0.12, tolerance = 1e-12)
  r <- compute_prdc_three_factor_value(150, fit$fx_volatility, jpy, usd, fit$domestic_rate, fit$foreign_rate, c(0.3, -0.2, 0.4), 3, 1, 150,
                                       0.8, 0.78)
  expect_equal(r$coupons$effective_volatility, 0.12, tolerance = 1e-12)
})
