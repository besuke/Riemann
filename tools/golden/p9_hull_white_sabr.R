# Golden values for Riemann P9: Hull-White from the QuantLib R package, the callable bond of
# LagrangeFinance II-8 (its own R functions, read from the chapter without changing it) and SABR.
# All inputs are synthetic; no market data is used.
# Usage (from the Riemann root): Rscript tools/golden/p9_hull_white_sabr.R
# Writes tests/golden/hw_{curve,bond_option,swaption,bermudan,callable}.csv and sabr_volatility.csv.

suppressMessages(library(QuantLib))

out_dir <- file.path("tests", "golden")
set.seed(20261104)
fmt <- function(x) sprintf("%.17g", x)
day_count <- QuantLib::Actual365Fixed()
calendar <- QuantLib::NullCalendar()
today <- QuantLib::Date(2L, "November", 2026L)
invisible(QuantLib::Settings_instance()$setEvaluationDate(today))
today_serial <- as.integer(QuantLib::Date_serialNumber(today))
at_days <- function(days) QuantLib::Date(today_serial + as.integer(days))
header <- function(path, columns) {
  writeLines(c(paste0("# QuantLib R package ", packageVersion("QuantLib"), "; synthetic inputs; tools/golden/p9_hull_white_sabr.R"),
               paste(columns, collapse = ",")), path)
}
add <- function(path, values) write(paste(values, collapse = ","), path, append = TRUE)

# A curve with log-linear discount factors: shared with Riemann through this file
node_days <- as.integer(c(0, 365, 730, 1095, 1825, 2555, 3650, 5475, 7300, 10950))
node_discounts <- exp(-cumsum(c(0, 0.02, 0.022, 0.025, 2 * 0.027, 2 * 0.03, 3 * 0.032, 5 * 0.033, 5 * 0.031, 10 * 0.03)))
path <- file.path(out_dir, "hw_curve.csv")
header(path, c("days", "discount"))
for (i in seq_along(node_days)) add(path, c(node_days[i], fmt(node_discounts[i])))
dates <- do.call(QuantLib::DateVector, list())
for (d in node_days) QuantLib::DateVector_push_back(dates, at_days(d))
curve <- QuantLib::DiscountCurve(dates, node_discounts, day_count)
handle <- QuantLib::YieldTermStructureHandle(curve)

# Options on zero-coupon bonds: reached through a swaption with one payment, the only public route
# Swaptions by Jamshidian's decomposition
schedule_of <- function(start_days, years) {
  QuantLib::Schedule(at_days(start_days), at_days(start_days + 365L * years), QuantLib::Period("Annual"), calendar,
                     "Unadjusted", "Unadjusted", "Backward", FALSE)
}
# The days of a schedule from today: the annual periods are calendar years, not 365 days
schedule_days <- function(schedule) {
  vapply(seq_len(QuantLib::Schedule_size(schedule)) - 1L, function(i) as.integer(QuantLib::Date_serialNumber(QuantLib::Schedule_date(schedule, i))) - today_serial, 0L)
}
index <- QuantLib::IborIndex("Synthetic1Y", QuantLib::Period("Annual"), 0L, QuantLib::JPYCurrency(), calendar, "Unadjusted", FALSE, day_count, handle)
path <- file.path(out_dir, "hw_swaption.csv")
header(path, c("payer", "days", "fixed_rate", "a", "sigma", "value"))
for (i in 1:200) {
  payer <- sample(c(TRUE, FALSE), 1)
  start_days <- 365L * sample(1:8, 1)
  years <- sample(c(1L, 2L, 3L, 5L, 7L, 10L), 1)
  fixed_rate <- runif(1, 0.005, 0.06)
  a <- runif(1, 0.01, 0.4)
  sigma <- runif(1, 0.002, 0.02)
  model <- QuantLib::HullWhite(handle, a, sigma)
  swap <- QuantLib::VanillaSwap(if (payer) QuantLib::Swap_Payer_get() else QuantLib::Swap_Receiver_get(), 1, schedule_of(start_days, years), fixed_rate, day_count,
                                schedule_of(start_days, years), index, 0, day_count)
  swaption <- QuantLib::Swaption(swap, QuantLib::EuropeanExercise(at_days(start_days)))
  QuantLib::Instrument_setPricingEngine(swaption, QuantLib::JamshidianSwaptionEngine(model))
  add(path, c(if (payer) 1L else 0L, paste(schedule_days(schedule_of(start_days, years)), collapse = ";"), fmt(c(fixed_rate, a, sigma, swaption$NPV()))))
}

# Bermudan swaptions on QuantLib's tree, which converges with the steps: compared within a tolerance
path <- file.path(out_dir, "hw_bermudan.csv")
header(path, c("payer", "days", "fixed_rate", "a", "sigma", "tree_value_500", "tree_value_1000"))
for (i in 1:20) {
  payer <- sample(c(TRUE, FALSE), 1)
  start_days <- 365L * sample(1:3, 1)
  years <- sample(c(3L, 5L), 1)
  fixed_rate <- runif(1, 0.015, 0.05)
  a <- runif(1, 0.03, 0.3)
  sigma <- runif(1, 0.004, 0.015)
  model <- QuantLib::HullWhite(handle, a, sigma)
  swap <- QuantLib::VanillaSwap(if (payer) QuantLib::Swap_Payer_get() else QuantLib::Swap_Receiver_get(), 1, schedule_of(start_days, years), fixed_rate, day_count,
                                schedule_of(start_days, years), index, 0, day_count)
  exercise_dates <- do.call(QuantLib::DateVector, list())
  days <- schedule_days(schedule_of(start_days, years))
  for (d in days[-length(days)]) QuantLib::DateVector_push_back(exercise_dates, at_days(d))
  swaption <- QuantLib::Swaption(swap, QuantLib::BermudanExercise(exercise_dates))
  values <- vapply(c(500L, 1000L), function(steps) {
    QuantLib::Instrument_setPricingEngine(swaption, QuantLib::TreeSwaptionEngine(model, steps))
    swaption$NPV()
  }, 0)
  add(path, c(if (payer) 1L else 0L, paste(days, collapse = ";"), fmt(c(fixed_rate, a, sigma, values))))
}

# SABR
path <- file.path(out_dir, "sabr_volatility.csv")
header(path, c("kind", "strike", "forward", "expiry", "alpha", "beta", "nu", "rho", "volatility"))
for (i in 1:400) {
  forward <- runif(1, 0.005, 0.08)
  strike <- forward * exp(rnorm(1, 0, 0.3))
  if (i %% 20 == 0) strike <- forward
  expiry <- runif(1, 0.1, 15)
  alpha <- runif(1, 0.005, 0.2)
  beta <- runif(1, 0, 1)
  nu <- runif(1, 0.01, 1)
  rho <- runif(1, -0.9, 0.9)
  add(path, c("lognormal", fmt(c(strike, forward, expiry, alpha, beta, nu, rho, QuantLib::sabrVolatility(strike, forward, expiry, alpha, beta, nu, rho)))))
}

# The callable bond of LagrangeFinance II-8: its functions, read from the chapter
chapter <- readLines("../LagrangeFinance/part2_quantive_riemann/ch08_early_redemption_models.qmd", encoding = "UTF-8")
chunk <- function(label, stop_at = NULL) {
  start <- which(chapter == paste0("#| label: ", label))
  end <- start + which(startsWith(chapter[(start + 1):length(chapter)], "```"))[1]
  body <- chapter[(start + 1):(end - 1)]
  if (!is.null(stop_at)) body <- body[seq_len(which(startsWith(body, stop_at))[1] - 1L)]
  paste(body, collapse = "\n")
}
environment_lf <- new.env()
eval(parse(text = chunk("hull-white-conditional-discount")), environment_lf)
eval(parse(text = chunk("callable-hull-white-backward", "model_result <-")), environment_lf)
contract <- function(coupon_rate, call_price, first_call, maturity) {
  times <- seq(0.5, maturity, by = 0.5)
  tibble::tibble(
    payment_time = times, coupon = 100 * coupon_rate * 0.5, principal_at_maturity = ifelse(times == maturity, 100, 0),
    issuer_can_call = times >= first_call & times < maturity, call_principal = ifelse(times >= first_call & times < maturity, call_price, NA_real_)
  )
}
path <- file.path(out_dir, "hw_callable.csv")
writeLines(c("# LagrangeFinance II-8 compute_callable_bond_LRL; synthetic inputs; tools/golden/p9_hull_white_sabr.R",
             paste(c("coupon_rate", "call_price", "first_call", "maturity", "flat_rate", "a", "sigma", "points", "limit", "quadrature", "bullet", "callable"), collapse = ",")), path)
cases <- list(list(0.03, 100, 1, 5, 0.02, 0.15, 0.012, 321L, 0.13, 24L), list(0.03, 100, 1, 5, 0.02, 0.15, 0.012, 161L, 0.13, 16L))
for (i in 1:18) {
  cases[[length(cases) + 1L]] <- list(round(runif(1, 0.01, 0.06), 3), sample(c(100, 101), 1), sample(c(1, 2), 1), sample(c(3, 5, 7), 1), round(runif(1, 0.005, 0.05), 3),
                                       round(runif(1, 0.05, 0.3), 3), round(runif(1, 0.005, 0.02), 4), 321L, 0.13, 24L)
}
for (x in cases) {
  v <- environment_lf$compute_callable_bond_LRL(contract(x[[1]], x[[2]], x[[3]], x[[4]]), flat_rate = x[[5]], mean_reversion = x[[6]], volatility = x[[7]],
                                                number_of_grid_points = x[[8]], grid_limit = x[[9]], quadrature_nodes = x[[10]])
  add(path, c(fmt(unlist(x[1:7])), x[[8]], fmt(x[[9]]), x[[10]], fmt(c(v$bullet_price, v$callable_price))))
}
