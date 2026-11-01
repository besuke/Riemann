# Asset swaps: end to end against GaussQuant, each with its own bootstrap from the same synthetic
# quotes, and against the golden values with GaussQuant's curve nodes (in the Rust tests).

verify_asset_swaps <- function() {
  suppressMessages(requireNamespace("GaussQuant"))
  evaluation <- "2026-10-06"
  tenors <- c("1Y", "2Y", "3Y", "5Y", "7Y", "10Y", "15Y")
  set.seed(20261103)
  markets <- tibble::tribble(
    ~currency, ~index, ~level, ~calendar_riemann, ~settlement_days,
    "JPY", "tonar", 0.012, "japan", 2L,
    "USD", "sofr", 0.038, "us_sofr", 2L,
    "EUR", "estr", 0.024, "target", 2L,
    "GBP", "sonia", 0.040, "uk_exchange", 0L
  )
  quantlib_calendar <- list(JPY = QuantLib::Japan(), USD = QuantLib::UnitedStates("SOFR"), EUR = QuantLib::TARGET(), GBP = QuantLib::UnitedKingdom("Exchange"))
  rows <- purrr::pmap(markets, function(currency, index, level, calendar_riemann, settlement_days) {
    q <- level + cumsum(rnorm(length(tenors), 0.0005, 0.001))
    gq_curve <- GaussQuant::ir_build_ois_curve_GQH(
      curve_data = tibble(kind = "ois", tenor = tenors, rate = q), trade_date = evaluation, currency = currency, verbose = FALSE,
      curve_type = "log_cubic_discount", reference = "evaluation", payment_frequency = "Annual", payment_lag = 0L,
      payment_convention = "ModifiedFollowing", max_quote_error_bp = 1e-4
    )
    r_curve <- RiemannFinMath::build_ois_curve(evaluation, evaluation, index, "actual_365_fixed", RiemannFinMath::ois_instruments(tenors, q, settlement_days, 1L, 0L))
    purrr::map_dfr(1:10, function(i) {
      coupon <- round(runif(1, 0.005, 0.06), 4)
      years <- sample(c(3L, 5L, 7L, 10L), 1)
      clean <- runif(1, 85, 115)
      GaussQuant::set_eval_date_GQL(evaluation)
      gq_bond <- GaussQuant::fixed_rate_bond_GQL(
        issue_date = "2026-09-30", maturity_date = sprintf("%d-09-30", 2026L + years), coupon_rate = coupon, face_amount = 100, redemption = 100, settlement_days = 2L,
        frequency = QuantLib::Period("Semiannual"), calendar = quantlib_calendar[[currency]], accrual_day_counter = QuantLib::Actual365Fixed(),
        payment_convention = "Following", schedule_convention = "Unadjusted", maturity_convention = "Unadjusted"
      )
      gq <- GaussQuant::ois_asset_swap_GQL(gq_bond, clean, gq_curve, payment_frequency = "Semiannual", payment_lag = 0L)$valuation
      r_bond <- RiemannFinMath::fixed_rate_bond(
        settlement_days = 2L, face_amount = 100, effective_date = "2026-09-30", termination_date = sprintf("%d-09-30", 2026L + years), tenor = "6M",
        calendar = calendar_riemann, convention = "unadjusted", termination_convention = "unadjusted", rule = "backward", end_of_month = FALSE,
        coupon_rate = coupon, day_count = "actual_365_fixed", payment_convention = "following", redemption = 100
      )
      r <- RiemannFinMath::ois_asset_swap(r_bond, clean, r_curve, evaluation)
      tibble(currency = currency, diff_bp = abs(r$fair_spread_bp - gq$fair_spread_bp), diff_annuity = abs(r$spread_annuity - gq$spread_annuity))
    })
  })
  bind_rows(rows) |>
    group_by(currency) |>
    summarise(cases = n(), max_diff_bp = max(diff_bp), max_diff_annuity = max(diff_annuity), .groups = "drop") |>
    mutate(pass = max_diff_bp < 1e-6 & max_diff_annuity < 1e-8)
}
