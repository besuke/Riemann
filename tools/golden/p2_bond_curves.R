# Golden values for Riemann P2-3 (discount curves and bond valuation on a curve) from the
# QuantLib R binding.
# Usage (from the Riemann root): Rscript tools/golden/p2_bond_curves.R
# Reads tests/golden/bond_cases.csv (P2-1); writes tests/golden/curves.csv, curve_discounts.csv
# and bond_curve_values.csv. Numbers have 17 significant digits; ERROR = QuantLib raised an error.

suppressMessages(library(QuantLib))

out_dir <- file.path("tests", "golden")
ql_version <- as.character(packageVersion("QuantLib"))
set.seed(20261103)

calendars <- list(japan = QuantLib::Japan(), target = QuantLib::TARGET(),
                  us_sofr = QuantLib::UnitedStates("SOFR"), null_calendar = QuantLib::NullCalendar())
bond_day_counters <- list(actual_365_fixed = QuantLib::Actual365Fixed(), actual_360 = QuantLib::Actual360(),
                          thirty_360_bond_basis = QuantLib::Thirty360("BondBasis"),
                          act_act_isma = QuantLib::ActualActual("ISMA"), act_act_isda = QuantLib::ActualActual("ISDA"))
curve_day_counters <- list(actual_365_fixed = QuantLib::Actual365Fixed(), actual_360 = QuantLib::Actual360(),
                           thirty_360_bond_basis = QuantLib::Thirty360("BondBasis"), act_act_isda = QuantLib::ActualActual("ISDA"))
frequencies <- c(Annual = 1L, Semiannual = 2L, Quarterly = 4L, Monthly = 12L)

from_serial <- function(n) QuantLib::Date(as.integer(n))
s <- function(x) as.integer(QuantLib::Date_serialNumber(x))
fmt <- function(x) sprintf("%.17g", x)
num <- function(expr) tryCatch(fmt(expr), error = function(e) "ERROR")
add_months <- function(d, m) s(QuantLib::Date___add__(from_serial(d), QuantLib::Period(as.integer(m), "Months")))

# A random curve on `reference`, reaching at least `horizon` (or stopping short of it).
make_curve <- function(id, reference, horizon) {
  kind <- sample(c("zero", "zero", "discount", "flat", "spread"), 1)
  base_kind <- if (kind == "spread") sample(c("zero", "discount"), 1) else kind
  dc_name <- sample(names(curve_day_counters), 1)
  dc <- curve_day_counters[[dc_name]]
  extrapolate <- runif(1) < 0.4
  comp <- "Continuous"; freq <- "Annual"
  nodes <- ""; curve <- NULL
  if (base_kind == "flat") {
    comp <- sample(c("Simple", "Compounded", "Continuous"), 1)
    freq <- if (comp == "Compounded") sample(names(frequencies), 1) else "Annual"
    rate <- sample(c(-0.004, 0.001, 0.0125, 0.035, 0.07), 1)
    curve <- QuantLib::FlatForward(from_serial(reference), rate, dc, comp, freq)
    nodes <- paste0(reference, ":", fmt(rate))
  } else {
    months <- c(1, 3, 6, 12, 24, 36, 60, 84, 120, 180, 240, 360)
    last <- if (runif(1) < 0.8) horizon + sample(c(30L, 400L), 1) else horizon - 200L
    dates <- reference
    for (m in months) {
      d <- add_months(reference, m)
      if (d > max(last, add_months(reference, 1))) break
      dates <- c(dates, d)
    }
    if (length(dates) < 2) dates <- c(reference, add_months(reference, 12))
    z <- cumsum(c(sample(c(-0.004, 0.002, 0.01, 0.03), 1), runif(length(dates) - 1, -0.002, 0.006)))
    dv <- QuantLib::DateVector()
    for (d in dates) QuantLib::DateVector_append(dv, from_serial(d))
    if (base_kind == "zero") {
      comp <- sample(c("Continuous", "Continuous", "Compounded", "Simple"), 1)
      freq <- if (comp == "Compounded") sample(names(frequencies), 1) else "Annual"
      curve <- QuantLib::ZeroCurve(dv, z, dc, QuantLib::NullCalendar(), QuantLib::Linear(), comp, freq)
      values <- z
    } else {
      t <- (dates - reference) / 365
      values <- exp(-z * t); values[1] <- 1
      curve <- QuantLib::DiscountCurve(dv, values, dc)
    }
    nodes <- paste(paste0(dates, ":", fmt(values)), collapse = " ")
  }
  base_row <- data.frame(curve = paste0(id, if (kind == "spread") "b" else ""), kind = base_kind, reference = reference,
                         day_counter = dc_name, compounding = comp, frequency = frequencies[[freq]],
                         extrapolate = as.integer(extrapolate && kind != "spread"), nodes = nodes, base = "", spread = "")
  if (extrapolate && kind != "spread") QuantLib::TermStructure_enableExtrapolation(curve)
  rows <- base_row
  if (kind == "spread") {
    scomp <- sample(c("Simple", "Compounded", "Continuous"), 1)
    sfreq <- if (scomp == "Compounded") sample(names(frequencies), 1) else "Annual"
    spread <- sample(c(-0.002, 0.0005, 0.01, 0.025), 1)
    h <- QuantLib::YieldTermStructureHandle(curve)
    curve <- QuantLib::ZeroSpreadedTermStructure(h, QuantLib::QuoteHandle(QuantLib::SimpleQuote(spread)), scomp, sfreq, dc)
    if (extrapolate) QuantLib::TermStructure_enableExtrapolation(curve)
    rows <- rbind(rows, data.frame(curve = id, kind = "spread", reference = reference, day_counter = dc_name,
                                   compounding = scomp, frequency = frequencies[[sfreq]], extrapolate = as.integer(extrapolate),
                                   nodes = "", base = paste0(id, "b"), spread = fmt(spread)))
  }
  list(curve = curve, rows = rows)
}

read_golden <- function(file) {
  lines <- readLines(file.path(out_dir, file))
  utils::read.csv(text = lines[!startsWith(lines, "#")], colClasses = "character")
}
cases <- read_golden("bond_cases.csv")
cases <- cases[cases$built == "1", ]

curve_rows <- list(); discount_rows <- list(); bond_rows <- list()
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
  evaluation <- eff + sample(0:max(0L, term - eff - 30L), 1)
  QuantLib::Settings_instance()$setEvaluationDate(from_serial(evaluation))
  id <- paste0("c", c$case)
  made <- make_curve(id, evaluation, term + 10L)
  curve_rows[[length(curve_rows) + 1L]] <- made$rows
  # discount factors at dates from the reference date to beyond the horizon
  probe <- sort(unique(c(evaluation, evaluation + sample(1:(term - evaluation + 3000L), 6))))
  probe <- probe[probe <= 109574L]
  discount_rows[[length(discount_rows) + 1L]] <- data.frame(
    curve = id, date = probe, discount = vapply(probe, function(d) num(QuantLib::YieldTermStructure_discount(made$curve, from_serial(d))), ""))
  # bond on the curve
  engine <- QuantLib::DiscountingBondEngine(QuantLib::YieldTermStructureHandle(made$curve))
  QuantLib::Instrument_setPricingEngine(bond, engine)
  bond_rows[[length(bond_rows) + 1L]] <- data.frame(
    case = c$case, evaluation_date = evaluation, curve = id,
    npv = num(QuantLib::Instrument_NPV(bond)), clean_price = num(QuantLib::Bond_cleanPrice(bond)),
    dirty_price = num(QuantLib::Bond_dirtyPrice(bond)))
}

write_golden <- function(df, file, note) {
  path <- file.path(out_dir, file)
  writeLines(c(paste0("# QuantLib R binding ", ql_version, "; ", note), paste(names(df), collapse = ",")), path)
  utils::write.table(df, path, sep = ",", row.names = FALSE, col.names = FALSE, append = TRUE, quote = FALSE)
}
curves <- do.call(rbind, curve_rows); discounts <- do.call(rbind, discount_rows); bonds <- do.call(rbind, bond_rows)
write_golden(curves, "curves.csv", paste0("curve inputs: zero = ZeroCurve(Linear) of zero rates in the given compounding, ",
             "discount = DiscountCurve(LogLinear), flat = FlatForward, spread = ZeroSpreadedTermStructure on base; nodes are serial:value"))
write_golden(discounts, "curve_discounts.csv", "YieldTermStructure::discount(date)")
write_golden(bonds, "bond_curve_values.csv", "FixedRateBond with DiscountingBondEngine: NPV(), cleanPrice(), dirtyPrice()")
cat("curves:", length(unique(curves$curve)), "discounts:", nrow(discounts), "errors", sum(discounts$discount == "ERROR"),
    "; bonds:", nrow(bonds), "errors", sum(bonds$npv == "ERROR"), "\n")
print(table(curves$kind))
