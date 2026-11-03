# Writes baseline/lagrange_values.csv from the saved results of run_chapter.R.
# Usage, from comparison/: Rscript baseline/extract_values.R <directory of the .rds files>
# Only calculated results are written. Market quotes and the bond prices of the basket are left out.
suppressPackageStartupMessages(library(tidyverse))

results <- commandArgs(TRUE)[1]
values_of <- function(chapter) readRDS(file.path(results, paste0(chapter, ".rds")))$values
row <- function(key, section, value) tibble(key = key, section = section, value = as.numeric(value))
metric <- function(table, name, column = "value") table[[column]][table[[1]] == name]
serial <- function(x) as.numeric(as.Date(x))

ch1 <- values_of("ch01_deterministic_cashflow")
ch2 <- values_of("ch02_interest_rate_curves")
ch3 <- values_of("ch03_ois_swaps_multicurve")
ch4 <- values_of("ch04_black_model")
ch5 <- values_of("ch05_credit_risk_cds")
ch6 <- values_of("ch06_irs_calibration")
ch7 <- values_of("ch07_irs_rate_models")
ch8 <- values_of("ch08_early_redemption_models")
ch9 <- values_of("ch09_exotic_derivatives_prdc")
option_greeks <- values_of("ch05_option_greeks_autodiff")

bond_rows <- bind_rows(
  row("bond.settlement", "II-1.3.1", serial(ch1$settlement_date_chr)),
  row("bond.compound_yield_pct", "II-1.5.2", ch1$yen_compound_yield_pct),
  row("bond.flat_dirty_price", "II-1.5.3", ch1$flat_dirty_price),
  row("bond.market_price", "II-1.8.1", ch1$benchmark_market_price),
  row("bond.yield_pct", "II-1.8.6", ch1$yield_gaussquant_result),
  row(sprintf("bond.cashflow.amount.%03d", seq_len(nrow(ch1$tbl_bond_cashflow))), "II-1.3.2", ch1$tbl_bond_cashflow$amount),
  row(sprintf("bond.cashflow.date.%03d", seq_len(nrow(ch1$tbl_bond_cashflow))), "II-1.3.2", serial(ch1$tbl_bond_cashflow$date)),
  row("bond.z_spread_bp", "II-1.6.2", 1e4 * ch1$z_spread),
  row("bond.z_spread_npv", "II-1.6.2", ch1$zspread_result$npv),
  row("yen.remaining_years", "II-1.4.1", ch1$yen_remaining_years),
  row("yen.simple_price", "II-1.4.1", ch1$yen_simple_price),
  row("yen.simple_rate_pct", "II-1.4.1", ch1$yen_simple_rate_recalculated),
  row("yen.compound_yield_pct", "II-1.5.2", ch1$yen_compound_yield_pct),
  row("yen.compound_price", "II-1.5.2", ch1$yen_compound_price),
  row("yen.bpv_yen", "II-1.7.2", ch1$tbl_yen_risk$bpv_yen),
  row("yen.modified_duration", "II-1.7.2", ch1$tbl_yen_risk$modified_duration),
  row("yen.convexity", "II-1.7.2", ch1$tbl_yen_risk$convexity)
)

curve_rows <- bind_rows(
  row(paste0("tona.discount.", ch3$tona_curve_display_tbl$date), "II-3.2.2", ch3$tona_curve_display_tbl$discount_factor),
  row(paste0("tona.zero_pct.", ch3$tona_curve_display_tbl$date), "II-3.2.2", 100 * ch3$tona_curve_display_tbl$zero_rate),
  row("tona.fair_rate_pct", "II-3.2.4", 100 * ch3$jpy_fair_rate_check_tbl$fair_swap_rate_hand),
  row(paste0("market.quote_error_bp.", ch3$asw4_curve_summary_tbl$currency), "II-3.7.2", ch3$asw4_curve_summary_tbl$max_abs_quote_error_bp)
)

summary_rows <- function(prefix, section, summary) {
  bind_rows(row(paste0(prefix, ".npv"), section, summary$npv), row(paste0(prefix, ".fair_rate_pct"), section, 100 * summary$fair_rate))
}
leg_rows <- function(prefix, section, table) {
  bind_rows(
    row(sprintf("%s.amount.%02d", prefix, seq_len(nrow(table))), section, table$amount),
    row(sprintf("%s.date.%02d", prefix, seq_len(nrow(table))), section, serial(table$date))
  )
}
daily <- ch3$jpy_daily_float_cf_tbl
swap_rows <- bind_rows(
  summary_rows("ois.short", "II-3.2.3", ch3$swap_summaries[["JPY short OIS summary"]]),
  summary_rows("ois.after", "II-3.3.2", ch3$swap_summaries[["TONA short OIS summary after fixings"]]),
  summary_rows("ois.sofr", "II-3.4.2", ch3$swap_summaries[["SOFR OIS summary"]]),
  leg_rows("ois.short.fixed", "II-3.2.3", ch3$jpy_ois_fixed_cf_tbl),
  leg_rows("ois.short.overnight", "II-3.2.3", ch3$jpy_ois_overnight_cf_tbl),
  leg_rows("ois.after.overnight", "II-3.3.2", ch3$jpy_overnight_after_tbl),
  leg_rows("ois.sofr.overnight", "II-3.4.2", ch3$sofr_overnight_cf_tbl),
  row(sprintf("ois.daily.days.%02d", seq_len(nrow(daily))), "II-3.3.1", daily$days),
  row(sprintf("ois.daily.forward_pct.%02d", seq_len(nrow(daily))), "II-3.3.1", 100 * daily$forward_rate_from_df),
  row(sprintf("ois.daily.amount.%02d", seq_len(nrow(daily))), "II-3.3.1", daily$amount),
  row("ois.after.pv", "II-3.3.2", ch3$jpy_fixing_diag_tbl$pv)
)

native <- ch3$asw4_native_asw_tbl |>
  select(currency, accrued = accrued_per_100, fair_spread_bp, spread_annuity, npv, bond_leg_pv, overnight_leg_pv, principal_pv, upfront_pv) |>
  pivot_longer(-currency) |>
  transmute(key = paste("asw", currency, name, sep = "."), section = "II-3.7.3", value)
yen <- ch3$asw4_jpy_asw_tbl |>
  select(currency, native_asw_bp, jpy_asw_bp, simple_sum_bp) |>
  left_join(ch3$asw4_jpy_check_tbl |> select(currency, calibrated_discount_spread_bp), by = "currency") |>
  pivot_longer(-currency) |>
  transmute(key = paste("yen_asw", currency, name, sep = "."), section = "II-3.7.4", value)
asw_rows <- bind_rows(
  native, yen,
  row(paste0("yen_asw.basis.", ch3$asw4_basis_sensitivity_tbl$basis_on_jpy_bp), "II-3.7.4", ch3$asw4_basis_sensitivity_tbl$jpy_asw_bp),
  row(paste0("yen_asw.fx.", ch3$asw4_fx_sensitivity_tbl$fx_spot), "II-3.7.4", ch3$asw4_fx_sensitivity_tbl$jpy_asw_bp)
)

current <- ch2$tbl_jgb_current
stress <- ch2$tbl_jgb_stress_display
futures_rows <- bind_rows(
  row(paste0("future.yield_pct.", current$issue), "II-2.6.2", 100 * current$compound_yield),
  row(paste0("future.duration.", current$issue), "II-2.6.2", current$duration),
  row(paste0("future.accrued_start.", current$issue), "II-2.6.3", current$accrued_start),
  row(paste0("future.accrued_end.", current$issue), "II-2.6.3", current$accrued_end),
  row(paste0("future.coupon_income.", current$issue), "II-2.6.3", current$coupon_income),
  row(paste0("future.gross_basis.", current$issue), "II-2.6.3", current$gross_basis),
  row(paste0("future.carry.", current$issue), "II-2.6.3", current$carry),
  row(paste0("future.net_basis.", current$issue), "II-2.6.3", current$net_basis),
  row(paste0("future.implied_repo_pct.", current$issue), "II-2.6.3", 100 * current$implied_repo),
  row("future.ctd_issue", "II-2.6.3", current$issue[current$ctd]),
  row("stress.futures_price", "II-2.7.3", ch2$stress_futures_price),
  row(paste0("stress.price.", stress$issue), "II-2.7.4", stress$price),
  row(paste0("stress.gross_basis.", stress$issue), "II-2.7.4", stress$gross_basis),
  row(paste0("stress.net_basis.", stress$issue), "II-2.7.4", stress$net_basis),
  row(paste0("stress.implied_repo_pct.", stress$issue), "II-2.7.4", stress$implied_repo_pct),
  row("stress.ctd_issue", "II-2.7.4", stress$issue[stress$ctd])
)

option_rows <- bind_rows(
  row(paste0("black.", ch4$black_calculator_tbl$metric), "II-4.2.1", ch4$black_calculator_tbl$value),
  row("black.implied_volatility_pct", "II-4.2.2", 100 * metric(ch4$analytic_european_tbl, "implied_volatility")),
  row(paste0("tree.european.", ch4$european_comparison_tbl$method), "II-4.3.2", ch4$european_comparison_tbl$npv),
  row(paste0("tree.american.", ch4$american_comparison_tbl$method), "II-4.5", ch4$american_comparison_tbl$npv),
  row("lsm.npv", "II-4.6.2", metric(ch4$lsm_result$summary, "lsm_npv")),
  row("lsm.standard_error", "II-4.6.2", metric(ch4$lsm_result$summary, "standard_error")),
  row(paste0("lsm.exercise_paths.", ch4$exercise_summary_tbl$exercise_step), "II-4.6.2", ch4$exercise_summary_tbl$paths),
  row(paste0("lsm.exercised_in_regression.", ch4$lsm_result$diagnostics$step), "II-4.6.2", ch4$lsm_result$diagnostics$exercised_paths),
  row("sobol.first_paths_sum", "II-4.6.1", sum(ch4$sobol_display_tbl$price)),
  row("mc.npv", "II-4.4.1", ch4$mc_result$summary$npv),
  row("mc.standard_error", "II-4.4.1", ch4$mc_result$summary$standard_error),
  row(paste0("greeks.", option_greeks$tbl_gaussquant_greeks$metric), "III-5.2", option_greeks$tbl_gaussquant_greeks$value)
)

cds_rows <- bind_rows(
  row("cds.flat.npv", "II-5.2.4", metric(ch5$cds_flat_summary_tbl, "npv")),
  row("cds.flat.fair_spread_bp", "II-5.2.4", 1e4 * metric(ch5$cds_flat_summary_tbl, "fair_spread")),
  row("cds.flat.coupon_leg", "II-5.2.4", metric(ch5$cds_flat_summary_tbl, "coupon_leg_npv")),
  row("cds.flat.default_leg", "II-5.2.4", metric(ch5$cds_flat_summary_tbl, "default_leg_npv")),
  row("cds.flat.hazard_pct", "II-5.4.1", 100 * ch5$hazard_rate_calibrated),
  row("cds.isda.maturity", "II-5.5.1", serial(metric(ch5$isda_cds_input_tbl, "maturity_date"))),
  row("cds.isda.hazard", "II-5.5.4", ch5$hazard_rate_isda),
  row("cds.isda.npv", "II-5.6.3", metric(ch5$isda_cds_summary_tbl, "cds_npv")),
  row("cds.isda.coupon_leg", "II-5.6.3", metric(ch5$isda_cds_summary_tbl, "coupon_leg_npv")),
  row("cds.isda.default_leg", "II-5.6.3", metric(ch5$isda_cds_summary_tbl, "default_leg_npv")),
  row("cds.isda.rebate", "II-5.6.3", metric(ch5$isda_cds_summary_tbl, "accrual_rebate_npv")),
  row("cds.isda.fair_spread_bp", "II-5.6.3", 1e4 * metric(ch5$isda_cds_summary_tbl, "fair_spread")),
  row("cds.isda.fair_upfront_pct", "II-5.6.3", 100 * metric(ch5$isda_cds_summary_tbl, "fair_upfront")),
  row("cds.midpoint.npv", "II-5.6.6", metric(ch5$engine_comparison_tbl, "npv", "MidPoint")),
  row("cds.midpoint.coupon_leg", "II-5.6.6", metric(ch5$engine_comparison_tbl, "coupon_leg_npv", "MidPoint")),
  row("cds.midpoint.default_leg", "II-5.6.6", metric(ch5$engine_comparison_tbl, "default_leg_npv", "MidPoint")),
  row("cds.midpoint.fair_spread_bp", "II-5.6.6", 1e4 * metric(ch5$engine_comparison_tbl, "fair_spread", "MidPoint"))
)

model_rows <- bind_rows(
  row("sabr.alpha", "II-6.3.2", ch6$sabr_parameters[["alpha"]]),
  row("sabr.nu", "II-6.3.2", ch6$sabr_parameters[["volvol"]]),
  row("sabr.rho", "II-6.3.2", ch6$sabr_parameters[["rho"]]),
  row(paste0("sabr.volatility_pct.", ch6$tbl_sabr_fit$strike_pct), "II-6.3.2", 100 * ch6$tbl_sabr_fit$fitted_volatility),
  row(sprintf("hw.option_bp.%g.%g", ch6$tbl_bond_option_market$option_maturity, ch6$tbl_bond_option_market$bond_maturity), "II-6.4.3", 1e4 * ch6$tbl_bond_option_market$market_price),
  row(sprintf("hw.calibration.model_bp.%g.%g", ch6$tbl_hw_repricing$option_maturity, ch6$tbl_hw_repricing$bond_maturity), "II-6.4.3", 1e4 * ch6$tbl_hw_repricing$model_price),
  row(sprintf("hw.calibration.error_bp.%g.%g", ch6$tbl_hw_repricing$option_maturity, ch6$tbl_hw_repricing$bond_maturity), "II-6.4.3", 1e4 * ch6$tbl_hw_repricing$price_error),
  row("hw.calibration.a_pct", "II-6.4.3", 100 * ch6$hw_fit$par[["a"]]),
  row("hw.calibration.sigma_pct", "II-6.4.3", 100 * ch6$hw_fit$par[["sigma"]]),
  row("hw.calibration.objective", "II-6.4.3", ch6$hw_fit$value),
  row("callable.bullet", "II-8.4.2", ch8$tbl_price_comparison$bullet_from_model),
  row("callable.callable", "II-8.4.2", ch8$tbl_price_comparison$callable_from_model),
  row("callable.call_value", "II-8.4.2", ch8$tbl_price_comparison$issuer_call_value),
  row(sprintf("callable.volatility.%g", ch8$tbl_volatility_sensitivity$short_rate_volatility), "II-8.5", ch8$tbl_volatility_sensitivity$callable_price),
  row(sprintf("callable.grid.%d", ch8$tbl_grid_convergence$number_of_grid_points), "II-8.6", ch8$tbl_grid_convergence$callable_price),
  row(sprintf("mbs.scenario.wal.%d", 1:3), "II-8.8", ch8$tbl_mbs_scenarios$weighted_average_life),
  row(sprintf("mbs.scenario.price_fixed.%d", 1:3), "II-8.8", ch8$tbl_mbs_scenarios$price_fixed_discount),
  row(sprintf("mbs.scenario.price_path.%d", 1:3), "II-8.8", ch8$tbl_mbs_scenarios$price_path_discount),
  row(sprintf("mbs.scenario.principal.%d", 1:3), "II-8.8", ch8$tbl_mbs_scenarios$total_principal),
  row(sprintf("mbs.path.price.%d", ch8$tbl_mbs_mc$path), "II-8.8", ch8$tbl_mbs_mc$price),
  row(sprintf("mbs.path.wal.%d", ch8$tbl_mbs_mc$path), "II-8.8", ch8$tbl_mbs_mc$weighted_average_life),
  row("mbs.path.mean_price", "II-8.8", mean(ch8$tbl_mbs_mc$price)),
  row(sprintf("mbs.static_oas_bp.%g.%g", ch8$tbl_psa_oas$psa_percent, ch8$tbl_psa_oas$assumed_price), "II-8.8.1", ch8$tbl_psa_oas$oas_bp),
  row("lmm.caplet.black_bp", "II-7.4.2", 1e4 * ch7$tbl_caplet_check$black_price),
  row("lmm.caplet.simulated_bp", "II-7.4.2", 1e4 * ch7$tbl_caplet_check$simulated_price),
  row("lmm.caplet.standard_error_bp", "II-7.4.2", 1e4 * ch7$tbl_caplet_check$mc_standard_error),
  row(sprintf("lmm.correlation.%s.%s", ch7$tbl_shock_correlation$forward_1, ch7$tbl_shock_correlation$forward_2), "II-7.4.3", ch7$tbl_shock_correlation$simulated)
)

prdc_rows <- bind_rows(
  row("prdc.npv", "II-9.3.3", metric(ch9$prdc_valuation_summary_tbl, "total_pv_jpy")),
  row("prdc.standard_error", "II-9.5.3", metric(ch9$monte_carlo_error_tbl, "standard_error")),
  row(paste0("prdc.spot.", ch9$spot_sensitivity_tbl$scenario), "II-9.5.1", ch9$spot_sensitivity_tbl$prdc_npv),
  row(paste0("prdc.volatility.", ch9$volatility_sensitivity_tbl$scenario), "II-9.5.2", ch9$volatility_sensitivity_tbl$prdc_npv)
)

bind_rows(bond_rows, curve_rows, swap_rows, asw_rows, futures_rows, option_rows, cds_rows, model_rows, prdc_rows) |>
  write_csv("baseline/lagrange_values.csv")
