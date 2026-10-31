# Golden values for Riemann P2-2 (bond prices, yields and yield risk) from the QuantLib R binding.
# Usage (from the Riemann root): Rscript tools/golden/p2_bond_yields.R
# Reads tests/golden/bond_cases.csv (P2-1) and writes tests/golden/bond_yields.csv.
# Numbers are printed with 17 significant digits; ERROR = QuantLib raised an error.

suppressMessages(library(QuantLib))

out_dir <- file.path("tests", "golden")
ql_version <- as.character(packageVersion("QuantLib"))
set.seed(20261102)

calendars <- list(japan = QuantLib::Japan(), target = QuantLib::TARGET(),
                  us_sofr = QuantLib::UnitedStates("SOFR"), null_calendar = QuantLib::NullCalendar())
bond_day_counters <- list(actual_365_fixed = QuantLib::Actual365Fixed(), actual_360 = QuantLib::Actual360(),
                          thirty_360_bond_basis = QuantLib::Thirty360("BondBasis"),
                          act_act_isma = QuantLib::ActualActual("ISMA"), act_act_isda = QuantLib::ActualActual("ISDA"))
yield_day_counters <- list(actual_365_fixed = QuantLib::Actual365Fixed(), actual_360 = QuantLib::Actual360(),
                           thirty_360_bond_basis = QuantLib::Thirty360("BondBasis"),
                           act_act_isma = QuantLib::ActualActual("ISMA"))
frequencies <- c(Annual = 1L, Semiannual = 2L, Quarterly = 4L, Monthly = 12L)

from_serial <- function(n) QuantLib::Date(as.integer(n))
s <- function(x) as.integer(QuantLib::Date_serialNumber(x))
fmt <- function(x) sprintf("%.17g", x)
num <- function(expr) tryCatch(fmt(expr), error = function(e) "ERROR")

read_golden <- function(file) {
  lines <- readLines(file.path(out_dir, file))
  utils::read.csv(text = lines[!startsWith(lines, "#")], colClasses = "character")
}
cases <- read_golden("bond_cases.csv")
cases <- cases[cases$built == "1", ]

rows <- list()
for (i in seq_len(nrow(cases))) {
  c <- cases[i, ]
  as_date <- function(x) if (x == "0") QuantLib::Date() else from_serial(as.integer(x))
  sc <- QuantLib::Schedule(from_serial(as.integer(c$effective)), from_serial(as.integer(c$termination)),
                           QuantLib::Period(as.integer(c$tenor_n), c$tenor_unit), calendars[[c$calendar]],
                           c$convention, c$termination_convention, c$rule, c$end_of_month == "1",
                           as_date(c$first_date), as_date(c$next_to_last_date))
  bond <- QuantLib::FixedRateBond(as.integer(c$settlement_days), as.numeric(c$face_amount), sc, as.numeric(c$coupon),
                                  bond_day_counters[[c$day_counter]], c$payment_convention, as.numeric(c$redemption),
                                  from_serial(as.integer(c$issue_date)))
  eff <- as.integer(c$effective); term <- as.integer(c$termination)
  for (j in 1:6) {
    evaluation <- eff + sample(0:max(0L, term - eff - 30L), 1)
    QuantLib::Settings_instance()$setEvaluationDate(from_serial(evaluation))
    settlement <- QuantLib::Bond_settlementDate(bond)
    comp <- sample(c("Simple", "Compounded", "Compounded", "Continuous"), 1)
    freq <- if (comp == "Compounded") sample(names(frequencies), 1) else "Annual"
    rate <- sample(c(-0.005, 0, 0.0001, 0.015, 0.04, 0.12), 1)
    dc_name <- sample(names(yield_day_counters), 1)
    dc <- yield_day_counters[[dc_name]]
    clean <- num(QuantLib::BondFunctions_cleanPrice(bond, rate, dc, comp, freq, settlement))
    accrued <- num(QuantLib::Bond_accruedAmount(bond, settlement))
    dur <- vapply(c("Simple", "Modified", "Macaulay"), function(t)
      num(QuantLib::BondFunctions_duration(bond, rate, dc, comp, freq, t, settlement)), "")
    convexity <- num(QuantLib::BondFunctions_convexity(bond, rate, dc, comp, freq, settlement))
    bpv <- num(QuantLib::BondFunctions_basisPointValue(bond, rate, dc, comp, freq, settlement))
    yvbp <- num(QuantLib::BondFunctions_yieldValueBasisPoint(bond, rate, dc, comp, freq, settlement))
    # Yield from a price near the computed one (clean or dirty).
    price_type <- sample(c("Clean", "Dirty"), 1)
    base_price <- if (clean == "ERROR") 100 else as.numeric(clean) + if (price_type == "Dirty") as.numeric(accrued) else 0
    price <- base_price + sample(c(0, 0, -0.25, 0.4, -3), 1)
    yld <- num(QuantLib::BondFunctions_yield(bond, QuantLib::BondPrice(price, price_type), dc, comp, freq, settlement))
    rows[[length(rows) + 1L]] <- data.frame(
      case = c$case, evaluation_date = evaluation, settlement_date = s(settlement), rate = fmt(rate),
      day_counter = dc_name, compounding = comp, frequency = frequencies[[freq]],
      clean_price = clean, accrued = accrued, duration_simple = dur[["Simple"]], duration_modified = dur[["Modified"]],
      duration_macaulay = dur[["Macaulay"]], convexity = convexity, bpv = bpv, yvbp = yvbp,
      price = fmt(price), price_type = price_type, yield = yld)
  }
}
out <- do.call(rbind, rows)
path <- file.path(out_dir, "bond_yields.csv")
writeLines(c(paste0("# QuantLib R binding ", ql_version, "; BondFunctions cleanPrice/duration/convexity/basisPointValue/",
                    "yieldValueBasisPoint for a yield, and yield (NewtonSafe, accuracy 1e-10, guess 0.05) for a price"),
             paste(names(out), collapse = ",")), path)
utils::write.table(out, path, sep = ",", row.names = FALSE, col.names = FALSE, append = TRUE, quote = FALSE)
cat("rows:", nrow(out), "; errors: clean", sum(out$clean_price == "ERROR"), "macaulay", sum(out$duration_macaulay == "ERROR"),
    "yield", sum(out$yield == "ERROR"), "\n")
