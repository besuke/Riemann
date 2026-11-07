# Golden values for the FX smile, from the QuantLib R package Version 1.42: strikes from deltas, deltas
# from strikes and ATM strikes of BlackDeltaCalculator (spot, forward and premium-adjusted deltas), and
# the Black volatilities of BlackVolatilitySurfaceDelta (volatilities at put and call deltas and ATM by
# expiry; each delta's variance interpolated linearly in time, the deltas turned into strikes at the
# time asked, and the smile interpolated linearly in strike) for each delta type and ATM type, and a PRDC
# note on each surface by static replication (the coupons' call spreads by blackFormula at the surface's
# volatilities; kinds prdc_rate, with the coupon time and accrual, and prdc_npv). All inputs are synthetic; no market data is used.
# Usage (from the Riemann root): Rscript tools/golden/p10_fx_smile.R
# Writes tests/golden/fx_delta.csv (option, delta_type, spot, domestic_discount, foreign_discount,
# std_dev, kind, input, value) and tests/golden/fx_smile.csv (case, kind, i, j, value; the kind
# "setting" holds the case's conventions in i as text).

suppressMessages(library(QuantLib))

fmt <- function(x) sprintf("%.17g", x)

# ---- BlackDeltaCalculator ---------------------------------------------------------------------------------
delta_rows <- list()
add <- function(...) delta_rows[[length(delta_rows) + 1L]] <<- c(...)
for (dt in c("Spot", "Fwd", "PaSpot", "PaFwd")) {
  for (discounts in list(c(0.99, 0.96), c(0.97, 0.995))) {
    for (sd in c(0.05, 0.12, 0.3)) {
      for (ot in c("Call", "Put")) {
        b <- BlackDeltaCalculator(ot, dt, 150, discounts[1], discounts[2], sd)
        key <- c(ot, dt, "150", fmt(discounts[1]), fmt(discounts[2]), fmt(sd))
        sign <- if (ot == "Call") 1 else -1
        for (delta in c(0.1, 0.25, 0.4)) {
          add(key, "strike_from_delta", fmt(sign * delta), fmt(BlackDeltaCalculator_strikeFromDelta(b, sign * delta)))
        }
        for (strike in c(110, 135, 150, 165, 200)) {
          add(key, "delta_from_strike", fmt(strike), fmt(BlackDeltaCalculator_deltaFromStrike(b, strike)))
        }
        for (at in c("AtmSpot", "AtmFwd", "AtmDeltaNeutral", "AtmPutCall50")) {
          v <- tryCatch(BlackDeltaCalculator_atmStrike(b, at), error = function(e) NA_real_)
          if (!is.na(v)) add(key, paste0("atm_", at), "0", fmt(v))
        }
      }
    }
  }
}
delta_table <- do.call(rbind, delta_rows)
colnames(delta_table) <- c("option", "delta_type", "spot", "domestic_discount", "foreign_discount", "std_dev", "kind", "input", "value")
write.csv(delta_table, file.path("tests", "golden", "fx_delta.csv"), row.names = FALSE, quote = FALSE)

# ---- BlackVolatilitySurfaceDelta --------------------------------------------------------------------------
ref <- Date(2L, "November", 2026L)
invisible(Settings_instance()$setEvaluationDate(ref))
dc <- Actual365Fixed()
rd <- 0.012; rf <- 0.035; spot <- 150
domestic <- YieldTermStructureHandle(FlatForward(ref, QuoteHandle(SimpleQuote(rd)), dc, "Continuous"))
foreign <- YieldTermStructureHandle(FlatForward(ref, QuoteHandle(SimpleQuote(rf)), dc, "Continuous"))
expiry_dates <- c("2027-02-02", "2027-05-03", "2027-11-02", "2028-11-02", "2031-11-03")
expiries <- lapply(expiry_dates, DateParser_parseISO)
expiry_times <- vapply(expiries, function(d) DayCounter_yearFraction(dc, ref, d), numeric(1))
dates <- DateVector(); for (d in expiries) DateVector_append(dates, d)
# rows = expiries; columns = 10P, 25P, ATM, 25C, 10C
vols <- rbind(c(0.142, 0.124, 0.110, 0.113, 0.128),
              c(0.145, 0.126, 0.112, 0.115, 0.131),
              c(0.150, 0.130, 0.115, 0.118, 0.135),
              c(0.156, 0.134, 0.118, 0.121, 0.140),
              c(0.161, 0.138, 0.121, 0.124, 0.146))
as_matrix <- function(v) {
  m <- Matrix(nrow(v), ncol(v))
  for (i in seq_len(nrow(v))) for (j in seq_len(ncol(v))) Matrix_setitem(m, i - 1L, j - 1L, v[i, j])
  m
}
# The binding cannot pass the optional ATM delta type, so only the constructor with the defaults after
# the ATM type is used: the ATM delta type is the delta type, the smile is linear in strike with the
# interpolation's own extrapolation, the volatility is flat in time after the last expiry, and there is no
# switch to long-term conventions (the switch is checked in Rust by joining two of these surfaces).
cases <- list(
  list(name = "spot_dn", dt = "Spot", at = "AtmDeltaNeutral", atm = TRUE),
  list(name = "paspot_dn", dt = "PaSpot", at = "AtmDeltaNeutral", atm = TRUE),
  list(name = "fwd_atmfwd", dt = "Fwd", at = "AtmFwd", atm = TRUE),
  list(name = "pafwd_dn", dt = "PaFwd", at = "AtmDeltaNeutral", atm = TRUE),
  list(name = "fwd_putcall50", dt = "Fwd", at = "AtmPutCall50", atm = TRUE),
  list(name = "spot_atmspot", dt = "Spot", at = "AtmSpot", atm = TRUE),
  list(name = "spot_no_atm", dt = "Spot", at = "AtmDeltaNeutral", atm = FALSE)
)
query_times <- c(0.1, expiry_times[1], 0.4, expiry_times[2], 0.75, expiry_times[3], 1.5, expiry_times[4], 3.2, expiry_times[5], 6.5, 8)
query_strikes <- c(100, 115, 125, 132, 138, 143, 147, 150, 153, 158, 165, 175, 190, 210)
smile_rows <- list()
put <- function(case, kind, i, j, value) smile_rows[[length(smile_rows) + 1L]] <<- c(case, kind, i, j, value)
for (cs in cases) {
  columns <- if (cs$atm) 1:5 else c(1, 2, 4, 5)
  v <- vols[, columns, drop = FALSE]
  surface <- BlackVolatilitySurfaceDelta(ref, dates, c(-0.1, -0.25), c(0.25, 0.1), cs$atm, as_matrix(v), dc, NullCalendar(),
                                         QuoteHandle(SimpleQuote(spot)), domestic, foreign, cs$dt, cs$at)
  settings <- c(delta_type = cs$dt, atm_type = cs$at, has_atm = as.character(cs$atm))
  for (k in names(settings)) put(cs$name, "setting", k, settings[[k]], "0")
  put(cs$name, "market", "spot", "0", fmt(spot))
  put(cs$name, "market", "domestic_rate", "0", fmt(rd))
  put(cs$name, "market", "foreign_rate", "0", fmt(rf))
  for (i in seq_along(expiry_times)) put(cs$name, "expiry", fmt(i - 1), "0", fmt(expiry_times[i]))
  for (i in seq_len(nrow(v))) for (j in seq_len(ncol(v))) put(cs$name, "vol", fmt(i - 1), fmt(j - 1), fmt(v[i, j]))
  for (t in query_times) for (k in query_strikes) {
    put(cs$name, "black_vol", fmt(t), fmt(k), fmt(BlackVolTermStructure_blackVol(surface, t, k, TRUE)))
  }
  # a PRDC note priced by static replication on the smile: each coupon clamp(0.8 S / 150 - 0.78, 0, 8%)
  # is 0.8 / 150 times the call spread between the strikes of the floor and the cap, the calls by
  # blackFormula at the surface's volatilities, undiscounted, on the forward of the curves
  coupon_times <- c(0.5, 1, 2, 3, 4.5)
  accruals <- diff(c(0, coupon_times))
  scale <- 0.8 / 150
  strikes <- c((0.78 + 0) / scale, (0.78 + 0.08) / scale)
  npv <- 0
  for (k in seq_along(coupon_times)) {
    t <- coupon_times[k]
    pd <- domestic$discount(t)
    pf <- foreign$discount(t)
    forward <- spot * pf / pd
    vols_at <- vapply(strikes, function(x) BlackVolTermStructure_blackVol(surface, t, x, TRUE), numeric(1))
    calls <- vapply(1:2, function(m) blackFormula("Call", strikes[m], forward, vols_at[m] * sqrt(t), 1), numeric(1))
    rate <- 0 + scale * (calls[1] - calls[2])
    put(cs$name, "prdc_rate", fmt(t), fmt(accruals[k]), fmt(rate))
    npv <- npv + rate * pd * accruals[k]
  }
  npv <- npv + domestic$discount(coupon_times[length(coupon_times)])
  put(cs$name, "prdc_npv", "0", "0", fmt(npv))
}
smile_table <- do.call(rbind, smile_rows)
colnames(smile_table) <- c("case", "kind", "i", "j", "value")
write.csv(smile_table, file.path("tests", "golden", "fx_smile.csv"), row.names = FALSE, quote = FALSE)
cat("fx_delta.csv:", nrow(delta_table), "rows; fx_smile.csv:", nrow(smile_table), "rows\n")
