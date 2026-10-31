# Golden values for Riemann P2-1 (fixed-rate bond cash flows and accrued interest)
# from the QuantLib R binding.
# Usage (from the Riemann root): Rscript tools/golden/p2_fixed_rate_bonds.R
# Output: tests/golden/bond_cases.csv, bond_cashflows.csv, bond_accrued.csv
# Inputs are drawn with a fixed seed; amounts are printed with 17 significant digits.

suppressMessages(library(QuantLib))

out_dir <- file.path("tests", "golden")
dir.create(out_dir, recursive = TRUE, showWarnings = FALSE)
ql_version <- as.character(packageVersion("QuantLib"))
set.seed(20261101)

calendars <- list(japan = QuantLib::Japan(), target = QuantLib::TARGET(),
                  us_sofr = QuantLib::UnitedStates("SOFR"), null_calendar = QuantLib::NullCalendar())
day_counters <- list(actual_365_fixed = QuantLib::Actual365Fixed(), actual_360 = QuantLib::Actual360(),
                     thirty_360_bond_basis = QuantLib::Thirty360("BondBasis"),
                     act_act_isma = QuantLib::ActualActual("ISMA"), act_act_isda = QuantLib::ActualActual("ISDA"))
conventions <- c("Following", "ModifiedFollowing", "Preceding", "Unadjusted")
tenors <- list(c(6, "Months"), c(1, "Years"), c(3, "Months"))

s <- function(x) as.integer(QuantLib::Date_serialNumber(x))
from_serial <- function(n) QuantLib::Date(as.integer(n))
fmt <- function(x) sprintf("%.17g", x)
starts <- c(s(QuantLib::Date(31L, "January", 2024L)), s(QuantLib::Date(29L, "February", 2024L)),
            s(QuantLib::Date(15L, "March", 2023L)), s(QuantLib::Date(30L, "September", 2025L)),
            s(QuantLib::Date(20L, "December", 2022L)), s(QuantLib::Date(1L, "July", 2026L)))

cases <- list(); flows <- list(); accrued <- list()
n_bonds <- 1500
for (k in seq_len(n_bonds)) {
  eff <- sample(starts, 1) + sample(c(0L, 0L, 3L, -5L), 1)
  years <- sample(c(1L, 2L, 5L, 7L, 10L, 20L, 30L), 1)
  term <- s(QuantLib::Date___add__(from_serial(eff), QuantLib::Period(years, "Years"))) + sample(c(0L, 0L, -10L, 12L), 1)
  tenor <- tenors[[sample(length(tenors), 1)]]
  cal <- sample(names(calendars), 1)
  conv <- sample(conventions, 1); term_conv <- sample(conventions, 1); pay_conv <- sample(conventions, 1)
  rule <- sample(c("Backward", "Forward"), 1)
  eom <- sample(c(FALSE, TRUE), 1)
  first <- if (runif(1) < 0.15) eff + sample(c(40L, 75L, 100L), 1) else 0L
  ntl <- if (runif(1) < 0.15) term - sample(c(40L, 75L, 100L), 1) else 0L
  coupon <- sample(c(0, 0.0005, 0.015, 0.0425, 0.08), 1)
  dc <- sample(names(day_counters), 1)
  redemption <- sample(c(100, 100, 101.5), 1)
  settlement_days <- sample(0:3, 1)
  face <- sample(c(100, 1e6), 1)

  ok <- tryCatch({
    sc <- QuantLib::Schedule(from_serial(eff), from_serial(term), QuantLib::Period(as.integer(tenor[1]), tenor[2]),
                             calendars[[cal]], conv, term_conv, rule, eom,
                             if (first > 0) from_serial(first) else QuantLib::Date(),
                             if (ntl > 0) from_serial(ntl) else QuantLib::Date())
    bond <- QuantLib::FixedRateBond(as.integer(settlement_days), face, sc, coupon, day_counters[[dc]], pay_conv,
                                    redemption, from_serial(eff))
    cf <- QuantLib::Bond_cashflows(bond)
    n <- QuantLib::Leg___len__(cf)
    rows <- lapply(seq_len(n) - 1L, function(i) {
      c0 <- QuantLib::Leg___getitem__(cf, i)
      cp <- tryCatch(QuantLib::as_coupon(c0), error = function(e) NULL)
      is_coupon <- !is.null(cp) && !is.null(attr(cp, "class")) && class(cp)[1] == "_p_Coupon" &&
        !inherits(tryCatch(QuantLib::Coupon_nominal(cp), error = function(e) e), "error")
      if (is_coupon) {
        data.frame(case = k, kind = "coupon", date = s(QuantLib::CashFlow_date(c0)), amount = fmt(QuantLib::CashFlow_amount(c0)),
                   nominal = fmt(QuantLib::Coupon_nominal(cp)), rate = fmt(QuantLib::Coupon_rate(cp)),
                   accrual_start = s(QuantLib::Coupon_accrualStartDate(cp)), accrual_end = s(QuantLib::Coupon_accrualEndDate(cp)),
                   ref_start = s(QuantLib::Coupon_referencePeriodStart(cp)), ref_end = s(QuantLib::Coupon_referencePeriodEnd(cp)))
      } else {
        data.frame(case = k, kind = "redemption", date = s(QuantLib::CashFlow_date(c0)), amount = fmt(QuantLib::CashFlow_amount(c0)),
                   nominal = "", rate = "", accrual_start = 0L, accrual_end = 0L, ref_start = 0L, ref_end = 0L)
      }
    })
    flows[[length(flows) + 1L]] <- do.call(rbind, rows)
    # accrued interest at a few evaluation dates during the life of the bond
    evals <- sort(unique(c(eff - 3L, eff + sample(0:(term - eff), 4), term - 1L, term + 2L)))
    for (e in evals) {
      QuantLib::Settings_instance()$setEvaluationDate(from_serial(e))
      res <- tryCatch(c(s(QuantLib::Bond_settlementDate(bond)), fmt(QuantLib::Bond_notional(bond)),
                        fmt(QuantLib::Bond_accruedAmount(bond))), error = function(err) c("ERROR", "ERROR", "ERROR"))
      accrued[[length(accrued) + 1L]] <- data.frame(case = k, evaluation_date = e, settlement_date = res[1],
                                                    notional = res[2], accrued = res[3])
    }
    TRUE
  }, error = function(e) FALSE)

  cases[[k]] <- data.frame(case = k, effective = eff, termination = term, tenor_n = as.integer(tenor[1]), tenor_unit = tenor[2],
                           calendar = cal, convention = conv, termination_convention = term_conv, rule = rule,
                           end_of_month = as.integer(eom), first_date = first, next_to_last_date = ntl,
                           coupon = fmt(coupon), day_counter = dc, payment_convention = pay_conv, redemption = fmt(redemption),
                           settlement_days = settlement_days, face_amount = fmt(face), issue_date = eff, built = as.integer(ok))
}

write_golden <- function(df, file, note) {
  path <- file.path(out_dir, file)
  writeLines(c(paste0("# QuantLib R binding ", ql_version, "; ", note), paste(names(df), collapse = ",")), path)
  utils::write.table(df, path, sep = ",", row.names = FALSE, col.names = FALSE, append = TRUE, quote = FALSE)
}
cases <- do.call(rbind, cases); flows <- do.call(rbind, flows); accrued <- do.call(rbind, accrued)
write_golden(cases, "bond_cases.csv", "FixedRateBond inputs (schedule inputs as in schedules.csv; built = 0 if QuantLib raised an error)")
write_golden(flows, "bond_cashflows.csv", "Bond::cashflows(): coupons (with reference periods) and the redemption")
write_golden(accrued, "bond_accrued.csv", "settlementDate(), notional() and accruedAmount() (per 100) for an evaluation date")
cat("bonds:", nrow(cases), "built:", sum(cases$built), "cash flows:", nrow(flows), "accrued rows:", nrow(accrued),
    "accrued errors:", sum(accrued$accrued == "ERROR"), "\n")
