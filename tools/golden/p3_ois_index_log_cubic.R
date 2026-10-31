# Golden values for Riemann P3-2 (OIS curves of ESTR, SOFR and SONIA, monotonic log-cubic discount
# factors) from the QuantLib R package Version 1.42. The exact C++ commit of the package is
# unknown, so these are not results of running C++ v1.42 itself (see docs/spec_ois.md, chapter 1).
# PiecewiseLogCubicDiscount has the settings of C++ MonotonicLogCubic (docs/spec_ois.md 2.7).
# All quotes are synthetic random numbers; no market data is used. The settlement days, payment
# lags and frequencies are drawn at random and are not market conventions.
# Usage (from the Riemann root): Rscript tools/golden/p3_ois_index_log_cubic.R <estr|sofr|sonia>
# Writes tests/golden/ois_<index>_log_cubic_{cases,instruments,nodes,discounts}.csv.

suppressMessages(library(QuantLib))

index_name <- commandArgs(trailingOnly = TRUE)[1]
stopifnot(index_name %in% c("estr", "sofr", "sonia"))
out_dir <- file.path("tests", "golden")
ql_version <- as.character(packageVersion("QuantLib"))

settings <- list(
  # dates: holidays, weekends and month ends of each calendar
  estr = list(seed = 20261201L, make = QuantLib::Estr, calendar = QuantLib::TARGET(),
              settlement = c(2L, 2L, 2L, 0L, 1L),
              dates = c("2026-04-03", "2026-04-06", "2026-05-01", "2026-12-25", "2026-12-26", "2024-02-29", "2023-12-31",
                        "2025-01-01", "2025-01-31", "2028-02-29", "2025-08-30", "2026-06-30")),
  sofr = list(seed = 20261202L, make = QuantLib::Sofr, calendar = QuantLib::UnitedStates("SOFR"),
              settlement = c(2L, 2L, 2L, 0L, 1L),
              dates = c("2026-04-03", "2026-07-03", "2026-11-26", "2026-12-25", "2025-01-01", "2024-02-29", "2023-12-31",
                        "2024-06-19", "2025-01-31", "2028-02-29", "2025-08-30", "2026-06-30")),
  sonia = list(seed = 20261203L, make = QuantLib::Sonia, calendar = QuantLib::UnitedKingdom("Exchange"),
               settlement = c(0L, 0L, 0L, 1L, 2L),
               dates = c("2026-04-03", "2026-04-06", "2026-05-04", "2026-05-25", "2026-08-31", "2026-12-25", "2026-12-28",
                         "2022-09-19", "2024-02-29", "2023-12-31", "2025-01-31", "2028-02-29"))
)[[index_name]]
set.seed(settings$seed)

from_serial <- function(n) QuantLib::Date(as.integer(n))
s <- function(x) as.integer(QuantLib::Date_serialNumber(x))
fmt <- function(x) sprintf("%.17g", x)
clean <- function(m) gsub("[0-9][0-9.e+-]*", "N", gsub(",", ";", sub("^SWIG:RuntimeError ", "", m)))
ordinal_at <- function(m, pattern) {
  hit <- regmatches(m, regexpr(pattern, m))
  if (length(hit)) as.integer(sub("[^0-9].*$", "", hit)) else 0L
}
quote_of <- function(r) QuantLib::QuoteHandle(QuantLib::SimpleQuote(r))
rate_or_error <- function(expr) tryCatch(fmt(QuantLib::InterestRate_rate(expr)), error = function(e) "ERROR")
calendar <- settings$calendar
day_counters <- list(actual_365_fixed = QuantLib::Actual365Fixed(), actual_360 = QuantLib::Actual360())
frequencies <- list(`1` = QuantLib::Frequency_Annual_get(), `2` = QuantLib::Frequency_Semiannual_get())
units <- c(W = "Weeks", M = "Months", Y = "Years")
period_of <- function(t) QuantLib::Period(as.integer(sub("[WMY]$", "", t)), units[[sub("^[0-9]+", "", t)]])
all_tenors <- c("1W", "2W", "3W", "1M", "2M", "3M", "4M", "6M", "9M", "1Y", "18M", "2Y", "3Y", "4Y", "5Y", "7Y", "10Y",
                "12Y", "15Y", "20Y", "25Y", "30Y", "40Y")

cases <- list(); instruments <- list(); nodes <- list(); discounts <- list()
for (k in 1:400) {
  evaluation <- if (runif(1) < 0.25) as.Date(sample(settings$dates, 1)) else as.Date("2000-01-01") + sample(0:14600, 1)
  eval_serial <- as.integer(as.numeric(evaluation) + 25569)
  QuantLib::Settings_instance()$setEvaluationDate(from_serial(eval_serial))
  dc_name <- sample(c("actual_365_fixed", "actual_365_fixed", "actual_360"), 1); dc <- day_counters[[dc_name]]
  settlement_days <- sample(settings$settlement, 1)
  payment_lag <- sample(c(0L, 0L, 1L, 2L), 1)
  frequency <- sample(c(1L, 1L, 1L, 2L), 1)
  extrapolate <- runif(1) < 0.5
  spot <- s(QuantLib::Calendar_advance(calendar, QuantLib::Calendar_adjust(calendar, from_serial(eval_serial)),
                                       settlement_days, "Days"))
  u <- runif(1)
  reference <- if (u < 0.75) eval_serial else if (u < 0.97) spot else eval_serial + 30000L
  tenors <- sort(sample(seq_along(all_tenors), sample(1:14, 1)))
  tenor_names <- all_tenors[tenors]
  if (runif(1) < 0.02 && "1Y" %in% tenor_names) tenor_names <- c(tenor_names, "12M") # duplicate pillars
  has_deposit <- runif(1) < 0.7 || !length(tenor_names)
  labels <- c(if (has_deposit) "ON", tenor_names)
  level <- sample(c(-0.004, 0.0005, 0.005, 0.02, 0.05), 1)
  rates <- level + cumsum(runif(length(labels), -0.002, 0.003))
  shape <- runif(1)
  if (shape < 0.15) rates <- rates + runif(length(labels), -0.015, 0.015)      # non-monotonic quotes
  if (shape >= 0.15 && shape < 0.20) rates <- rates + rep_len(c(0.08, -0.08), length(labels)) # hard to bootstrap
  if (runif(1) < 0.02) rates[1] <- 1.5 # outside the bootstrap's bracket
  order <- if (runif(1) < 0.3) sample(seq_along(labels)) else seq_along(labels)
  labels <- labels[order]; rates <- rates[order]
  index <- settings$make()
  helpers <- lapply(seq_along(labels), function(i) {
    if (labels[i] == "ON") QuantLib::DepositRateHelper(quote_of(rates[i]), index)
    else QuantLib::OISRateHelper(settlement_days, period_of(labels[i]), quote_of(rates[i]), index,
                                 QuantLib::YieldTermStructureHandle(), FALSE, payment_lag, "Following",
                                 frequencies[[as.character(frequency)]])
  })
  vec <- QuantLib::RateHelperVector()
  for (h in helpers) QuantLib::RateHelperVector_append(vec, h)
  curve <- QuantLib::PiecewiseLogCubicDiscount(from_serial(reference), vec, dc)
  message_text <- tryCatch({ curve$maxDate(); "ok" }, error = function(e) conditionMessage(e))
  status <- if (message_text == "ok") "ok" else clean(message_text)
  cases[[k]] <- data.frame(case = k, evaluation = eval_serial, reference = reference, day_counter = dc_name,
                           extrapolate = as.integer(extrapolate), settlement_days = settlement_days,
                           payment_lag = payment_lag, payment_frequency = frequency,
                           instruments = paste(paste0(labels, ":", fmt(rates)), collapse = " "), status = status,
                           failed_iteration = ordinal_at(message_text, "[0-9]+[a-z]+ iteration: failed"),
                           failed_alive = ordinal_at(sub("^.*failed at ", "", message_text), "^[0-9]+[a-z]+ alive"))
  instruments[[k]] <- do.call(rbind, lapply(seq_along(helpers), function(i) {
    h <- helpers[[i]]; pillar <- s(h$pillarDate())
    used <- status == "ok" && pillar > reference
    data.frame(case = k, position = i - 1L, start_date = s(h$earliestDate()), maturity_date = s(h$maturityDate()),
               pillar_date = pillar, latest_relevant_date = s(h$latestRelevantDate()), used = as.integer(used),
               implied_rate = if (used) fmt(h$impliedQuote()) else "NA")
  }))
  if (status == "ok") {
    if (extrapolate) QuantLib::TermStructure_enableExtrapolation(curve)
    node_dates <- curve$dates(); values <- as.numeric(curve$data())
    serials <- vapply(seq_along(values), function(i) s(QuantLib::DateVector___getitem__(node_dates, i - 1L)), 0L)
    nodes[[k]] <- data.frame(case = k, date = serials, discount = fmt(values))
    last <- max(serials)
    probe <- sort(unique(c(reference, reference - 5L, sample(reference:last, min(8L, last - reference + 1L)),
                           last, last + sample(1:800, 2))))
    probe <- probe[probe >= 367L & probe <= 109573L]
    discounts[[k]] <- do.call(rbind, lapply(probe, function(d) {
      date <- from_serial(d); next_date <- from_serial(d + 1L)
      data.frame(case = k, date = d,
                 discount = tryCatch(fmt(QuantLib::YieldTermStructure_discount(curve, date)), error = function(e) "ERROR"),
                 zero_continuous = rate_or_error(QuantLib::YieldTermStructure_zeroRate(curve, date, dc, "Continuous", "Annual")),
                 forward_1d_simple = rate_or_error(QuantLib::YieldTermStructure_forwardRate(curve, date, next_date, dc, "Simple", "Annual")),
                 forward_instantaneous = rate_or_error(QuantLib::YieldTermStructure_forwardRate(curve, date, date, dc, "Continuous", "Annual")))
    }))
  }
}

write_golden <- function(rows, part, note) {
  df <- do.call(rbind, rows); path <- file.path(out_dir, sprintf("ois_%s_log_cubic_%s.csv", index_name, part))
  writeLines(c(paste0("# QuantLib R package Version ", ql_version, " (exact C++ commit unknown); synthetic quotes; ", note),
               paste(names(df), collapse = ",")), path)
  utils::write.table(df, path, sep = ",", row.names = FALSE, col.names = FALSE, append = TRUE, quote = FALSE)
  df
}
cs <- write_golden(cases, "cases", "instruments are label:rate; status is ok or the error message with numbers replaced; failed_iteration and failed_alive are 0 when not given")
is <- write_golden(instruments, "instruments", "helper dates and impliedQuote in the order of the cases file")
ns <- write_golden(nodes, "nodes", "curve nodes and discount factors")
ds <- write_golden(discounts, "discounts", "discount, zeroRate(Continuous) and forwardRate over one calendar day (Simple) and at one date (Continuous)")
cat(index_name, "cases", nrow(cs), "; built", sum(cs$status == "ok"), "; instruments", nrow(is), "; nodes", nrow(ns), "; discounts", nrow(ds), "errors", sum(ds$discount == "ERROR"), "\n")
