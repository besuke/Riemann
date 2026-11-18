# Golden values for the dates of an OIS swap made from a tenor: the effective and the termination date of
# QuantLib's MakeOIS(swapTenor, overnightIndex, ...) with settlement days, as the R package builds them.
# Inputs are synthetic: evaluation dates on every kind of day (business, weekend, holiday, month end),
# the four indices, tenors from days to 30 years, and settlement days 0 to 3.
#
# Usage (from the rmql root): Rscript tools/golden/p11_ois_swap_dates.R
# Writes tests/golden/ois_swap_dates.csv. status is ok, or failed with the reason in the last column.
suppressMessages(library(QuantLib))

at <- function(serial) QuantLib::Date(as.integer(serial))
number_of <- function(d) as.integer(QuantLib::Date_serialNumber(d))
# QuantLib's serial number is the days since 1899-12-30 for dates after 1900-03-01.
serial_of <- function(year, month, day) as.integer(as.Date(sprintf("%04d-%02d-%02d", year, month, day)) - as.Date("1899-12-30"))

handle <- QuantLib::YieldTermStructureHandle(QuantLib::FlatForward(at(serial_of(2026, 1, 5)), 0.01, QuantLib::Actual365Fixed()))
index_of <- function(name) {
  switch(name,
    tonar = QuantLib::OvernightIndex("TONA", 0L, QuantLib::JPYCurrency(), QuantLib::Japan(), QuantLib::Actual365Fixed(), handle),
    estr = QuantLib::Estr(handle),
    sofr = QuantLib::Sofr(handle),
    sonia = QuantLib::Sonia(handle)
  )
}

# Evaluation dates: every day of two spans, which covers weekends, holidays and month ends of all years.
evaluations <- c(serial_of(2026, 12, 20):serial_of(2027, 1, 12), serial_of(2028, 2, 20):serial_of(2028, 3, 3), serial_of(2026, 4, 27):serial_of(2026, 5, 6),
                 serial_of(2026, 6, 26):serial_of(2026, 7, 3), serial_of(2026, 9, 28):serial_of(2026, 10, 2), serial_of(2026, 9, 8):serial_of(2026, 9, 12))
tenors <- list(c(1L, "Days"), c(1L, "Weeks"), c(2L, "Weeks"), c(1L, "Months"), c(3L, "Months"), c(6L, "Months"), c(13L, "Months"),
               c(1L, "Years"), c(2L, "Years"), c(5L, "Years"), c(7L, "Years"), c(10L, "Years"), c(30L, "Years"))
settlement_days <- c(0L, 1L, 2L, 3L)

path <- file.path("tests", "golden", "ois_swap_dates.csv")
dir.create(dirname(path), showWarnings = FALSE)
writeLines(c(paste0("# QuantLib R package ", packageVersion("QuantLib"), "; synthetic inputs; tools/golden/p11_ois_swap_dates.R"),
             "index,evaluation,tenor_length,tenor_unit,settlement_days,status,effective,termination,message"), path)

rows <- character()
for (name in c("tonar", "estr", "sofr", "sonia")) {
  for (evaluation in evaluations) {
    QuantLib::Settings_instance()$setEvaluationDate(at(evaluation))
    for (tenor in tenors) {
      for (lag in settlement_days) {
        outcome <- tryCatch({
          index <- index_of(name)
          builder <- QuantLib::MakeOIS(swapTenor = QuantLib::Period(as.integer(tenor[1]), tenor[2]), overnightIndex = index, fixedRate = 0.01)
          builder <- QuantLib::MakeOIS_withSettlementDays(builder, lag)
          swap <- QuantLib::MakeOIS_makeOIS(builder)
          c("ok", number_of(swap$startDate()), number_of(swap$maturityDate()), "")
        }, error = function(e) c("failed", "NA", "NA", gsub("[,\n]", ";", sub("^SWIG:RuntimeError ", "", conditionMessage(e)))))
        rows <- c(rows, paste(c(name, evaluation, tenor[1], tenor[2], lag, outcome), collapse = ","))
      }
    }
  }
}
write(rows, path, append = TRUE)
cat("written", length(rows), "rows\n")
print(table(vapply(strsplit(rows, ","), function(x) x[6], "")))
