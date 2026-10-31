# Golden values for Riemann P2-4 (Japanese simple yield, Japanese compound yield and their
# risk measures) from GaussQuant's bond_*_YEN_GQL functions.
# Usage (from the Riemann root): Rscript tools/golden/p2_yen_bonds.R
# Writes tests/golden/yen_bonds.csv. Numbers have 17 significant digits; ERROR = GaussQuant
# raised an error.
#
# Remaining years: GaussQuant (the definition that Riemann adopts) switches to
# the anniversary rule (leap days not counted) when the calendar years differ by more than
# one. yf_market is a second-hand description of market practice that switches when the
# remaining period is one year or more; it is kept only for reference, and rule_differs = 1
# marks rows where the two differ. All price and yield columns are computed by GaussQuant
# with its own remaining years (yf_gaussquant).

suppressMessages(library(GaussQuant))

out_dir <- file.path("tests", "golden")
gq_version <- as.character(packageVersion("GaussQuant"))
set.seed(20261104)

fmt <- function(x) sprintf("%.17g", x)
num <- function(expr) tryCatch(fmt(expr), error = function(e) "ERROR")
serial <- function(d) as.integer(as.numeric(d) + 25569) # QuantLib serial number
add_years <- function(d, n) lubridate::`%m+%`(d, lubridate::years(n))

# The market-practice rule written directly: anniversary rule for one year or more.
yf_market <- function(start, end) {
  if (end >= add_years(start, 1L)) {
    day28 <- function(d) if (lubridate::month(d) == 2L && lubridate::day(d) == 29L) 28L else lubridate::day(d)
    ref_start <- lubridate::make_date(1990L, lubridate::month(start), day28(start))
    ref_end <- lubridate::make_date(1991L, lubridate::month(end), day28(end))
    t <- as.numeric(ref_end - ref_start) / 365 + (lubridate::year(end) - lubridate::year(start)) - 1
  } else {
    t <- as.numeric(end - start) / 365
  }
  trunc(t * 1e7) / 1e7
}

starts <- c(as.Date(c("2022-04-01", "2024-02-29", "2023-02-28", "2023-03-01", "2023-12-31", "2020-01-31")),
            as.Date("2019-01-01") + sample(0:4000, 194))
rows <- list()
for (k in seq_len(1200)) {
  start <- sample(starts, 1)
  end <- if (runif(1) < 0.25) {
    start + sample(1:800, 1) # around one year, where the two rules can differ
  } else if (runif(1) < 0.1) {
    add_years(start, sample(1:40, 1)) # exact anniversaries
  } else {
    start + sample(1:(40 * 366), 1)
  }
  if (runif(1) < 0.02) end <- start # zero remaining time
  coupon <- sample(c(0, 0.1, 0.8, 2.25, 4, 7.5), 1)
  redemption <- sample(c(100, 100, 101.5), 1)
  simple_rate <- sample(c(-0.3, 0, 0.05, 0.9, 2.5, 6.515, 12), 1)
  compound_yield <- sample(c(-0.4, 0, 0.0, 0.7, 3.2, 6.515), 1)
  cf <- sample(c(1, 2, 2, 4), 1)
  mf <- sample(c(1, 2, 2, 4, 12), 1)
  accrued <- sample(c(0, 0.123, 1.75), 1)
  sd <- as.character(start); ed <- as.character(end)
  yf <- num(bond_year_fraction_YEN_GQL(sd, ed))
  market <- fmt(yf_market(start, end))
  price_s <- num(bond_price_from_simple_rate_YEN_GQL(sd, ed, coupon, simple_rate, redemption, digits = NULL))
  price_s3 <- num(bond_price_from_simple_rate_YEN_GQL(sd, ed, coupon, simple_rate, redemption))
  # prices to solve from: the truncated price (as in LagrangeFinance II-1), shifted at random
  target <- if (price_s3 == "ERROR") 98.765 else as.numeric(price_s3) + sample(c(0, 0, -1.25, 2.5), 1)
  rate_s <- num(bond_simple_rate_from_price_YEN_GQL(sd, ed, coupon, target, redemption))
  price_c <- num(bond_price_from_compound_yield_YEN_GQL(sd, ed, coupon, compound_yield, redemption, cf, mf, digits = NULL))
  price_c3 <- num(bond_price_from_compound_yield_YEN_GQL(sd, ed, coupon, compound_yield, redemption, cf, mf))
  rate_c <- num(bond_compound_yield_from_price_YEN_GQL(sd, ed, coupon, target, redemption, cf, mf))
  risk <- function(f) tryCatch(f(), error = function(e) NULL)
  rs <- risk(function() bond_risk_from_simple_rate_YEN_GQL(sd, ed, coupon, simple_rate, redemption, accrued))
  rc <- risk(function() bond_risk_from_compound_yield_YEN_GQL(sd, ed, coupon, compound_yield, redemption, cf, mf, accrued))
  risk_cols <- function(r) if (is.null(r)) rep("ERROR", 6) else
    fmt(c(r$price, r$dP_dY_pct, r$d2P_dY2_pct, r$bpv_yen, r$modified_duration, r$convexity))
  rows[[k]] <- c(case = k, settlement = serial(start), maturity = serial(end), coupon = fmt(coupon),
                 redemption = fmt(redemption), simple_rate = fmt(simple_rate), compound_yield = fmt(compound_yield),
                 coupon_frequency = cf, compound_frequency = mf, accrued = fmt(accrued), target_price = fmt(target),
                 yf_gaussquant = yf, yf_market = market, rule_differs = as.integer(yf != "ERROR" && yf != market),
                 price_simple = price_s, price_simple_3 = price_s3, rate_from_price_simple = rate_s,
                 price_compound = price_c, price_compound_3 = price_c3, rate_from_price_compound = rate_c,
                 setNames(risk_cols(rs), paste0("risk_s_", c("price", "d1", "d2", "bpv_yen", "mod_dur", "convexity"))),
                 setNames(risk_cols(rc), paste0("risk_c_", c("price", "d1", "d2", "bpv_yen", "mod_dur", "convexity"))))
}
df <- as.data.frame(do.call(rbind, rows))
path <- file.path(out_dir, "yen_bonds.csv")
writeLines(c(paste0("# GaussQuant ", gq_version, " (commit c08fdb1) bond_*_YEN_GQL; dates are QuantLib serial numbers; ",
                    "prices and yields use yf_gaussquant; yf_market is the market-practice rule (see the script)"),
             paste(names(df), collapse = ",")), path)
utils::write.table(df, path, sep = ",", row.names = FALSE, col.names = FALSE, append = TRUE, quote = FALSE)
cat("cases:", nrow(df), "; rule differs:", sum(df$rule_differs == "1"),
    "; errors: price", sum(df$price_simple == "ERROR"), "simple rate", sum(df$rate_from_price_simple == "ERROR"),
    "compound price", sum(df$price_compound == "ERROR"), "compound yield", sum(df$rate_from_price_compound == "ERROR"),
    "risk s", sum(df$risk_s_price == "ERROR"), "risk c", sum(df$risk_c_price == "ERROR"), "\n")
