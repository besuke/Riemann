# P11-4: how the function that stands for the continuation value changes the exercise decision and the
# value of a Bermudan put (docs/spec_longstaff_schwartz.md, chapter 7).
#
# The explanatory variable is the spot S. At each exercise date the continuation value is estimated by
#   C_t(S) = a_t,  a_t + b_t S,  a_t + b_t S + c_t S^2, ...  (powers of x = S / K)
# and a path exercises when the immediate value K - S exceeds it. Changing the function changes the
# decision, and so the value. Everything that the results depend on is fixed below: inputs, seeds, path
# counts and the settings of the binomial tree that gives the reference value.
#
# Usage, from the Riemann root, with RiemannFinMath installed:
#   Rscript tools/experiments/p11_ls_function_shape.R
# Writes tools/experiments/output/p11_ls_*.csv, p11_ls_tables.md and p11_ls_continuation.png.

suppressPackageStartupMessages({
  library(RiemannFinMath)
  library(dplyr)
  library(purrr)
  library(tibble)
  library(ggplot2)
})

out_dir <- file.path("tools", "experiments", "output")
dir.create(out_dir, showWarnings = FALSE, recursive = TRUE)

# ---- conditions: LagrangeFinance II-4.6.2 ----------------------------------------------------------------
spot <- 100; strike <- 100; rate <- 0.05; dividend <- 0; volatility <- 0.2
time <- 367 / 365  # the 367 days of the legacy text, Actual/365 (Fixed)
steps <- 12L       # exercise at t_i = i * time / 12, i = 1..12, not at 0
seeds <- 1:8       # pseudo-random seeds for A and C
many_seeds <- 1:64 # for B, where the bias of sharing is small against the noise
evaluation_paths <- 2^16

# ---- reference: the same Bermudan put on a CRR tree with the exercise dates on its nodes -------------------
tree_steps_per_date <- c(1000L, 1001L) # CRR oscillates; the value is the mean of neighbouring sizes
bermudan_tree <- function(per_date, boundary_dates = integer()) {
  n <- steps * per_date
  dt <- time / n
  u <- exp(volatility * sqrt(dt)); d <- 1 / u
  p <- (exp((rate - dividend) * dt) - d) / (u - d)
  disc <- exp(-rate * dt)
  value <- pmax(strike - spot * u^(0:n) * d^(n - 0:n), 0)
  boundary <- c()
  for (i in (n - 1):0) {
    value <- disc * (p * value[2:(i + 2)] + (1 - p) * value[1:(i + 1)])
    if (i > 0 && i %% per_date == 0) {
      s <- spot * u^(0:i) * d^(i - 0:i)
      date <- i %/% per_date
      if (date %in% boundary_dates) boundary[as.character(date)] <- max(s[strike - s >= value & s < strike])
      value <- pmax(value, strike - s)
    }
  }
  list(value = value[1], boundary = boundary)
}
trees <- map(tree_steps_per_date, \(m) bermudan_tree(m, c(3L, 6L, 9L, 11L)))
reference <- mean(map_dbl(trees, "value"))
tree_boundary <- tibble(step = c(3L, 6L, 9L, 11L), low = pmin(trees[[1]]$boundary, trees[[2]]$boundary)[c("3", "6", "9", "11")],
                        high = pmax(trees[[1]]$boundary, trees[[2]]$boundary)[c("3", "6", "9", "11")])

# ---- one valuation --------------------------------------------------------------------------------------------
value_put <- function(degree, paths, sequence = "pseudo", seed = 1, calibration = "in_sample", calibration_paths = paths, payoff = FALSE) {
  compute_longstaff_schwartz_value(
    "put", spot, strike, rate, dividend, volatility, time, steps = steps, paths = paths, sequence = sequence, seed = seed,
    basis_degree = degree, include_payoff = payoff, calibration = calibration, calibration_paths = calibration_paths,
    few_in_the_money = "hold"
  )
}
summarise_run <- function(r, paths) {
  ex <- r$exercise[[1]]
  boundary <- r$regressions[[1]]$exercise_boundary[r$regressions[[1]]$step == 6L]
  tibble(value = r$value, early_exercise = sum(ex$paths[ex$step < steps]) / paths, boundary_6 = boundary, too_few_dates = r$dates_with_too_few_paths)
}
mean_sd <- function(x) c(mean = mean(x), sd = sd(x))

# ---- LagrangeFinance's own result, as the starting point ----------------------------------------------------------
lf <- value_put(2L, 2048, sequence = "sobol")

# ---- A. the basis, with the same paths and the same rule -------------------------------------------------------------
bases <- tribble(~basis, ~degree, ~payoff,
  "constant", 0L, FALSE, "linear", 1L, FALSE, "quadratic", 2L, FALSE, "cubic", 3L, FALSE, "quartic", 4L, FALSE, "constant + payoff", 0L, TRUE)
table_a <- pmap_dfr(bases, \(basis, degree, payoff) {
  runs <- map_dfr(seeds, \(s) summarise_run(value_put(degree, evaluation_paths, seed = s, calibration = "separate", calibration_paths = 2^14, payoff = payoff), evaluation_paths))
  in_sample <- map_dbl(seeds, \(s) value_put(degree, 2^14, seed = s, payoff = payoff)$value)
  sobol <- value_put(degree, 2048, sequence = "sobol", payoff = payoff)
  finite <- runs$boundary_6[is.finite(runs$boundary_6)]
  tibble(basis = basis, value = mean(runs$value), difference = mean(runs$value) - reference, sd = sd(runs$value),
         in_sample_value = mean(in_sample), early_exercise = mean(runs$early_exercise),
         boundary_6 = mean(finite), boundary_6_sd = sd(finite), boundary_seeds = length(finite), seeds = length(seeds),
         too_few_dates = sum(runs$too_few_dates), sobol_in_sample_2048 = sobol$value)
})

# ---- B. sharing the paths of the regression and of the valuation, quadratic basis --------------------------
table_b <- map_dfr(c(8L, 10L, 12L, 14L), \(k) {
  n <- 2^k
  shared <- map_dbl(many_seeds, \(s) value_put(2L, n, seed = s)$value - reference)
  separate <- map_dbl(many_seeds, \(s) value_put(2L, evaluation_paths, seed = s, calibration = "separate", calibration_paths = n)$value - reference)
  sobol <- \(calibration, paths) value_put(2L, paths, sequence = "sobol", calibration = calibration, calibration_paths = n)$value - reference
  error_of_mean <- \(x) sd(x) / sqrt(length(x))
  shared_error <- error_of_mean(shared)
  separate_error <- error_of_mean(separate)
  tibble(regression_paths = sprintf("2^%d", k), shared = mean(shared), shared_error_of_mean = shared_error,
         separate = mean(separate), separate_error_of_mean = separate_error,
         sobol_shared = sobol("in_sample", n), sobol_overlapping = sobol("overlapping", evaluation_paths),
         sobol_separate = sobol("separate", evaluation_paths))
})

# ---- C. the number of regression paths -----------------------------------------------------------------------------
table_c <- tidyr::expand_grid(degree = c(0L, 2L, 4L), k = c(6L, 8L, 10L, 12L, 14L, 16L)) |>
  pmap_dfr(\(degree, k) {
    runs <- map_dfr(seeds, \(s) summarise_run(value_put(degree, evaluation_paths, seed = s, calibration = "separate", calibration_paths = 2^k), evaluation_paths))
    finite <- runs$boundary_6[is.finite(runs$boundary_6)]
    tibble(degree = degree, regression_paths = sprintf("2^%d", k), difference = mean(runs$value) - reference, sd = sd(runs$value),
           early_exercise = mean(runs$early_exercise), boundary_6 = if (length(finite)) mean(finite) else NA_real_,
           boundary_6_sd = if (length(finite) > 1) sd(finite) else NA_real_, boundary_seeds = length(finite), seeds = length(seeds),
           too_few_dates = sum(runs$too_few_dates))
  })

# ---- the functions themselves: (K - S) - C_t(S) on a grid of spots in the money (S < K) ---------------------------
grid_spots <- seq(60, strike - 0.01, by = 0.01)
curves <- pmap_dfr(filter(bases, !payoff), \(basis, degree, payoff) {
  r <- value_put(degree, evaluation_paths, seed = 1, calibration = "separate", calibration_paths = 2^14)
  reg <- r$regressions[[1]]
  map_dfr(c(6L, 11L), \(step) {
    beta <- reg$coefficients[[which(reg$step == step)]]
    x <- grid_spots / strike
    continuation <- vapply(x, \(v) sum(beta * v^(seq_along(beta) - 1L)), 1)
    tibble(basis = basis, step = step, spot = grid_spots, difference = (strike - grid_spots) - continuation)
  })
})
crossings <- curves |>
  group_by(basis, step) |>
  summarise(
    crossings = {
      s <- sign(difference); i <- which(s[-1] != s[-length(s)])
      if (length(i)) paste(sprintf("%.2f", spot[i] + 0.005), collapse = "; ") else "none"
    },
    exercise_region = {
      ex <- spot[difference > 0]
      if (length(ex)) sprintf("%.2f to %.2f", min(ex), max(ex)) else "none"
    },
    .groups = "drop"
  ) |>
  left_join(transmute(tree_boundary, step, tree_boundary = sprintf("%.2f to %.2f", low, high)), by = "step")

figure <- ggplot(curves, aes(spot, difference, colour = factor(basis, levels = bases$basis))) +
  geom_hline(yintercept = 0, colour = "grey40") +
  geom_vline(data = tree_boundary |> filter(step %in% c(6L, 11L)) |> mutate(step = step),
             aes(xintercept = (low + high) / 2), linetype = "dashed", colour = "grey30") +
  geom_line(linewidth = 0.6) +
  facet_wrap(~ factor(paste("step", step), levels = c("step 6", "step 11"))) +
  coord_cartesian(xlim = c(60, strike), ylim = c(-3, 3)) +
  labs(x = "Spot S (in the money: S < K = 100)", y = "(K - S) - C_t(S)", colour = "Basis",
       title = "Exercise where the curve is above zero",
       subtitle = "Regression on 2^14 pseudo-random paths, seed 1. Dashed line: the boundary of the binomial tree") +
  theme_minimal()
ggsave(file.path(out_dir, "p11_ls_continuation.png"), figure, width = 9, height = 4.5, dpi = 120)

# ---- outputs ---------------------------------------------------------------------------------------------------------
write.csv(table_a, file.path(out_dir, "p11_ls_basis.csv"), row.names = FALSE)
write.csv(table_b, file.path(out_dir, "p11_ls_sharing.csv"), row.names = FALSE)
write.csv(table_c, file.path(out_dir, "p11_ls_paths.csv"), row.names = FALSE)
write.csv(crossings, file.path(out_dir, "p11_ls_crossings.csv"), row.names = FALSE)
kable <- function(x, digits) paste(knitr::kable(x, format = "pipe", digits = digits), collapse = "\n")
writeLines(c(
  "# P11-4 experiments (generated by tools/experiments/p11_ls_function_shape.R; do not edit)",
  "",
  sprintf("Reference (CRR, %s steps per date, mean): %.6f. Tree values: %s.", paste(tree_steps_per_date, collapse = " and "),
          reference, paste(sprintf("%.6f", map_dbl(trees, "value")), collapse = ", ")),
  sprintf("LagrangeFinance's conditions (Sobol 2048, quadratic, shared): %.10f, %+.4f from the reference.", lf$value, lf$value - reference),
  "", "## Tree boundary", "", kable(tree_boundary, 2),
  "", "## A. Basis", "", kable(table_a, 4),
  "", "## B. Sharing (differences from the reference)", "", kable(table_b, 4),
  "", "## C. Regression paths", "", kable(table_c, 4),
  "", "## Zero crossings of (K - S) - C_t(S), S < K", "", kable(crossings, 2)
), file.path(out_dir, "p11_ls_tables.md"))
cat(readLines(file.path(out_dir, "p11_ls_tables.md")), sep = "\n")
