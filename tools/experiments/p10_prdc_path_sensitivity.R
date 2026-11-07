# P10: how the step and the number of paths change the derivatives of PRDC notes with a target redemption or
# a knock-out (docs/spec_prdc.md, chapter 13).
#
# The derivatives are central differences on common random numbers. A path whose ending (date or reason)
# changes between the two sides contributes a jump divided by the step, so the noise grows as the step
# shrinks, and the bias of the difference grows as it widens. Each condition is run on several seeds, so the
# spread of the estimates across seeds can be set against the reported standard error.
#
# Usage, from the Riemann root, with RiemannFinMath installed:
#   Rscript tools/experiments/p10_prdc_path_sensitivity.R
# Writes tools/experiments/output/p10_prdc_path_sensitivity.csv and p10_prdc_path_sensitivity.md.

suppressPackageStartupMessages({
  library(RiemannFinMath)
  library(dplyr)
  library(purrr)
  library(tibble)
  library(tidyr)
})

out_dir <- file.path("tools", "experiments", "output")
dir.create(out_dir, showWarnings = FALSE, recursive = TRUE)

# ---- conditions (synthetic) -------------------------------------------------------------------------------
today <- "2026-11-02"
jpy <- create_flat_curve(today, 0.012, "actual_365_fixed", "continuous")
usd <- create_flat_curve(today, 0.035, "actual_365_fixed", "continuous")
rates <- list(domestic = c(0.03, 0.01), foreign = c(0.05, 0.012))
correlation <- c(0.3, -0.2, 0.4)
fx_ends <- c(1, 3, 5); fx_vols <- c(0.09, 0.12, 0.11)  # a piecewise FX volatility
notes <- list(
  target = list(target = 0.15, knock_out_level = Inf),
  knock_out = list(target = Inf, knock_out_level = 155)
)
bumps <- c(1e-2, 3e-3, 1e-3, 3e-4, 1e-4)
path_counts <- c(10000L, 40000L, 160000L)
seeds <- 1:4
parameters <- c("spot", "fx_volatility", "domestic_volatility", "rho_foreign_fx")

run <- function(note, bump, paths, seed) {
  n <- notes[[note]]
  r <- compute_prdc_three_factor_value(150, fx_vols, jpy, usd, rates$domestic, rates$foreign, correlation, 1:5, rep(1, 5), 150, 0.8, 0.78,
                                       0, 0.08, paths = paths, steps_per_year = 12L, seed = seed, target = n$target,
                                       knock_out_level = n$knock_out_level, bump = bump, fx_volatility_times = fx_ends)
  r$path_sensitivity |>
    filter(parameter %in% parameters) |>
    mutate(note = note, bump = bump, paths = paths, seed = seed, value = r$summary$value)
}

grid <- expand_grid(note = names(notes), bump = bumps, paths = path_counts, seed = seeds)
started <- Sys.time()
results <- pmap(grid, run) |> list_rbind()
elapsed <- as.numeric(difftime(Sys.time(), started, units = "secs"))
write.csv(results, file.path(out_dir, "p10_prdc_path_sensitivity.csv"), row.names = FALSE)

# ---- summary: mean over seeds, the reported standard error and the spread across seeds ---------------------
summary <- results |>
  group_by(note, parameter, bump, paths) |>
  summarise(estimate = mean(d_npv), reported_error = mean(standard_error), seed_spread = sd(d_npv),
            changed_paths = mean(changed_paths), .groups = "drop")
format_table <- function(d) {
  rows <- pmap_chr(d, \(note, parameter, bump, paths, estimate, reported_error, seed_spread, changed_paths) {
    sprintf("| %s | %s | %.0e | %d | %.6g | %.3g | %.3g | %.4f |", note, parameter, bump, paths, estimate, reported_error, seed_spread,
            changed_paths)
  })
  c("| note | parameter | bump | paths | mean of 4 seeds | reported SE | SD across seeds | changed paths |",
    "|---|---|---:|---:|---:|---:|---:|---:|", rows)
}
lines <- c(
  "# P10: derivatives of target and knock-out PRDC notes by bump and paths",
  "",
  sprintf("Run on %s, %d runs in %.0f s. Synthetic inputs: flat curves 1.2%% and 3.5%%, Hull-White (0.03, 1%%) and (0.05, 1.2%%),",
          format(Sys.Date()), nrow(grid), elapsed),
  "correlations (0.3, -0.2, 0.4), FX volatility 9%, 12%, 11% to 1, 3, 5 years, spot 150, five annual coupons",
  "`clamp(0.8 S / 150 - 0.78, 0, 8%)`, notional 1, 12 steps a year. Target 15% (capped) or knock-out at 155 on the first four dates.",
  "The spot's step is relative, the volatilities' absolute and the correlation's 10 times the bump.",
  "",
  format_table(summary))
writeLines(lines, file.path(out_dir, "p10_prdc_path_sensitivity.md"))
cat(lines, sep = "\n")
