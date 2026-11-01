# Shared by every chapter of the QuantLib and RiemannFinMath comparison.
# Both libraries are always called through their namespaces, so each result names its source.
# Nothing here falls back from one library to the other.

suppressPackageStartupMessages(library(tidyverse))

comparison_tolerance <- 1e-4
lagrange_root <- "../../LagrangeFinance"

# ---- dates --------------------------------------------------------------------------------------
ql_date <- function(x) QuantLib::Date(as.integer(as.Date(x)) + 25569L)
ql_iso <- function(d) as.Date(QuantLib::Date_ISO(d))
ql_set_evaluation_date <- function(x) invisible(QuantLib::Settings_instance()$setEvaluationDate(ql_date(x)))
ql_date_vector <- function(v) map(seq_len(v$size()) - 1L, \(i) QuantLib::DateVector___getitem__(v, i))

# ---- recording ----------------------------------------------------------------------------------
results_store <- new.env()
results_store$rows <- tibble()

add_rows <- function(rows) {
  results_store$rows <- bind_rows(results_store$rows, rows)
  rows
}

# Judges the unrounded values. A row stays FAIL when it fails: the cause goes in the chapter's text.
compare_values <- function(item, unit, quantlib, riemann, baseline = "QuantLib C++", note = NA_character_) {
  tibble(item = item, unit = unit, baseline = baseline, baseline_value = quantlib, riemann_value = riemann) |>
    mutate(
      difference = riemann_value - baseline_value,
      tolerance = comparison_tolerance,
      verdict = if_else(abs(difference) <= tolerance, "PASS", "FAIL"),
      note = note
    ) |>
    add_rows()
}

# The largest absolute difference over a vector, without showing the values themselves.
compare_max <- function(item, unit, quantlib, riemann, baseline = "QuantLib C++", note = NA_character_) {
  largest <- max(abs(riemann - quantlib))
  tibble(item = item, unit = unit, baseline = baseline, baseline_value = NA_real_, riemann_value = NA_real_) |>
    mutate(
      difference = largest,
      tolerance = comparison_tolerance,
      verdict = if_else(difference <= tolerance, "PASS", "FAIL"),
      note = note
    ) |>
    add_rows()
}

# Two numbers that are not the same quantity under the same rules: shown, and not judged.
not_comparable <- function(item, unit, quantlib, riemann, reason, baseline = "QuantLib C++") {
  tibble(item = item, unit = unit, baseline = baseline, baseline_value = quantlib, riemann_value = riemann) |>
    mutate(difference = riemann_value - baseline_value, tolerance = NA_real_, verdict = "N/C", note = reason) |>
    add_rows()
}

format_4 <- function(x) {
  rounded <- round(x, 4)
  rounded[!is.na(rounded) & rounded == 0] <- 0 # no negative zero
  if_else(is.na(x), "-", formatC(rounded, format = "f", digits = 4, big.mark = ","))
}

show_results <- function(rows, caption = "Results (shown to 4 decimals; judged before rounding)") {
  shown <- rows |>
    transmute(
      Item = item, Unit = unit, Baseline = baseline,
      `Baseline value` = format_4(baseline_value), RiemannFinMath = format_4(riemann_value),
      Difference = format_4(difference), Tolerance = format_4(tolerance), Verdict = verdict, Note = note
    )
  if (all(is.na(shown$Note))) shown <- select(shown, -Note)
  knitr::kable(shown, caption = caption, align = c("l", "l", "l", "r", "r", "r", "r", "c", "l")[seq_along(shown)])
}

show_inputs <- function(inputs, caption = "Inputs") {
  tibble(Input = names(inputs), Value = map_chr(inputs, \(x) paste(as.character(x), collapse = ", "))) |>
    knitr::kable(caption = caption)
}

show_design <- function(rows, caption = "Differences of wrapper and core") {
  knitr::kable(rows, caption = caption)
}

finish_chapter <- function(chapter) {
  rows <- results_store$rows |> mutate(chapter = chapter, .before = 1)
  dir.create("results", showWarnings = FALSE)
  write_csv(rows, file.path("results", paste0(chapter, ".csv")))
  rows |>
    count(Verdict = verdict) |>
    knitr::kable(caption = paste("Verdicts in", chapter))
}

# ---- inputs of the legacy text, read and never copied ---------------------------------------------
# Evaluates one assignment of a LagrangeFinance chapter in a new environment, without changing the file.
lagrange_object <- function(chapter_file, name, ...) {
  lagrange_objects(chapter_file, name, ...)[[name]]
}

# The same for several assignments, in the order of the chapter and in one environment.
lagrange_objects <- function(chapter_file, names, ...) {
  source_lines <- readLines(file.path(lagrange_root, chapter_file), encoding = "UTF-8", warn = FALSE)
  starts <- which(startsWith(source_lines, "```{r"))
  ends <- map_int(starts, \(s) s + which(startsWith(source_lines[(s + 1):length(source_lines)], "```"))[1])
  code <- map2(starts, ends, \(s, e) source_lines[(s + 1):(e - 1)]) |> list_c()
  expressions <- as.list(parse(text = code, encoding = "UTF-8", keep.source = FALSE))
  environment <- new.env(parent = globalenv())
  list2env(list(...), environment)
  wanted <- keep(expressions, \(e) is.call(e) && identical(e[[1]], as.name("<-")) && as.character(e[[2]])[1] %in% names)
  found <- map_chr(wanted, \(e) as.character(e[[2]])[1])
  absent <- setdiff(names, found)
  if (length(absent)) stop("Not in ", chapter_file, ": ", paste(absent, collapse = ", "), call. = FALSE)
  walk(wanted, \(e) eval(e, environment))
  environment
}

# ---- curves with a given shape ------------------------------------------------------------------
# The public API builds curves from quotes only. This finds the quotes whose log-linear curve has a
# given continuous zero rate at each node. A constant rate makes the curve flat between nodes as well.
fitted_curve <- function(date, zero, tenors, curve_day_count = "actual_365_fixed", index = "tonar", iterations = 12L) {
  build <- function(quotes) {
    RiemannFinMath::build_ois_curve(
      date, date, index, curve_day_count, RiemannFinMath::ois_instruments(tenors, quotes, 0L, 1L, 0L),
      interpolation = "log_linear", accuracy = 1e-15
    )
  }
  # One Newton step on each quote. A long par quote moves a long zero rate by much more than itself,
  # so the error is scaled by the time and the discount factor over the annuity of a flat curve.
  step <- function(quotes) {
    curve <- build(quotes)
    nodes <- RiemannFinMath::discount_nodes(curve) |> slice(-1)
    error <- RiemannFinMath::zero_rate(curve, nodes$date, curve_day_count, "continuous") - zero
    time <- as.numeric(nodes$date - as.Date(date)) / 365
    scale <- if_else(abs(zero) < 1e-10, 1, time * exp(-zero * time) * expm1(zero) / -expm1(-zero * time))
    quotes - error * scale
  }
  reduce(seq_len(iterations), \(quotes, i) step(quotes), .init = expm1(zero)) |>
    build()
}

flat_curve <- function(date, rate, curve_day_count = "actual_365_fixed", index = "tonar",
                       tenors = c("1M", "3M", "6M", "9M", paste0(1:40, "Y"))) {
  fitted_curve(date, rep(rate, length(tenors)), tenors, curve_day_count, index)
}
