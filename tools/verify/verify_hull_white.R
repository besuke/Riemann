# Hull-White: how the callable bond changes with the quadrature. Comparisons with QuantLib and
# LagrangeFinance are in the Rust tests (crates/riemann/tests/golden_hull_white.rs). Synthetic inputs.

verify_hull_white <- function() {
  quotes <- RiemannFinMath::ois_instruments(c("1Y", "5Y", "10Y", "20Y"), rep(0.02, 4), 2L, 1L, 0L)
  curve <- RiemannFinMath::build_ois_curve("2026-11-02", "2026-11-02", "tonar", "actual_365_fixed", quotes)
  flows <- list(time = (1:10) / 2, coupon = 1.5, principal = c(rep(0, 9), 100), call_price = c(NA, rep(100, 8), NA))
  value <- function(points, nodes, spacing) {
    do.call(RiemannFinMath::hull_white_callable_bond, c(flows, list(0.15, 0.012, curve, points = points, limit = 0.2, nodes = nodes, spacing = spacing)))$callable
  }
  reference <- value(1601L, 24L, 0.01)
  tibble(
    rule = c("Gauss-Hermite 16", "Gauss-Hermite 24", "Gauss-Hermite 48", "Trapezoid 0.05", "Trapezoid 0.02"),
    callable = c(value(801L, 16L, 0), value(801L, 24L, 0), value(801L, 48L, 0), value(801L, 24L, 0.05), value(801L, 24L, 0.02))
  ) |> mutate(diff_from_finest = callable - reference)
}
