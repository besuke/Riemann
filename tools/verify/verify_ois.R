# Compares Riemann OIS curves (TONAR, ESTR, SOFR, SONIA) with the QuantLib R package Version 1.42 on synthetic quotes,
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

# Curve shapes made up from formulas; no market data is used. The conventions are explicit values
# for the comparison: settlement days are QuantLib's MakeOIS defaults, not confirmed market
# conventions (docs/spec_ois.md 2.10). Evaluation dates include holidays of each calendar.
make_ois_scenarios <- function(seed = 20261121L) {
  set.seed(seed)
  tenors <- ois_tenors()
  zigzag <- runif(nrow(tenors), -0.002, 0.002)
  shapes <- list(
    Upward = 0.002 + 0.02 * (1 - exp(-tenors$years / 7)),
    Inverted = 0.03 - 0.015 * (1 - exp(-tenors$years / 5)),
    Humped = 0.005 + 0.03 * (tenors$years / 5) * exp(1 - tenors$years / 5),
    Jagged = 0.002 + 0.02 * (1 - exp(-tenors$years / 7)) + zigzag
  )
  tribble(
    ~index,  ~shape,     ~evaluation_date, ~settlement_days, ~payment_lag, ~payment_frequency,
    "TONAR", "Upward",   "2026-03-17",     2L,               0L,           1L,
    "TONAR", "Inverted", "2026-04-29",     2L,               2L,           1L,
    "TONAR", "Humped",   "2024-02-29",     2L,               0L,           2L,
    "TONAR", "Jagged",   "2026-07-31",     2L,               2L,           1L,
    "ESTR",  "Upward",   "2026-03-17",     2L,               1L,           1L,
    "ESTR",  "Jagged",   "2026-12-25",     2L,               1L,           1L,
    "SOFR",  "Upward",   "2026-03-17",     2L,               2L,           1L,
    "SOFR",  "Jagged",   "2026-04-03",     2L,               2L,           1L,
    "SONIA", "Upward",   "2026-03-17",     0L,               0L,           1L,
    "SONIA", "Jagged",   "2026-05-04",     0L,               0L,           1L
  ) |>
    mutate(
      case = row_number(),
      scenario = paste(index, shape),
      rates = map(shape, \(name) round(shapes[[name]], 5)),
      evaluation = date_to_serial(as.Date(evaluation_date)),
      reference = evaluation,
      day_counter = "actual_365_fixed",
      extrapolate = 0L,
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
  index <- switch(scenario$index, TONAR = QuantLib::Tonar(), ESTR = QuantLib::Estr(), SOFR = QuantLib::Sofr(), SONIA = QuantLib::Sonia())
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
              instruments, interpolation, probes = map_chr(probes, \(p) paste(p, collapse = " ")), index) |>
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
      diff_riemann_quantlib = abs(riemann - quantlib),
      tolerance = quote_reproduction_tolerance(),
      pass = !is.na(error_riemann) & error_riemann <= tolerance,
      pass_quantlib = !is.na(error_quantlib) & error_quantlib <= tolerance
    )
}

summarise_ois <- function(results) {
  results$comparison |>
    left_join(select(results$scenarios, case, index), by = "case") |>
    mutate(index = factor(index, unique(results$scenarios$index))) |>
    group_by(index, interpolation, kind, tolerance, diff_type) |>
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

# ---- all golden curves ----------------------------------------------------------------

golden_ois_sets <- function() {
  tribble(
    ~index,  ~prefix,
    "TONAR", "ois_tonar_log_cubic",
    "ESTR",  "ois_estr_log_cubic",
    "SOFR",  "ois_sofr_log_cubic",
    "SONIA", "ois_sonia_log_cubic"
  )
}

read_golden_part <- function(root, prefix, part) {
  read_csv(file.path(root, "tests", "golden", paste0(prefix, "_", part, ".csv")), comment = "#",
           col_types = cols(.default = col_character()), na = character())
}

# The difference from QuantLib and both reproduction errors of every golden curve of one index.
# The tolerances are those of docs/spec_ois.md 10.4.
check_golden_index <- function(index, prefix, root) {
  cases <- read_golden_part(root, prefix, "cases")
  instruments <- read_golden_part(root, prefix, "instruments")
  nodes <- read_golden_part(root, prefix, "nodes")
  discounts <- read_golden_part(root, prefix, "discounts")
  probes <- discounts |>
    group_by(case) |>
    summarise(probes = paste(date, collapse = " "), .groups = "drop")
  input_path <- tempfile(fileext = ".csv")
  output_path <- tempfile(fileext = ".csv")
  on.exit(unlink(c(input_path, output_path)), add = TRUE)
  cases |>
    left_join(probes, by = "case") |>
    transmute(case, evaluation, reference, day_counter, extrapolate, settlement_days, payment_lag, payment_frequency,
              instruments, interpolation = "MonotonicLogCubic", probes = replace_na(probes, ""), index = index) |>
    write_csv(input_path)
  run_riemann_example("verify_ois", input_path, output_path, root)
  riemann <- read_csv(output_path, col_types = cols(.default = col_character()))

  # QuantLib errors are the status of the case; dates and causes are compared by cargo test
  built_riemann <- riemann |> filter(kind != "build") |> pull(case) |> unique()
  built_quantlib <- cases |> filter(status == "ok") |> pull(case)
  value_of <- function(kind_name) riemann |> filter(kind == kind_name) |> transmute(case, key, riemann = to_number(value))

  quotes <- cases |>
    filter(status == "ok") |>
    transmute(case, item = str_split(instruments, " ")) |>
    mutate(key = map(item, \(x) as.character(seq_along(x) - 1L))) |>
    unnest(c(item, key)) |>
    transmute(case, key, label = str_remove(item, ":.*$"), quote = as.numeric(str_remove(item, "^.*:")))
  implied <- instruments |>
    filter(implied_rate != "NA") |>
    transmute(case, key = position, quantlib = as.numeric(implied_rate)) |>
    inner_join(value_of("implied_rate"), by = c("case", "key")) |>
    inner_join(quotes, by = c("case", "key")) |>
    mutate(diff = abs(riemann - quantlib), error_quantlib = abs(quantlib - quote), error_riemann = abs(riemann - quote))
  node_diff <- nodes |>
    transmute(case, key = date, quantlib = as.numeric(discount)) |>
    inner_join(value_of("node_discount"), by = c("case", "key")) |>
    mutate(diff = abs(riemann - quantlib), over = diff > 1e-12 * abs(quantlib))
  discount_diff <- discounts |>
    filter(discount != "ERROR") |>
    transmute(case, key = date, quantlib = as.numeric(discount)) |>
    inner_join(value_of("discount"), by = c("case", "key")) |>
    mutate(diff = abs(riemann - quantlib), over = diff > 1e-12 * abs(quantlib))

  tolerance <- quote_reproduction_tolerance()
  misses <- implied |>
    filter(error_quantlib > tolerance | error_riemann > tolerance) |>
    group_by(case) |>
    summarise(
      miss_quotes = n(),
      labels = paste(label, collapse = ", "),
      max_error_riemann = max(error_riemann), max_error_quantlib = max(error_quantlib), max_diff = max(diff),
      .groups = "drop"
    ) |>
    left_join(node_diff |> group_by(case) |> summarise(lowest_discount = min(quantlib), .groups = "drop"), by = "case") |>
    mutate(index = index, .before = 1)

  comparison_ok <- setequal(built_riemann, built_quantlib) &&
    max(node_diff$diff) <= 2e-12 && max(discount_diff$diff) <= 2e-12 && max(implied$diff) <= 1e-10
  summary <- tibble(
    index = index, curves = nrow(cases), built = length(built_quantlib), both_error = nrow(cases) - length(built_quantlib),
    build_mismatches = length(setdiff(built_quantlib, built_riemann)) + length(setdiff(built_riemann, built_quantlib)),
    quotes = nrow(implied),
    max_diff_node = max(node_diff$diff), max_diff_discount = max(discount_diff$diff), max_diff_quote = max(implied$diff),
    comparison = if (comparison_ok) "PASS" else "FAIL",
    max_error_riemann = max(implied$error_riemann),
    miss_quotes_riemann = sum(implied$error_riemann > tolerance),
    miss_curves_riemann = n_distinct(implied$case[implied$error_riemann > tolerance]),
    max_error_quantlib = max(implied$error_quantlib),
    miss_quotes_quantlib = sum(implied$error_quantlib > tolerance),
    miss_curves_quantlib = n_distinct(implied$case[implied$error_quantlib > tolerance]),
    over_node = sum(node_diff$over), over_discount = sum(discount_diff$over),
    over_quote = sum(implied$diff > 1e-12 * abs(implied$quantlib))
  ) |>
    mutate(reproduction = if_else(miss_quotes_riemann == 0L, "基準達成", "基準未達"))
  list(summary = summary, misses = misses)
}

verify_ois_golden <- function(root = find_riemann_root()) {
  message("Checking every golden OIS curve with Riemann ...")
  checked <- pmap(golden_ois_sets(), \(index, prefix) check_golden_index(index, prefix, root))
  list(summary = list_rbind(map(checked, "summary")), misses = list_rbind(map(checked, "misses")))
}

verify_ois <- function(root = find_riemann_root()) {
  message("Building synthetic OIS scenarios ...")
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
    with(summary[i, ], message(sprintf("%-6s %-18s %-22s %s  compared %d  failed %d  max diff %.2e  tolerance %.0e",
                                       index, interpolation, kind, result, compared, failed, max_diff, tolerance)))
  })
  message(sprintf("Quote reproduction: max error %.2e, tolerance %.0e, failed %d",
                  max(results$quotes$error_riemann, na.rm = TRUE), quote_reproduction_tolerance(), sum(!results$quotes$pass)))
  results
}
