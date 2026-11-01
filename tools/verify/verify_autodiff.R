# Automatic differentiation against analytic formulas and numeric differences, through the R package.
# All inputs are synthetic.

ad_bond <- function(coupon, years) {
  RiemannFinMath::fixed_rate_bond(
    settlement_days = 1L, face_amount = 100,
    effective_date = "2025-03-20", termination_date = sprintf("%d-03-20", 2025L + years),
    tenor = "6M", calendar = "japan", convention = "following", termination_convention = "following",
    rule = "backward", end_of_month = FALSE, coupon_rate = coupon, day_count = "actual_365_fixed",
    payment_convention = "following", redemption = 100
  )
}

verify_yield_sensitivity <- function() {
  grid <- tidyr::expand_grid(coupon = c(0, 0.015, 0.03, 0.08), years = c(5L, 10L, 20L),
                             rate = c(0.001, 0.02, 0.07),
                             convention = list(c("compounded", "2"), c("compounded", "1"), c("continuous", "1")))
  rows <- purrr::pmap(grid, function(coupon, years, rate, convention) {
    bond <- ad_bond(coupon, years)
    compounding <- convention[1]
    frequency <- as.integer(convention[2])
    s <- RiemannFinMath::yield_sensitivity(bond, rate, "actual_365_fixed", compounding, frequency, "2026-11-04")
    v <- RiemannFinMath::bond_values(bond, rate, "actual_365_fixed", compounding, frequency, "2026-11-04")
    tibble(price_diff = abs(s$dirty_price - v$dirty_price),
           first_diff = abs(s$d_price_d_yield + v$duration_modified * v$dirty_price) / abs(v$duration_modified * v$dirty_price),
           second_diff = abs(s$d2_price_d_yield2 - v$convexity * v$dirty_price) / abs(v$convexity * v$dirty_price))
  })
  all_rows <- bind_rows(rows)
  tibble(cases = nrow(all_rows), max_price_diff = max(all_rows$price_diff),
         max_first_diff = max(all_rows$first_diff), max_second_diff = max(all_rows$second_diff),
         pass = max_price_diff == 0 && max_first_diff < 1e-11 && max_second_diff < 1e-11)
}

verify_quote_sensitivity <- function() {
  tenors <- c("ON", "1Y", "2Y", "5Y", "10Y", "20Y")
  rates <- c(0.002, 0.004, 0.006, 0.011, 0.016, 0.02)
  grid <- tidyr::expand_grid(index = c("tonar", "estr", "sofr", "sonia"),
                             interpolation = c("log_linear", "monotonic_log_cubic"))
  bond <- ad_bond(0.02, 8L)
  evaluation <- "2026-11-02"
  curve_of <- function(index, interpolation, q) {
    RiemannFinMath::build_ois_curve(evaluation, evaluation, index, "actual_365_fixed",
                                    RiemannFinMath::ois_instruments(tenors, q, 2L, 1L, 0L), interpolation = interpolation)
  }
  h <- 1e-6
  purrr::pmap(grid, function(index, interpolation) {
    ad <- RiemannFinMath::quote_sensitivity(bond, curve_of(index, interpolation, rates), evaluation)
    numeric <- vapply(seq_along(rates), function(i) {
      up <- rates; up[i] <- up[i] + h
      down <- rates; down[i] <- down[i] - h
      (RiemannFinMath::value_on_curve(bond, curve_of(index, interpolation, up), evaluation)$npv -
         RiemannFinMath::value_on_curve(bond, curve_of(index, interpolation, down), evaluation)$npv) / (2 * h) * 1e-4
    }, numeric(1))
    tibble(index = index, interpolation = interpolation, quotes = length(rates),
           max_abs_diff = max(abs(ad$npv_per_bp - numeric)), pass = max_abs_diff < 1e-9)
  }) |> bind_rows()
}

verify_autodiff <- function() {
  list(yield = verify_yield_sensitivity(), quotes = verify_quote_sensitivity())
}
