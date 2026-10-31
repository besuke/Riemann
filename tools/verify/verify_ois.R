# Compares Riemann TONAR OIS curves with the QuantLib R package Version 1.42 on synthetic quotes,
# for log-linear and monotonic log-cubic interpolation of the discount factors. The exact C++
# commit of the package is unknown, so this is not a comparison with running QuantLib C++ v1.42.
#
# Riemann is called through crates/riemann/examples/verify_ois.rs, until the R entry point of P4.
#
# Usage from an R session started anywhere in the repository:
#   source("tools/verify/verify_common.R"); source("tools/verify/verify_ois.R")
#   results <- verify_ois(); print(summarise_ois(results))

ois_interpolations <- c("LogLinear", "MonotonicLogCubic")

# Rates: absolute 1e-10 (specification 7.3). The forward at a date divides by 0.0001 years, so
# the relative 1e-12 of discount factors becomes 1e-8.
ois_tolerances <- function() {
  tribble(
    ~kind,                   ~tolerance, ~diff_type,
    "pillar_date",           0,          "absolute",
    "node_discount",         1e-12,      "relative",
    "discount",              1e-12,      "relative",
    "zero_continuous",       1e-10,      "absolute",
    "forward_1d_simple",     1e-10,      "absolute",
    "forward_instantaneous", 1e-8,       "absolute",
    "implied_rate",          1e-10,      "absolute"
  )
}
quote_reproduction_tolerance <- function() 1e-9

# ---- synthetic inputs -------------------------------------------------------------------

ois_tenors <- function() {
  tibble(
    label = c("ON", "1W", "1M", "3M", "6M", "1Y", "18M", "2Y", "3Y", "5Y", "7Y", "10Y", "15Y", "20Y", "30Y"),
    years = c(1 / 365, 7 / 365, 1 / 12, 0.25, 0.5, 1, 1.5, 2, 3, 5, 7, 10, 15, 20, 30)
  )
}

# Four curve shapes. Rates are made up from formulas; no market data is used.
make_ois_scenarios <- function(seed = 20261121L) {
  set.seed(seed)
  tenors <- ois_tenors()
  zigzag <- runif(nrow(tenors), -0.002, 0.002)
  tribble(
    ~case, ~scenario,  ~evaluation_date, ~settlement_days, ~payment_lag, ~payment_frequency, ~rates,
    1L,    "Upward",   "2026-03-17",     2L,               0L,           1L,                 0.002 + 0.02 * (1 - exp(-tenors$years / 7)),
    2L,    "Inverted", "2026-04-29",     2L,               2L,           1L,                 0.03 - 0.015 * (1 - exp(-tenors$years / 5)),
    3L,    "Humped",   "2024-02-29",     2L,               0L,           2L,                 0.005 + 0.03 * (tenors$years / 5) * exp(1 - tenors$years / 5),
    4L,    "Jagged",   "2026-07-31",     2L,               2L,           1L,                 0.002 + 0.02 * (1 - exp(-tenors$years / 7)) + zigzag
  ) |>
    mutate(
      evaluation = date_to_serial(as.Date(evaluation_date)),
      reference = evaluation,
      day_counter = "actual_365_fixed",
      extrapolate = 0L,
      rates = map(rates, \(r) round(r, 5)),
      instruments = map_chr(rates, \(r) paste(paste0(tenors$label, ":", format_number(r)), collapse = " "))
    )
}

# ---- QuantLib ---------------------------------------------------------------------------

ql_period <- function(label) {
  units <- c(W = "Weeks", M = "Months", Y = "Years")
  n <- as.integer(str_remove(label, "[WMY]$"))
  QuantLib::Period(n, units[[str_remove(label, "^[0-9]+")]])
}

ql_rate_helpers <- function(scenario) {
  frequency <- c(`1` = QuantLib::Frequency_Annual_get(), `2` = QuantLib::Frequency_Semiannual_get())[[as.character(scenario$payment_frequency)]]
  index <- QuantLib::Tonar()
  map2(ois_tenors()$label, scenario$rates[[1]], \(label, rate) {
    quote <- QuantLib::QuoteHandle(QuantLib::SimpleQuote(rate))
    if (label == "ON") return(QuantLib::DepositRateHelper(quote, index))
    QuantLib::OISRateHelper(scenario$settlement_days, ql_period(label), quote, index, QuantLib::YieldTermStructureHandle(),
                            FALSE, scenario$payment_lag, "Following", frequency)
  })
}

set_ql_evaluation_date <- function(scenario) {
  QuantLib::Settings_instance()$setEvaluationDate(ql_date(scenario$evaluation))
}

ql_serial <- function(date) as.integer(QuantLib::Date_serialNumber(date))

# PiecewiseLogCubicDiscount uses Spline, monotonic, second derivative 0 at both ends: the
# settings of C++ MonotonicLogCubic (docs/spec_ois.md 2.7). ois_method_check tests this.
ql_ois_curve <- function(scenario, interpolation, helpers) {
  vector <- QuantLib::RateHelperVector()
  walk(helpers, \(h) QuantLib::RateHelperVector_append(vector, h))
  build <- switch(interpolation,
    LogLinear = QuantLib::PiecewiseLogLinearDiscount,
    MonotonicLogCubic = QuantLib::PiecewiseLogCubicDiscount
  )
  build(ql_date(scenario$reference), vector, QuantLib::Actual365Fixed())
}

# Dates where the curves are read: weekly for a year, then every 30 days to before the last pillar.
ois_probes <- function(scenario) {
  set_ql_evaluation_date(scenario)
  last <- max(map_int(ql_rate_helpers(scenario), \(h) ql_serial(h$pillarDate())))
  from <- scenario$reference
  sort(unique(c(from, seq(from, min(from + 365L, last - 1L), by = 7L), seq(from + 365L, last - 1L, by = 30L))))
}

quantlib_ois_values <- function(scenario, interpolation, probes) {
  set_ql_evaluation_date(scenario)
  helpers <- ql_rate_helpers(scenario)
  curve <- ql_ois_curve(scenario, interpolation, helpers)
  dc <- QuantLib::Actual365Fixed()
  built <- tryCatch({ curve$maxDate(); TRUE }, error = function(e) FALSE)
  row <- function(kind, key, value) tibble(case = scenario$case, interpolation = interpolation, kind = kind, key = as.integer(key), value = value)
  if (!built) return(row("build", 0L, NA_real_))
  value <- function(expr) tryCatch(expr, error = function(e) NA_real_)
  rate <- function(expr) value(QuantLib::InterestRate_rate(expr))
  positions <- seq_along(helpers) - 1L
  node_dates <- curve$dates()
  node_values <- as.numeric(curve$data())
  node_serials <- map_int(seq_along(node_values), \(i) ql_serial(QuantLib::DateVector___getitem__(node_dates, i - 1L)))
  probe_rows <- map(probes, \(serial) {
    date <- ql_date(serial)
    bind_rows(
      row("discount", serial, value(QuantLib::YieldTermStructure_discount(curve, date))),
      row("zero_continuous", serial, rate(QuantLib::YieldTermStructure_zeroRate(curve, date, dc, "Continuous", "Annual"))),
      row("forward_1d_simple", serial, rate(QuantLib::YieldTermStructure_forwardRate(curve, date, ql_date(serial + 1L), dc, "Simple", "Annual"))),
      row("forward_instantaneous", serial, rate(QuantLib::YieldTermStructure_forwardRate(curve, date, date, dc, "Continuous", "Annual")))
    )
  })
  bind_rows(
    row("implied_rate", positions, map_dbl(helpers, \(h) h$impliedQuote())),
    row("pillar_date", positions, map_dbl(helpers, \(h) ql_serial(h$pillarDate()))),
    row("node_discount", node_serials, node_values),
    list_rbind(probe_rows)
  )
}

# Runs QuantLib's own MonotonicLogCubicDiscountCurve through the bootstrapped nodes, to confirm
# that PiecewiseLogCubicDiscount interpolates as C++ MonotonicLogCubic does.
ois_method_check <- function(scenario, probes) {
  set_ql_evaluation_date(scenario)
  curve <- ql_ois_curve(scenario, "MonotonicLogCubic", ql_rate_helpers(scenario))
  reference_curve <- QuantLib::MonotonicLogCubicDiscountCurve(curve$dates(), curve$data(), QuantLib::Actual365Fixed())
  differences <- map_dbl(probes, \(serial) {
    a <- QuantLib::YieldTermStructure_discount(curve, ql_date(serial))
    b <- QuantLib::YieldTermStructure_discount(reference_curve, ql_date(serial))
    abs(a - b) / abs(b)
  })
  tibble(scenario = scenario$scenario, probes = length(probes), max_rel_diff = max(differences))
}

# ---- Riemann ----------------------------------------------------------------------------

riemann_ois_values <- function(runs, root = find_riemann_root()) {
  input_path <- tempfile(fileext = ".csv")
  output_path <- tempfile(fileext = ".csv")
  on.exit(unlink(c(input_path, output_path)), add = TRUE)
  runs |>
    transmute(case, evaluation, reference, day_counter, extrapolate, settlement_days, payment_lag, payment_frequency,
              instruments, interpolation, probes = map_chr(probes, \(p) paste(p, collapse = " "))) |>
    write_csv(input_path)
  run_riemann_example("verify_ois", input_path, output_path, root)
  read_csv(output_path, col_types = cols(.default = col_character())) |>
    mutate(case = as.integer(case), key = as.integer(key), value = to_number(value))
}

# ---- comparison -------------------------------------------------------------------------

compare_ois_values <- function(quantlib, riemann) {
  full_join(rename(quantlib, quantlib = value), rename(riemann, riemann = value),
            by = c("case", "interpolation", "kind", "key")) |>
    left_join(ois_tolerances(), by = "kind") |>
    mutate(
      abs_diff = abs(riemann - quantlib),
      rel_diff = if_else(quantlib != 0, abs_diff / abs(quantlib), abs_diff),
      measured_diff = if_else(diff_type == "relative", rel_diff, abs_diff),
      status = case_when(
        is.na(quantlib) & is.na(riemann) ~ "both error",
        is.na(quantlib) | is.na(riemann) ~ "one-sided error",
        .default = "value"
      ),
      pass = case_when(
        status == "both error" ~ TRUE,
        status == "one-sided error" ~ FALSE,
        .default = measured_diff <= tolerance
      )
    )
}

ois_quote_reproduction <- function(scenarios, comparison) {
  tenors <- ois_tenors() |> mutate(key = row_number() - 1L)
  scenarios |>
    transmute(case, scenario = factor(scenario, scenario), rates, position = map(rates, \(r) seq_along(r) - 1L)) |>
    unnest(c(rates, position)) |>
    rename(quote = rates, key = position) |>
    left_join(tenors, by = "key") |>
    left_join(filter(comparison, kind == "implied_rate") |> select(case, interpolation, key, quantlib, riemann),
              by = c("case", "key"), relationship = "many-to-many") |>
    mutate(
      error_quantlib = abs(quantlib - quote),
      error_riemann = abs(riemann - quote),
      tolerance = quote_reproduction_tolerance(),
      pass = !is.na(error_riemann) & error_riemann <= tolerance
    )
}

summarise_ois <- function(results) {
  results$comparison |>
    group_by(interpolation, kind, tolerance, diff_type) |>
    summarise(
      compared = n(),
      exact = sum(status == "value" & abs_diff == 0, na.rm = TRUE),
      within_tolerance = sum(status == "value" & abs_diff > 0 & pass, na.rm = TRUE),
      both_error = sum(status == "both error"),
      failed = sum(!pass),
      max_diff = if (any(status == "value")) max(measured_diff[status == "value"]) else NA_real_,
      .groups = "drop"
    ) |>
    mutate(result = if_else(failed == 0, "PASS", "FAIL"))
}

# Riemann values by interpolation, one row per date, for the figures and the method comparison.
ois_curves <- function(results) {
  results$comparison |>
    filter(kind %in% c("discount", "zero_continuous", "forward_1d_simple")) |>
    left_join(select(results$scenarios, case, scenario, reference), by = "case") |>
    transmute(scenario = factor(scenario, results$scenarios$scenario), interpolation, kind, key,
              years = (key - reference) / 365, value = riemann)
}

compare_interpolations <- function(results) {
  curves <- ois_curves(results)
  differences <- curves |>
    filter(kind != "discount") |>
    pivot_wider(names_from = interpolation, values_from = value) |>
    mutate(bp = (MonotonicLogCubic - LogLinear) * 1e4)
  by_shape <- differences |>
    group_by(scenario, kind) |>
    summarise(max_abs_bp = max(abs(bp), na.rm = TRUE), .groups = "drop") |>
    pivot_wider(names_from = kind, values_from = max_abs_bp, names_prefix = "max_gap_bp_")
  # the largest move of the 1-day forward between neighbouring read dates shows how rough it is
  roughness <- curves |>
    filter(kind == "forward_1d_simple") |>
    arrange(scenario, interpolation, key) |>
    group_by(scenario, interpolation) |>
    summarise(jump_bp = max(abs(diff(value)), na.rm = TRUE) * 1e4, min_forward = min(value, na.rm = TRUE), .groups = "drop") |>
    pivot_wider(names_from = interpolation, values_from = c(jump_bp, min_forward))
  list(differences = differences, summary = left_join(by_shape, roughness, by = "scenario"))
}

verify_ois <- function(root = find_riemann_root()) {
  message("Building synthetic TONAR scenarios ...")
  scenarios <- make_ois_scenarios()
  scenarios$probes <- map(seq_len(nrow(scenarios)), \(i) ois_probes(scenarios[i, ]))
  runs <- tidyr::crossing(scenarios, interpolation = ois_interpolations) |> arrange(case, interpolation)
  message("Bootstrapping ", nrow(runs), " curves with QuantLib R package ",
          as.character(packageVersion("QuantLib")), " (exact C++ commit unknown) ...")
  quantlib <- map(seq_len(nrow(runs)), \(i) {
    run <- runs[i, ]
    quantlib_ois_values(run, run$interpolation, run$probes[[1]])
  }) |> list_rbind()
  message("Checking PiecewiseLogCubicDiscount against MonotonicLogCubicDiscountCurve ...")
  method_check <- map(seq_len(nrow(scenarios)), \(i) ois_method_check(scenarios[i, ], scenarios$probes[[i]])) |> list_rbind()
  message("Bootstrapping the same curves with Riemann ...")
  riemann <- riemann_ois_values(runs, root)
  comparison <- compare_ois_values(quantlib, riemann)
  results <- list(scenarios = scenarios, comparison = comparison, method_check = method_check,
                  quotes = ois_quote_reproduction(scenarios, comparison))
  summary <- summarise_ois(results)
  walk(seq_len(nrow(summary)), \(i) {
    with(summary[i, ], message(sprintf("%-18s %-22s %s  compared %d  failed %d  max diff %.2e  tolerance %.0e",
                                       interpolation, kind, result, compared, failed, max_diff, tolerance)))
  })
  message(sprintf("Quote reproduction: max error %.2e, tolerance %.0e, failed %d",
                  max(results$quotes$error_riemann, na.rm = TRUE), quote_reproduction_tolerance(), sum(!results$quotes$pass)))
  results
}
