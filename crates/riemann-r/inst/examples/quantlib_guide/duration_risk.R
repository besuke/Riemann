# QuantLib Guide: assessing duration risk
#
# What it computes: the example of "Assessing duration risk" (https://www.quantlibguide.com/): a US
# Treasury curve bootstrapped from twelve par yields with bond helpers, linear in zero rates; the clean
# prices of eighteen bonds on it; and the change of each price under five moves: +1 bp on each bond's
# yield, +1 bp on the zero rates, a flattening and a steepening of the zero rates (+-0.2% at the
# evaluation date, -+0.2% at 60 years, linear in between), and the curve of the par yields a year later.
# RiemannFinMath's Rust functions do the calculation. The table compares them with the Guide's printed
# values and, when the QuantLib R package is installed, with the same calls to the local QuantLib.
#
# Run:   source(system.file("examples", "quantlib_guide", "duration_risk.R", package = "RiemannFinMath"))
# Needs: RiemannFinMath, purrr and tibble. QuantLib (the R package) is optional.
#
# Conditions: evaluation date 17 March 2022. Each par yield is the coupon of a bond priced at 100 (clean)
# from the evaluation date to the tenor (1, 2, 3 and 6 months, 1, 2, 3, 5, 7, 10, 20 and 30 years);
# semiannual coupons, unadjusted dates, backward generation, 30/360 (bond basis), 1 settlement day, US
# government bond calendar, payments Following. The curve is linear in continuous zero rates on Actual/360,
# with extrapolation. The eighteen bonds have the same conventions. The spreads are continuous zero rates.
#
# Every value equals QuantLib 1.42 (within 1e-13). The Guide prints 6 decimals, and every value is within
# that rounding of it except the +1 bp yield moves, which differ by up to 1.3e-5. Those use the yield that
# QuantLib's Bond.yield solves with its default accuracy of 1e-8, which this script passes too; with an
# accuracy of 1e-10 they agree with the Guide within 5.3e-7 and differ from QuantLib 1.42 by up to 1.3e-5.
# The Guide's yields were solved more precisely than 1.42's default; why was not traced (a difference of
# QuantLib versions).
#
# Not reproduced (docs/quantlib_guide_inventory.md): nothing on this page.

library(RiemannFinMath)
has_quantlib <- requireNamespace("QuantLib", quietly = TRUE)
today <- as.Date("2022-03-17")
tenors <- c("1M", "2M", "3M", "6M", "1Y", "2Y", "3Y", "5Y", "7Y", "10Y", "20Y", "30Y")
yields <- c(0.20, 0.30, 0.40, 0.81, 1.30, 1.94, 2.14, 2.17, 2.22, 2.20, 2.60, 2.50)
new_yields <- c(4.31, 4.51, 4.52, 4.71, 4.26, 3.81, 3.68, 3.44, 3.45, 3.39, 3.76, 3.60)
after <- function(d, tenor) seq(d, by = sub("M$", " months", sub("Y$", " years", tenor)), length.out = 2)[2]
treasury <- function(start, end, coupon) {
  create_fixed_rate_bond(1L, 100, start, end, "6M", "us_government_bond", "unadjusted", "unadjusted", "backward", FALSE, coupon,
                         "thirty_360_bond_basis", "following", 100)
}
par_curve <- function(y) {
  helpers <- purrr::map2(tenors, y, \(tenor, rate) treasury(today, after(today, tenor), rate / 100))
  with_extrapolation(bootstrap_bond_curve(today, helpers, rep(100, length(tenors)), "clean", "actual_360", "linear_zero"))
}
portfolio <- data.frame(
  start = as.Date(c("2016-09-15", "2017-11-01", "2021-08-01", "2008-12-01", "2020-06-01", "2009-11-01", "2016-04-01", "2012-09-15", "2012-03-01",
                    "2017-02-01", "2007-08-01", "2006-09-15", "2018-09-01", "2013-03-01", "2012-09-01", "2007-09-01", "2016-03-01", "2020-03-01")),
  end = as.Date(c("2022-09-15", "2022-11-01", "2023-08-01", "2024-12-01", "2027-06-01", "2029-11-01", "2030-04-01", "2032-09-15", "2035-03-01",
                  "2037-02-01", "2039-08-01", "2041-09-15", "2044-09-01", "2048-03-01", "2049-09-01", "2051-09-01", "2067-03-01", "2072-03-01")),
  coupon = c(1.45, 5.5, 4.75, 2.5, 2.2, 5.25, 1.35, 1.33, 3.35, 4.0, 3.0, 2.97, 3.75, 3.45, 3.85, 1.7, 2.8, 2.15)
)
bonds <- purrr::pmap(portfolio, \(start, end, coupon) treasury(start, end, coupon / 100))
prices_on <- function(curve) purrr::map_dbl(bonds, \(b) compute_bond_value_on_curve(b, curve, today)$clean_price)

curve <- par_curve(yields)
price <- prices_on(curve)
tb <- "thirty_360_bond_basis"
yield_bp <- purrr::map2_dbl(bonds, price, \(b, p) {
  s <- compute_settlement_date(b, today)
  # accuracy 1e-8: the default of QuantLib's Bond.yield, which the Guide calls
  y <- compute_yield_from_price(b, p, "clean", tb, "compounded", 2L, s, accuracy = 1e-8)
  compute_clean_price(b, y + 1e-4, tb, "compounded", 2L, s) - p
})
zero_bp <- prices_on(create_zero_spreaded_curve(curve, 1e-4)) - price
tilted <- function(short, long) create_zero_spreaded_curve(curve, c(short, long), spread_dates = c(today, after(today, "60Y")))
flattening <- prices_on(tilted(0.002, -0.002)) - price
steepening <- prices_on(tilted(-0.002, 0.002)) - price
year_later <- prices_on(par_curve(new_yields)) - price
riemann <- cbind(price, yield_bp, zero_bp, flattening, steepening, year_later)

guide <- cbind(
  price = c(100.325987, 102.821079, 104.341431, 101.086154, 100.118562, 121.230541, 93.664362, 91.673963, 111.512720, 120.086115, 107.116189,
            106.031770, 120.219158, 117.343050, 126.277776, 83.218728, 111.000187, 93.543566),
  yield_bp = c(-0.004914, -0.006333, -0.013913, -0.026350, -0.048968, -0.078070, -0.070619, -0.088712, -0.118580, -0.138879, -0.146023, -0.157860,
               -0.190583, -0.209384, -0.229305, -0.185221, -0.294067, -0.278227),
  zero_bp = c(-0.005045, -0.006505, -0.014248, -0.027058, -0.050229, -0.080078, -0.072447, -0.091026, -0.121531, -0.142264, -0.149421, -0.161291,
              -0.194415, -0.213709, -0.234061, -0.189618, -0.302897, -0.287335),
  flattening = c(-0.099161, -0.127298, -0.271664, -0.491554, -0.830087, -1.212212, -1.061867, -1.191191, -1.442707, -1.549767, -1.394847,
                 -1.319961, -1.339476, -1.072036, -1.041110, -0.411631, 1.375404, 2.220772),
  steepening = c(0.099260, 0.127456, 0.272379, 0.493991, 0.837225, 1.225468, 1.074386, 1.207413, 1.463129, 1.571870, 1.414609, 1.337761,
                 1.355756, 1.083442, 1.052151, 0.416085, -1.286171, -2.052258),
  year_later = c(-1.866123, -2.264614, -3.485236, -4.201212, -5.962030, -9.219825, -8.097490, -9.836337, -13.110537, -15.242371, -15.746457,
                 -16.829242, -19.800276, -20.953544, -22.683295, -17.548003, -25.273680, -23.000832)
)

quantlib <- matrix(NA_real_, nrow(guide), ncol(guide), dimnames = dimnames(guide))
if (has_quantlib) {
  ql_date <- function(x) QuantLib::Date(as.integer(as.Date(x)) + 25569L)
  invisible(QuantLib::Settings_instance()$setEvaluationDate(ql_date(today)))
  calendar <- QuantLib::UnitedStates("GovernmentBond")
  day_count <- QuantLib::Thirty360("BondBasis")
  schedule <- function(start, end) {
    QuantLib::Schedule(ql_date(start), ql_date(end), QuantLib::Period("Semiannual"), calendar, "Unadjusted", "Unadjusted", "Backward", FALSE)
  }
  ql_par_curve <- function(y) {
    helpers <- QuantLib::RateHelperVector()
    purrr::walk2(tenors, y, \(tenor, rate) {
      helpers$append(QuantLib::FixedRateBondHelper(QuantLib::QuoteHandle(QuantLib::SimpleQuote(100)), 1L, 100, schedule(today, after(today, tenor)),
                                                   rate / 100, day_count))
    })
    c <- QuantLib::PiecewiseLinearZero(ql_date(today), helpers, QuantLib::Actual360())
    c$enableExtrapolation()
    c
  }
  handle <- QuantLib::RelinkableYieldTermStructureHandle()
  engine <- QuantLib::DiscountingBondEngine(handle)
  ql_bonds <- purrr::pmap(portfolio, \(start, end, coupon) {
    b <- QuantLib::FixedRateBond(1L, 100, schedule(start, end), coupon / 100, day_count)
    invisible(QuantLib::Instrument_setPricingEngine(b, engine))
    b
  })
  ql_prices <- function(c) {
    invisible(QuantLib::RelinkableYieldTermStructureHandle_linkTo(handle, c))
    purrr::map_dbl(ql_bonds, QuantLib::Bond_cleanPrice)
  }
  ql_tilted <- function(short, long) {
    dates <- QuantLib::DateVector()
    purrr::walk(list(today, after(today, "60Y")), \(d) dates$append(ql_date(d)))
    spreads <- QuantLib::QuoteHandleVector()
    purrr::walk(c(short, long), \(s) spreads$append(QuantLib::QuoteHandle(QuantLib::SimpleQuote(s))))
    c <- QuantLib::PiecewiseZeroSpreadedTermStructure(QuantLib::YieldTermStructureHandle(ql_curve), spreads, dates)
    c$enableExtrapolation()
    c
  }
  ql_curve <- ql_par_curve(yields)
  q_price <- ql_prices(ql_curve)
  q_yield_bp <- purrr::map_dbl(ql_bonds, \(b) {
    y <- QuantLib::Bond_yield(b, day_count, "Compounded", "Semiannual")
    QuantLib::Bond_cleanPrice(b, y + 1e-4, day_count, "Compounded", "Semiannual") - QuantLib::Bond_cleanPrice(b)
  })
  quantlib <- cbind(
    price = q_price,
    yield_bp = q_yield_bp,
    zero_bp = ql_prices(QuantLib::ZeroSpreadedTermStructure(QuantLib::YieldTermStructureHandle(ql_curve), QuantLib::QuoteHandle(QuantLib::SimpleQuote(1e-4)))) - q_price,
    flattening = ql_prices(ql_tilted(0.002, -0.002)) - q_price,
    steepening = ql_prices(ql_tilted(-0.002, 0.002)) - q_price,
    year_later = ql_prices(ql_par_curve(new_yields)) - q_price
  )
}

results <- purrr::map(colnames(guide), \(name) {
  g <- guide[, name]
  r <- riemann[, name]
  tibble::tibble(item = name, maturity = portfolio$end, guide = g, quantlib_local = quantlib[, name], riemann = r, riemann_minus_guide = r - g)
}) |>
  purrr::list_rbind()
cat("QuantLib Guide, assessing duration risk\n")
cat("QuantLib R package:", if (has_quantlib) as.character(utils::packageVersion("QuantLib")) else "not installed", "\n")
cat("The curve reprices its 12 par bonds within", format(max(abs(curve$repricing$implied_price - 100)), digits = 2), "\n\n")
old <- options(width = 200)
print(as.data.frame(results), digits = 10, row.names = FALSE)
options(old)
exact <- results$item != "yield_bp"
cat("\nLargest difference from the Guide's 6-decimal prints:", format(max(abs(results$riemann_minus_guide[exact])), digits = 2),
    "(the +1 bp yield moves:", format(max(abs(results$riemann_minus_guide[!exact])), digits = 2), "; see the header)")
if (has_quantlib) cat("; from QuantLib 1.42:", format(max(abs(results$riemann - results$quantlib_local)), digits = 2))
cat("\n")
invisible(results)
