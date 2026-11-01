# PRDC: Riemann's closed form and Monte Carlo, and GaussQuant's Monte Carlo on the same curves, built
# by each from the same synthetic quotes. Coupons follow the LagrangeFinance III-7 note.

verify_prdc <- function() {
  evaluation <- "2026-11-02"
  tenors <- c("1Y", "2Y", "3Y", "5Y", "7Y")
  jpy_q <- c(0.01, 0.012, 0.014, 0.016, 0.018)
  usd_q <- c(0.04, 0.038, 0.036, 0.034, 0.032)
  jpy <- RiemannFinMath::build_ois_curve(evaluation, evaluation, "tonar", "actual_365_fixed", RiemannFinMath::ois_instruments(tenors, jpy_q, 2L, 1L, 0L))
  usd <- RiemannFinMath::build_ois_curve(evaluation, evaluation, "sofr", "actual_365_fixed", RiemannFinMath::ois_instruments(tenors, usd_q, 2L, 1L, 0L))
  gq <- function(currency, q) {
    GaussQuant::ir_build_ois_curve_GQH(curve_data = tibble(kind = "ois", tenor = tenors, rate = q), trade_date = evaluation, currency = currency, verbose = FALSE,
                                       curve_type = "log_cubic_discount", reference = "evaluation", payment_frequency = "Annual", payment_lag = 0L,
                                       payment_convention = "ModifiedFollowing", max_quote_error_bp = 1e-4)
  }
  jpy_g <- gq("JPY", jpy_q)
  usd_g <- gq("USD", usd_q)
  GaussQuant::set_eval_date_GQL(evaluation)
  today <- GaussQuant::date_GQL(evaluation)
  schedule <- QuantLib::Schedule(today, GaussQuant::date_GQL("2031-11-02"), QuantLib::Period("Annual"), QuantLib::NullCalendar(), "Unadjusted", "Unadjusted", "Forward", FALSE)
  days <- as.numeric(as.Date(c("2027-11-02", "2028-11-02", "2029-11-02", "2030-11-02", "2031-11-02")) - as.Date(evaluation))
  times <- days / 365
  accruals <- diff(c(0, days)) / 365
  volatility <- QuantLib::BlackVolTermStructureHandle(QuantLib::BlackConstantVol(today, QuantLib::NullCalendar(), 0.12, QuantLib::Actual365Fixed()))
  process <- GaussQuant::fx_process_GQL(150, usd_g$curve_handle, jpy_g$curve_handle, volatility)
  paths <- 100000L
  gaussquant <- GaussQuant::prdc_npv_GQL(evaluation, schedule, process, jpy_g$curve, 1e8, 150, 0.8, 0.78, floor_rate = 0, cap_rate = 0.08,
                                         day_counter = QuantLib::Actual365Fixed(), n_paths = paths, seed = 42L)
  terms <- list(150, 0.12, jpy, usd, times, accruals, 150, 0.8, 0.78, 0, 0.08, notional = 1e8)
  analytic <- do.call(RiemannFinMath::prdc_value, terms)
  monte_carlo <- do.call(RiemannFinMath::prdc_value, c(terms, list(paths = 400000, seed = 20261101)))
  plain <- do.call(RiemannFinMath::prdc_value, c(terms, list(paths = paths, seed = 20261101, antithetic = FALSE)))
  tolerance <- 5 * plain$summary$standard_error
  list(
    values = tibble(
      method = c("Riemann, closed form", "Riemann, Monte Carlo (400,000 paths, antithetic)", "GaussQuant, Monte Carlo (100,000 paths)"),
      npv = c(analytic$summary$npv, monte_carlo$summary$npv, gaussquant),
      diff_from_closed_form = npv - analytic$summary$npv
    ),
    tolerance = tolerance,
    pass = abs(gaussquant - analytic$summary$npv) < tolerance && abs(monte_carlo$summary$npv - analytic$summary$npv) < 5 * monte_carlo$summary$standard_error,
    standard_error = monte_carlo$summary$standard_error,
    greeks = tibble(
      measure = c("FX delta", "FX vega"),
      closed_form = c(analytic$summary$fx_delta, analytic$summary$fx_vega),
      monte_carlo = c(monte_carlo$summary$fx_delta, monte_carlo$summary$fx_vega)
    ),
    curves = analytic$curve_sensitivity |> mutate(monte_carlo_per_bp = monte_carlo$curve_sensitivity$npv_per_bp)
  )
}
