# Boundary cases of the zero spread solver from the QuantLib R package Version 1.42. All inputs are
# synthetic; no market data is used.
# Usage (from the Riemann root): Rscript tools/golden/p11_z_spread_edges.R
# Writes tests/golden/z_spread_edges.csv. status is ok, or the kind of failure. The same search takes
# the same steps, so a coarse accuracy shows whether the two searches follow the same path.

suppressMessages(library(QuantLib))

fmt <- function(x) sprintf("%.17g", x)
at <- function(serial) QuantLib::Date(as.integer(serial))
serial <- function(d) as.integer(as.Date(d)) + 25569L

evaluation <- serial("2026-11-02")
QuantLib::Settings_instance()$setEvaluationDate(at(evaluation))
day_count <- QuantLib::Actual365Fixed()
issue <- serial("2024-03-20")
maturity <- serial("2031-03-20")
schedule <- QuantLib::Schedule(at(issue), at(maturity), QuantLib::Period(6L, "Months"), QuantLib::Japan(), "Unadjusted", "Unadjusted", "Backward", FALSE)
bond <- QuantLib::FixedRateBond(2L, 100, schedule, 0.015, day_count, "Following", 100, at(issue))

node_days <- c(0L, 30L, 180L, 365L, 730L, 1825L, 2190L)
discounts <- exp(-c(0, 0.002, 0.003, 0.004, 0.006, 0.01, 0.012) * node_days / 365)
nodes <- paste(paste0(evaluation + node_days, ":", fmt(discounts)), collapse = " ")
curve_of <- function(extrapolate, last_days = 2190L) {
  dates <- do.call(QuantLib::DateVector, list())
  keep <- node_days <= last_days
  for (d in node_days[keep]) QuantLib::DateVector_push_back(dates, at(evaluation + d))
  curve <- QuantLib::DiscountCurve(dates, discounts[keep], day_count)
  if (extrapolate) QuantLib::TermStructure_enableExtrapolation(curve)
  curve
}

settlement_normal <- as.integer(QuantLib::Date_serialNumber(QuantLib::Bond_settlementDate(bond)))
reference <- QuantLib::BondFunctions_zSpread(bond, QuantLib::BondPrice(99, "Clean"), curve_of(TRUE), day_count, "Compounded", "Semiannual", at(settlement_normal), 1e-12, 1000L, 0)

cases <- tibble::tribble(
  ~label,                 ~price, ~type,   ~comp,        ~freq, ~settle,            ~accuracy, ~max_eval, ~guess,     ~extrapolate,
  "ordinary",             99,     "Clean", "Compounded", 2L,    settlement_normal,  1e-10,     100L,      0,          TRUE,
  "dirty",                99,     "Dirty", "Compounded", 2L,    settlement_normal,  1e-10,     100L,      0,          TRUE,
  "coarse accuracy",      99,     "Clean", "Compounded", 2L,    settlement_normal,  1e-3,      100L,      0,          TRUE,
  "coarse with far guess",    99,     "Clean", "Compounded", 2L,    settlement_normal,  1e-3,      100L,      0.2,        TRUE,
  "coarse and negative",     130,    "Clean", "Continuous", 1L,    settlement_normal,  1e-2,      100L,      0.05,       TRUE,
  "guess is the root",    99,     "Clean", "Compounded", 2L,    settlement_normal,  1e-10,     100L,      reference,  TRUE,
  "high price",           200,    "Clean", "Continuous", 1L,    settlement_normal,  1e-10,     100L,      0,          TRUE,
  "low price",            20,     "Clean", "Continuous", 1L,    settlement_normal,  1e-10,     100L,      0,          TRUE,
  "very low price",       1,      "Clean", "Continuous", 1L,    settlement_normal,  1e-10,     1000L,     0,          TRUE,
  "price of zero",        0,      "Dirty", "Continuous", 1L,    settlement_normal,  1e-10,     100L,      0,          TRUE,
  "negative price",       -5,     "Dirty", "Continuous", 1L,    settlement_normal,  1e-10,     100L,      0,          TRUE,
  "simple with high price",   130,    "Clean", "Simple",     1L,    settlement_normal,  1e-10,     100L,      0,          TRUE,
  "simple with low price",    60,     "Clean", "Simple",     1L,    settlement_normal,  1e-10,     100L,      0,          TRUE,
  "quarterly",            99,     "Clean", "Compounded", 4L,    settlement_normal,  1e-10,     100L,      0,          TRUE,
  "one evaluation",       99,     "Clean", "Compounded", 2L,    settlement_normal,  1e-10,     1L,        0,          TRUE,
  "two evaluations",      20,     "Clean", "Continuous", 1L,    settlement_normal,  1e-10,     2L,        0,          TRUE,
  "three evaluations",    99,     "Clean", "Compounded", 2L,    settlement_normal,  1e-10,     3L,        0,          TRUE,
  "five evaluations",     20,     "Clean", "Continuous", 1L,    settlement_normal,  1e-10,     5L,        0,          TRUE,
  "settlement on coupon", 99,     "Clean", "Compounded", 2L,    serial("2027-03-23"), 1e-10,   100L,      0,          TRUE,
  "settlement at the end", 99,    "Clean", "Compounded", 2L,    maturity,           1e-10,     100L,      0,          TRUE,
  "settlement after end", 99,     "Clean", "Compounded", 2L,    maturity + 10L,     1e-10,     100L,      0,          TRUE,
  "before issue",         99,     "Clean", "Compounded", 2L,    issue - 10L,        1e-10,     100L,      0,          TRUE,
  "accuracy of zero",     99,     "Clean", "Compounded", 2L,    settlement_normal,  0,         100L,      0,          TRUE,
  "short with extrapolation", 99,   "Clean", "Compounded", 2L,    settlement_normal,  1e-10,     100L,      0,          TRUE,
  "no extrapolation",     99,     "Clean", "Compounded", 2L,    settlement_normal,  1e-10,     100L,      0,          FALSE
)

frequency_name <- c(`1` = "Annual", `2` = "Semiannual", `4` = "Quarterly")
path <- file.path("tests", "golden", "z_spread_edges.csv")
writeLines(c(
  paste0("# QuantLib R package ", packageVersion("QuantLib"), "; synthetic inputs; tools/golden/p11_z_spread_edges.R; bond 2024-03-20 to 2031-03-20, 1.5%, T+2, Japan, Actual/365; evaluation ", evaluation, "; nodes ", nodes),
  paste(c("label", "price", "type", "compounding", "frequency", "settlement", "accuracy", "max_evaluations", "guess", "extrapolate", "status", "z_spread", "message"), collapse = ",")
), path)

for (i in seq_len(nrow(cases))) {
  x <- cases[i, ]
  message_text <- ""
  z <- tryCatch(
    QuantLib::BondFunctions_zSpread(bond, QuantLib::BondPrice(x$price, x$type), curve_of(x$extrapolate, if (grepl("extrapolation", x$label)) 730L else 2190L), day_count, x$comp,
                                    frequency_name[[as.character(x$freq)]], at(x$settle), x$accuracy, x$max_eval, x$guess),
    error = function(e) {
      message_text <<- gsub("[,\n]", ";", sub("^SWIG:RuntimeError ", "", conditionMessage(e)))
      NA_real_
    }
  )
  status <- if (is.na(z)) "failed" else "ok"
  write(paste(c(x$label, fmt(x$price), x$type, x$comp, x$freq, x$settle, fmt(x$accuracy), x$max_eval, fmt(x$guess), as.integer(x$extrapolate),
                status, if (is.na(z)) "NA" else fmt(z), substr(message_text, 1, 120)), collapse = ","), path, append = TRUE)
  cat(sprintf("%-24s %-7s %s %s\n", x$label, status, if (is.na(z)) "" else fmt(z), substr(message_text, 1, 90)))
}
