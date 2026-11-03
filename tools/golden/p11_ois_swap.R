# Golden values for Riemann P11-2 (overnight indexed swaps: legs, NPV, fair rate, fixings) from the
# QuantLib R package Version 1.42, built with MakeOIS as LagrangeFinance II-3 builds them. All inputs are
# synthetic; no market data is used. The curves are log-linear on the same nodes as Riemann's, so this
# compares the swap and not the bootstrap.
# Usage (from the Riemann root): Rscript tools/golden/p11_ois_swap.R
# Writes tests/golden/ois_swaps.csv. Lists inside a cell are separated by spaces. status is ok, or
# failed with the message.

suppressMessages(library(QuantLib))

set.seed(20261113)
fmt <- function(x) sprintf("%.17g", x)
at <- function(serial) QuantLib::Date(as.integer(serial))
number_of <- function(d) as.integer(QuantLib::Date_serialNumber(d))
day_counts <- list(tonar = QuantLib::Actual365Fixed(), estr = QuantLib::Actual360(), sofr = QuantLib::Actual360(), sonia = QuantLib::Actual365Fixed())
calendars <- list(tonar = QuantLib::Japan(), estr = QuantLib::TARGET(), sofr = QuantLib::UnitedStates("SOFR"), sonia = QuantLib::UnitedKingdom("Exchange"))
index_of <- function(name, handle) {
  switch(name,
    tonar = QuantLib::OvernightIndex("TONA", 0L, QuantLib::JPYCurrency(), QuantLib::Japan(), QuantLib::Actual365Fixed(), handle),
    estr = QuantLib::Estr(handle),
    sofr = QuantLib::Sofr(handle),
    sonia = QuantLib::Sonia(handle)
  )
}
frequencies <- c(`1` = "Annual", `2` = "Semiannual", `4` = "Quarterly")
frequency_value <- function(n) get(paste0("Frequency_", frequencies[[as.character(n)]], "_get"), asNamespace("QuantLib"))()

curve_of <- function(reference, level, day_count) {
  node_days <- c(0L, 7L, 30L, 90L, 180L, 365L, 730L, 1825L, 3650L, 7300L)
  zero <- level + cumsum(c(0, runif(9, -0.0015, 0.002)))
  discounts <- exp(-zero * node_days / 365)
  dates <- do.call(QuantLib::DateVector, list())
  for (d in node_days) QuantLib::DateVector_push_back(dates, at(reference + d))
  curve <- QuantLib::DiscountCurve(dates, discounts, day_count)
  QuantLib::TermStructure_enableExtrapolation(curve)
  list(curve = curve, nodes = paste(paste0(reference + node_days, ":", fmt(discounts)), collapse = " "))
}

leg_values <- function(leg) {
  vapply(seq_len(leg$size()), function(i) tryCatch(fmt(QuantLib::CashFlow_amount(leg[i][[1]])), error = function(e) "NA"), "")
}
leg_dates <- function(leg) {
  vapply(seq_len(leg$size()), function(i) number_of(QuantLib::CashFlow_date(leg[i][[1]])), 0L)
}

path <- file.path("tests", "golden", "ois_swaps.csv")
writeLines(c(
  paste0("# QuantLib R package ", packageVersion("QuantLib"), "; synthetic inputs; tools/golden/p11_ois_swap.R"),
  paste(c("index", "side", "nominal", "effective", "termination", "fixed_rate", "spread", "fixed_frequency", "overnight_frequency", "payment_lag",
          "evaluation", "reference", "forecast_nodes", "discount_nodes", "fixings",
          "status", "npv", "fixed_npv", "overnight_npv", "fixed_bps", "overnight_bps", "fair_rate", "fair_spread",
          "fixed_dates", "fixed_amounts", "overnight_dates", "overnight_amounts", "message"), collapse = ",")
), path)

for (k in 1:300) {
  name <- sample(names(day_counts), 1)
  day_count <- day_counts[[name]]
  calendar <- calendars[[name]]
  base <- 44000L + sample(0:3000, 1)
  reference <- number_of(QuantLib::Calendar_adjust(calendar, at(base)))

  # the swap starts before, on or after the evaluation date
  start_offset <- sample(c(-400L, -40L, -5L, 0L, 2L, 30L), 1, prob = c(1, 2, 2, 1, 3, 1))
  evaluation <- number_of(QuantLib::Calendar_adjust(calendar, at(reference + sample(c(0L, 0L, 3L, 10L), 1))))
  effective <- number_of(QuantLib::Calendar_adjust(calendar, at(evaluation + start_offset)))
  length_days <- sample(c(7L, 30L, 91L, 365L, 366L, 730L, 1825L, 3650L), 1)
  termination <- number_of(QuantLib::Calendar_adjust(calendar, at(effective + length_days)))
  if (termination <= effective) next
  side <- sample(c("payer", "receiver"), 1)
  nominal <- sample(c(1e6, 1e7, 12345678), 1)
  fixed_rate <- round(runif(1, -0.002, 0.08), 5)
  spread <- sample(c(0, 0, 0.001, -0.0005), 1)
  fixed_frequency <- sample(c(1L, 1L, 2L, 4L), 1)
  overnight_frequency <- sample(c(1L, 1L, 2L, 4L), 1)
  payment_lag <- sample(c(0L, 0L, 1L, 2L), 1)

  QuantLib::Settings_instance()$setEvaluationDate(at(evaluation))
  forecast <- curve_of(reference, runif(1, -0.001, 0.05), day_count)
  index <- index_of(name, QuantLib::YieldTermStructureHandle(forecast$curve))
  separate_discount <- runif(1) < 0.3
  discount <- if (separate_discount) curve_of(reference, runif(1, -0.001, 0.05), day_count) else forecast
  discount_handle <- QuantLib::YieldTermStructureHandle(discount$curve)

  # fixings: all past business days, one short in some cases, and today's in some cases
  QuantLib::Index_clearFixings(index)
  fixing_serials <- integer()
  if (effective < evaluation) {
    days <- effective:(evaluation - 1L)
    fixing_serials <- days[vapply(days, function(d) QuantLib::Calendar_isBusinessDay(calendar, at(d)), TRUE)]
    if (runif(1) < 0.1 && length(fixing_serials) > 0L) fixing_serials <- fixing_serials[-sample(length(fixing_serials), 1)]
  }
  if (runif(1) < 0.3 && QuantLib::Calendar_isBusinessDay(calendar, at(evaluation)) && effective <= evaluation) fixing_serials <- c(fixing_serials, evaluation)
  fixing_rates <- runif(length(fixing_serials), -0.001, 0.06)
  for (i in seq_along(fixing_serials)) index$addFixing(at(fixing_serials[i]), fixing_rates[i])
  fixings_text <- if (length(fixing_serials)) paste(paste0(fixing_serials, ":", fmt(fixing_rates)), collapse = " ") else "none"

  outcome <- tryCatch({
    builder <- QuantLib::MakeOIS(swapTenor = QuantLib::Period(as.integer(termination - effective), "Days"), overnightIndex = index, fixedRate = fixed_rate)
    builder <- QuantLib::MakeOIS_withEffectiveDate(builder, at(effective))
    builder <- QuantLib::MakeOIS_withTerminationDate(builder, at(termination))
    builder <- QuantLib::MakeOIS_withNominal(builder, nominal)
    builder <- QuantLib::MakeOIS_withOvernightLegSpread(builder, spread)
    builder <- QuantLib::MakeOIS_withPaymentLag(builder, payment_lag)
    builder <- QuantLib::MakeOIS_withFixedLegPaymentFrequency(builder, frequency_value(fixed_frequency))
    builder <- QuantLib::MakeOIS_withOvernightLegPaymentFrequency(builder, frequency_value(overnight_frequency))
    builder <- QuantLib::MakeOIS_withType(builder, if (side == "payer") QuantLib::Swap_Payer_get() else QuantLib::Swap_Receiver_get())
    builder <- QuantLib::MakeOIS_withDiscountingTermStructure(builder, discount_handle)
    swap <- QuantLib::MakeOIS_makeOIS(builder)
    values <- c(swap$NPV(), QuantLib::FixedVsFloatingSwap_fixedLegNPV(swap), QuantLib::OvernightIndexedSwap_overnightLegNPV(swap),
                QuantLib::FixedVsFloatingSwap_fixedLegBPS(swap), QuantLib::OvernightIndexedSwap_overnightLegBPS(swap),
                tryCatch(QuantLib::FixedVsFloatingSwap_fairRate(swap), error = function(e) NaN),
                tryCatch(QuantLib::FixedVsFloatingSwap_fairSpread(swap), error = function(e) NaN))
    fixed_leg <- QuantLib::FixedVsFloatingSwap_fixedLeg(swap)
    overnight_leg <- swap$overnightLeg()
    c("ok", fmt(values), paste(leg_dates(fixed_leg), collapse = " "), paste(leg_values(fixed_leg), collapse = " "),
      paste(leg_dates(overnight_leg), collapse = " "), paste(leg_values(overnight_leg), collapse = " "), "")
  }, error = function(e) {
    c("failed", rep("NA", 7), "", "", "", "", gsub("[,\n]", ";", sub("^SWIG:RuntimeError ", "", conditionMessage(e))))
  })

  write(paste(c(name, side, fmt(nominal), effective, termination, fmt(fixed_rate), fmt(spread), fixed_frequency, overnight_frequency, payment_lag,
                evaluation, reference, forecast$nodes, if (separate_discount) discount$nodes else "same", fixings_text, outcome), collapse = ","), path, append = TRUE)
}
cat("written", length(readLines(path)) - 2L, "rows\n")
print(table(vapply(strsplit(readLines(path)[-(1:2)], ","), function(x) x[16], "")))
print(sort(table(substr(vapply(strsplit(readLines(path)[-(1:2)], ","), function(x) x[28], ""), 1, 40)), decreasing = TRUE))
