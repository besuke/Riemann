# Compares Riemann bond prices, yields and yield risk with the QuantLib R package Version 1.42
# on synthetic inputs. The exact C++ commit of the package is unknown, so this is not a
# comparison with running QuantLib C++ v1.42 itself.
#
# Riemann is called directly through its R package RiemannFinMath (crates/riemann-r). The example program
# crates/riemann/examples/verify_bonds.rs stays as a second path, to compare the two.
#
# Usage from an R session started anywhere in the repository:
#   source("tools/verify/verify_common.R"); source("tools/verify/verify_bonds.R")
#   results <- verify_bonds(); print(summarise_verification(results))

# The tolerances of docs/specification.md 5.1, as applied in docs/spec_bonds.md 3.3.
bond_tolerances <- function() {
  tibble(
    measure = c("clean_price", "dirty_price", "accrued", "duration_modified", "convexity", "bpv", "yield"),
    kind = c(rep("relative", 6), "absolute")
  )
}

tolerance_of <- function(measure, compounding) {
  case_when(
    measure == "yield" ~ 1e-10,
    measure == "accrued" | compounding == "Simple" ~ 1e-15,
    .default = 1e-12
  )
}

# ---- synthetic inputs -------------------------------------------------------------------

# The first 20 columns are those of tests/golden/bond_cases.csv, so the Rust side can share
# the parsing used by the golden tests.
make_bond_inputs <- function(n_bonds = 60L, scenarios_per_bond = 3L, seed = 20261110L) {
  set.seed(seed)
  bonds <- tibble(
    case = seq_len(n_bonds),
    start = as.Date("2012-01-01") + sample(0:4000, n_bonds, replace = TRUE),
    years = sample(c(2L, 3L, 5L, 7L, 10L, 20L, 30L), n_bonds, replace = TRUE),
    tenor_n = sample(c(3L, 6L, 12L), n_bonds, replace = TRUE),
    calendar = sample(c("japan", "target", "us_sofr", "null_calendar"), n_bonds, replace = TRUE),
    convention = sample(c("Following", "ModifiedFollowing", "Unadjusted"), n_bonds, replace = TRUE),
    coupon = sample(c(0, 0.001, 0.0125, 0.03, 0.05, 0.08), n_bonds, replace = TRUE),
    day_counter = sample(c("actual_365_fixed", "actual_360", "thirty_360_bond_basis", "act_act_isma", "act_act_isda"),
                         n_bonds, replace = TRUE),
    redemption = sample(c(100, 100, 101.5), n_bonds, replace = TRUE),
    settlement_days = sample(0:3, n_bonds, replace = TRUE),
    face_amount = sample(c(100, 1e6), n_bonds, replace = TRUE)
  ) |>
    mutate(
      effective = date_to_serial(start),
      termination = map2_int(start, years, \(d, y) date_to_serial(seq(d, by = paste(y, "years"), length.out = 2)[2])),
      tenor_unit = "Months",
      termination_convention = convention,
      rule = "Backward",
      end_of_month = 0L,
      first_date = 0L,
      next_to_last_date = 0L,
      payment_convention = "Following",
      issue_date = effective,
      built = 1L
    )

  map(bonds$case, \(id) tibble(case = id, scenario = seq_len(scenarios_per_bond))) |>
    list_rbind() |>
    left_join(bonds, by = "case") |>
    mutate(
      row = row_number(),
      evaluation_date = pmap_int(list(effective, termination), \(from, to) from + sample.int(max(1L, to - from - 40L), 1)),
      rate = sample(c(-0.005, 0.0001, 0.015, 0.04, 0.12), n(), replace = TRUE),
      yield_day_counter = sample(c("actual_365_fixed", "actual_360", "thirty_360_bond_basis", "act_act_isma"), n(), replace = TRUE),
      compounding = sample(c("Simple", "Compounded", "Compounded", "Continuous"), n(), replace = TRUE),
      frequency = if_else(compounding == "Compounded", sample(c(1L, 2L, 4L, 12L), n(), replace = TRUE), 1L),
      price = round(runif(n(), 85, 115), 3),
      price_type = sample(c("Clean", "Dirty"), n(), replace = TRUE)
    )
}

# ---- QuantLib ---------------------------------------------------------------------------

quantlib_day_counter <- function(name) {
  switch(name,
    actual_365_fixed = QuantLib::Actual365Fixed(),
    actual_360 = QuantLib::Actual360(),
    thirty_360_bond_basis = QuantLib::Thirty360("BondBasis"),
    act_act_isma = QuantLib::ActualActual("ISMA"),
    act_act_isda = QuantLib::ActualActual("ISDA")
  )
}

quantlib_calendar <- function(name) {
  switch(name,
    japan = QuantLib::Japan(),
    target = QuantLib::TARGET(),
    us_sofr = QuantLib::UnitedStates("SOFR"),
    null_calendar = QuantLib::NullCalendar()
  )
}

quantlib_frequency_name <- function(frequency) {
  c(`1` = "Annual", `2` = "Semiannual", `4` = "Quarterly", `12` = "Monthly")[[as.character(frequency)]]
}

quantlib_bond_row <- function(input) {
  value <- function(expr) tryCatch(expr, error = function(e) NA_real_)
  schedule <- QuantLib::Schedule(
    ql_date(input$effective), ql_date(input$termination), QuantLib::Period(input$tenor_n, input$tenor_unit),
    quantlib_calendar(input$calendar), input$convention, input$termination_convention, input$rule, FALSE,
    QuantLib::Date(), QuantLib::Date()
  )
  bond <- QuantLib::FixedRateBond(
    input$settlement_days, input$face_amount, schedule, input$coupon, quantlib_day_counter(input$day_counter),
    input$payment_convention, input$redemption, ql_date(input$issue_date)
  )
  QuantLib::Settings_instance()$setEvaluationDate(ql_date(input$evaluation_date))
  settlement <- QuantLib::Bond_settlementDate(bond)
  dc <- quantlib_day_counter(input$yield_day_counter)
  frequency <- quantlib_frequency_name(input$frequency)
  clean <- value(QuantLib::BondFunctions_cleanPrice(bond, input$rate, dc, input$compounding, frequency, settlement))
  accrued <- value(QuantLib::Bond_accruedAmount(bond, settlement))
  tibble(
    row = input$row,
    settlement_date = as.integer(QuantLib::Date_serialNumber(settlement)),
    clean_price = clean,
    # the R package has no matching dirtyPrice overload; QuantLib defines clean = dirty - accrued
    dirty_price = clean + accrued,
    accrued = accrued,
    duration_modified = value(QuantLib::BondFunctions_duration(bond, input$rate, dc, input$compounding, frequency, "Modified", settlement)),
    convexity = value(QuantLib::BondFunctions_convexity(bond, input$rate, dc, input$compounding, frequency, settlement)),
    bpv = value(QuantLib::BondFunctions_basisPointValue(bond, input$rate, dc, input$compounding, frequency, settlement)),
    yield = value(QuantLib::BondFunctions_yield(bond, QuantLib::BondPrice(input$price, input$price_type), dc, input$compounding, frequency, settlement))
  )
}

quantlib_bond_values <- function(inputs) {
  inputs |>
    split(seq_len(nrow(inputs))) |>
    map(quantlib_bond_row) |>
    list_rbind()
}

# ---- Riemann ----------------------------------------------------------------------------

snake_name <- function(x) tolower(gsub("([a-z])([A-Z])", "\\1_\\2", x))

riemann_bond_row <- function(input) {
  failed <- tibble(row = input$row, settlement_date = NA_integer_, clean_price = NA_real_, dirty_price = NA_real_, accrued = NA_real_,
                   duration_modified = NA_real_, convexity = NA_real_, bpv = NA_real_, yield = NA_real_)
  bond <- tryCatch(
    RiemannFinMath::fixed_rate_bond(
      settlement_days = input$settlement_days, face_amount = input$face_amount,
      effective_date = serial_to_date(input$effective), termination_date = serial_to_date(input$termination),
      tenor = paste0(input$tenor_n, "M"), calendar = input$calendar, convention = snake_name(input$convention),
      termination_convention = snake_name(input$termination_convention), rule = snake_name(input$rule),
      end_of_month = input$end_of_month == 1L, coupon_rate = input$coupon, day_count = input$day_counter,
      payment_convention = snake_name(input$payment_convention), redemption = input$redemption,
      issue_date = serial_to_date(input$issue_date)
    ),
    error = function(e) NULL
  )
  if (is.null(bond)) return(failed)
  settlement <- tryCatch(RiemannFinMath::settlement_date(bond, serial_to_date(input$evaluation_date)), error = function(e) NULL)
  if (is.null(settlement)) return(failed)
  compounding <- tolower(input$compounding)
  values <- RiemannFinMath::bond_values(bond, input$rate, input$yield_day_counter, compounding, input$frequency, settlement, on_error = "na")
  tibble(
    row = input$row, settlement_date = date_to_serial(settlement), clean_price = values$clean_price, dirty_price = values$dirty_price,
    accrued = values$accrued_amount, duration_modified = values$duration_modified, convexity = values$convexity,
    bpv = values$basis_point_value,
    yield = suppressWarnings(RiemannFinMath::yield_from_price(bond, input$price, tolower(input$price_type), input$yield_day_counter,
                                                       compounding, input$frequency, settlement, on_error = "na"))
  )
}

riemann_bond_values <- function(inputs) {
  require_riemann_package()
  inputs |>
    split(seq_len(nrow(inputs))) |>
    map(riemann_bond_row) |>
    list_rbind()
}

riemann_bond_values_csv <- function(inputs, root = find_riemann_root()) {
  input_path <- tempfile(fileext = ".csv")
  output_path <- tempfile(fileext = ".csv")
  on.exit(unlink(c(input_path, output_path)), add = TRUE)
  inputs |>
    mutate(across(c(coupon, rate, redemption, face_amount, price), format_number)) |>
    select(case, effective, termination, tenor_n, tenor_unit, calendar, convention, termination_convention, rule,
           end_of_month, first_date, next_to_last_date, coupon, day_counter, payment_convention, redemption,
           settlement_days, face_amount, issue_date, built, evaluation_date, rate, yield_day_counter, compounding,
           frequency, price, price_type, row) |>
    write_csv(input_path)
  run_riemann_example("verify_bonds", input_path, output_path, root)
  read_csv(output_path, col_types = cols(.default = col_character())) |>
    mutate(across(everything(), to_number)) |>
    mutate(row = as.integer(row), settlement_date = as.integer(settlement_date))
}

# ---- comparison -------------------------------------------------------------------------

compare_bond_values <- function(inputs, quantlib, riemann) {
  to_long <- function(values, name) {
    values |>
      select(-settlement_date) |>
      pivot_longer(-row, names_to = "measure", values_to = name)
  }
  settlement_mismatches <- quantlib |>
    inner_join(riemann, by = "row", suffix = c("_q", "_r")) |>
    filter(settlement_date_q != settlement_date_r)

  comparison <- to_long(quantlib, "quantlib") |>
    inner_join(to_long(riemann, "riemann"), by = c("row", "measure")) |>
    left_join(select(inputs, row, compounding), by = "row") |>
    left_join(bond_tolerances(), by = "measure") |>
    mutate(
      tolerance = tolerance_of(measure, compounding),
      abs_diff = abs(riemann - quantlib),
      rel_diff = if_else(quantlib != 0, abs_diff / abs(quantlib), abs_diff),
      measured_diff = if_else(kind == "absolute", abs_diff, rel_diff),
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
  list(comparison = comparison, settlement_mismatches = settlement_mismatches)
}

summarise_verification <- function(results) {
  results$comparison |>
    group_by(measure, tolerance, kind) |>
    # within a group the failures are counted in the row of its tolerance
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

verify_bonds <- function(n_bonds = 60L, scenarios_per_bond = 3L, seed = 20261110L, root = find_riemann_root()) {
  message("Building synthetic bond inputs ...")
  inputs <- make_bond_inputs(n_bonds, scenarios_per_bond, seed)
  message("Valuing ", nrow(inputs), " scenarios with QuantLib R package ",
          as.character(packageVersion("QuantLib")), " (exact C++ commit unknown) ...")
  quantlib <- quantlib_bond_values(inputs)
  message("Valuing the same scenarios with Riemann, called directly ...")
  riemann <- riemann_bond_values(inputs)
  message("Comparing the direct call with the CSV path ...")
  riemann_csv <- riemann_bond_values_csv(inputs, root)
  long <- function(values) pivot_longer(select(values, -settlement_date), -row, names_to = "measure", values_to = "value")
  via_csv <- compare_direct_with_csv(long(riemann), long(riemann_csv), c("row", "measure"))
  results <- c(list(inputs = inputs, quantlib = quantlib, riemann = riemann, direct_vs_csv = via_csv), compare_bond_values(inputs, quantlib, riemann))
  summary <- summarise_verification(results)
  walk(seq_len(nrow(summary)), \(i) {
    with(summary[i, ], message(sprintf("%-18s %s  compared %d  failed %d  max diff %.2e  tolerance %.0e",
                                       measure, result, compared, failed, max_diff, tolerance)))
  })
  message("Settlement date mismatches: ", nrow(results$settlement_mismatches))
  results
}
