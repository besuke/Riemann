# Golden values for the Hull-White calibration (P11-3): QuantLib's CalibratedModel::calibrate on
# European swaptions, with its own Levenberg-Marquardt, EndCriteria and constraints. All inputs are
# synthetic; market prices are QuantLib's Black prices of synthetic volatilities.
# Usage (from the Riemann root): Rscript tools/golden/p11_hull_white_calibration.R
# Reads tests/golden/hw_curve.csv. Writes tests/golden/hw_calibration_cases.csv and
# hw_calibration_instruments.csv.

suppressMessages(library(QuantLib))

out_dir <- file.path("tests", "golden")
set.seed(20261103)
fmt <- function(x) sprintf("%.17g", x)
day_count <- QuantLib::Actual365Fixed()
calendar <- QuantLib::NullCalendar()
today <- QuantLib::Date(2L, "November", 2026L)
invisible(QuantLib::Settings_instance()$setEvaluationDate(today))
today_serial <- as.integer(QuantLib::Date_serialNumber(today))
at_days <- function(days) QuantLib::Date(today_serial + as.integer(days))

nodes <- read.csv(file.path(out_dir, "hw_curve.csv"), comment.char = "#")
dates <- do.call(QuantLib::DateVector, list())
for (d in nodes$days) QuantLib::DateVector_push_back(dates, at_days(d))
curve <- QuantLib::DiscountCurve(dates, nodes$discount, day_count)
handle <- QuantLib::YieldTermStructureHandle(curve)
discount <- function(days) QuantLib::YieldTermStructure_discount(curve, days / 365)
index <- QuantLib::IborIndex("Synthetic1Y", QuantLib::Period("Annual"), 0L, QuantLib::JPYCurrency(), calendar, "Unadjusted", FALSE, day_count, handle)

# The dates of the helper's fixed schedule, from today: calendar years from the exercise, forward
schedule_days <- function(start_years, length_years) {
  start <- QuantLib::Calendar_advance(calendar, today, start_years, "Years", "Unadjusted")
  end <- QuantLib::Calendar_advance(calendar, start, length_years, "Years", "Unadjusted")
  schedule <- QuantLib::Schedule(start, end, QuantLib::Period("Annual"), calendar, "Unadjusted", "Unadjusted", "Forward", FALSE)
  vapply(seq_len(QuantLib::Schedule_size(schedule)) - 1L, function(i) as.integer(QuantLib::Date_serialNumber(QuantLib::Schedule_date(schedule, i))) - today_serial, 1L)
}

price_error <- QuantLib::BlackCalibrationHelper_PriceError_get()
relative_error <- QuantLib::BlackCalibrationHelper_RelativePriceError_get()
criteria <- QuantLib::EndCriteria(1000L, 100L, 1e-8, 1e-8, 1e-8)
tight_criteria <- QuantLib::EndCriteria(100000L, 100L, 1e-12, 1e-12, 1e-12)

path_cases <- file.path(out_dir, "hw_calibration_cases.csv")
path_instruments <- file.path(out_dir, "hw_calibration_instruments.csv")
version <- paste0("# QuantLib R package ", packageVersion("QuantLib"), "; synthetic inputs; tools/golden/p11_hull_white_calibration.R")
writeLines(c(version, "case,error,fix_a,initial_a,initial_sigma,a,sigma,objective,end_criteria,instruments,a_tight,sigma_tight"), path_cases)
writeLines(c(version, "case,payer,days,fixed_rate,market,model"), path_instruments)

case <- 0L
for (set in 1:20) {
  n <- sample(4:8, 1)
  rows <- lapply(seq_len(n), function(i) {
    start_years <- sample(1:7, 1); length_years <- sample(c(2L, 3L, 5L, 7L, 10L), 1)
    days <- schedule_days(start_years, length_years)
    tau <- diff(days) / 365
    fair <- (discount(days[1]) - discount(days[length(days)])) / sum(tau * vapply(days[-1], discount, 1))
    strike <- fair + sample(c(-1, 1), 1) * runif(1, 0.001, 0.01)
    list(start_years = start_years, length_years = length_years, days = days, strike = strike, payer = strike > fair, volatility = runif(1, 0.25, 0.45))
  })
  for (mode in c("price", "relative", "price_fixed_a")) {
    case <- case + 1L
    error <- if (mode == "relative") relative_error else price_error
    fix_a <- mode == "price_fixed_a"
    initial <- c(a = 0.05, sigma = 0.01)
    calibrate <- function(lm, end_criteria) {
      model <- QuantLib::HullWhite(handle, initial[["a"]], initial[["sigma"]])
      engine <- QuantLib::JamshidianSwaptionEngine(model)
      helpers <- QuantLib::CalibrationHelperVector()
      made <- lapply(rows, function(r) {
        helper <- QuantLib::SwaptionHelper(
          QuantLib::Period(r$start_years, "Years"), QuantLib::Period(r$length_years, "Years"),
          QuantLib::QuoteHandle(QuantLib::SimpleQuote(r$volatility)), index, QuantLib::Period("Annual"), day_count, day_count, handle, error, r$strike, 1
        )
        QuantLib::BlackCalibrationHelper_setPricingEngine(helper, engine)
        QuantLib::CalibrationHelperVector_push_back(helpers, helper)
        helper
      })
      weights <- numeric(0)
      if (fix_a) {
        fixed <- c(TRUE, FALSE)
        QuantLib::CalibratedModel_calibrate(model, helpers, lm, end_criteria, QuantLib::NoConstraint(), weights, fixed)
      } else {
        QuantLib::CalibratedModel_calibrate(model, helpers, lm, end_criteria)
      }
      params <- QuantLib::CalibratedModel_params(model)
      list(model = model, helpers = made, a = QuantLib::Array___getitem__(params, 0L), sigma = QuantLib::Array___getitem__(params, 1L))
    }
    # QuantLib's usual tolerances, and tight ones to show where its search is heading
    usual <- calibrate(QuantLib::LevenbergMarquardt(1e-8, 1e-8, 1e-8), criteria)
    tight <- calibrate(QuantLib::LevenbergMarquardt(1e-12, 1e-12, 1e-12), tight_criteria)
    model <- usual$model; made <- usual$helpers; a <- usual$a; sigma <- usual$sigma
    market <- vapply(made, QuantLib::BlackCalibrationHelper_marketValue, 1)
    fitted <- vapply(made, QuantLib::BlackCalibrationHelper_modelValue, 1)
    w <- if (mode == "relative") 1 / market^2 else rep(1, n)
    objective <- sum(w * (market - fitted)^2)
    end <- QuantLib::CalibratedModel_endCriteria(model)
    write(paste(c(case, mode, as.integer(fix_a), fmt(initial), fmt(c(a, sigma, objective)), end, n, fmt(c(tight$a, tight$sigma))), collapse = ","), path_cases, append = TRUE)
    for (i in seq_len(n)) {
      write(paste(c(case, as.integer(rows[[i]]$payer), paste(rows[[i]]$days, collapse = ";"), fmt(c(rows[[i]]$strike, market[i], fitted[i]))), collapse = ","), path_instruments, append = TRUE)
    }
    cat(sprintf("%2d %-14s n=%d a=%.6f sigma=%.6f obj=%.3e end=%s\n", case, mode, n, a, sigma, objective, end))
  }
}
