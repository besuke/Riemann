#' Credit tranches and CMO waterfalls
#'
#' A tranche with attachment `A` and detachment `D` (fractions of the portfolio) loses
#' `min(max(l - A, 0), D - A)` when the portfolio loses the fraction `l` (LagrangeFinance I-11.9).
#'
#' `compute_tranche_risk()` reads each tranche's expected loss, the probabilities that it is hit (`l > A`) and
#' wiped out (`l >= D`), and its VaR off an exact loss distribution from `compute_credit_loss_distribution()`;
#' `total` turns losses into fractions. `compute_lhp_expected_tranche_loss()` is the expected tranche loss of a
#' large homogeneous pool, whose loss is `LGD p(Z)` with `p` the conditional PD.
#'
#' `compute_tranche_legs()` prices a tranche as in I-11.12 on obligors with constant hazard rates
#' (`PD(t) = 1 - exp(-h t)`): at each payment date the exact distribution gives `E[L_{A,D}(t)]`; the premium leg
#' per unit spread is `sum DF(t) dt E[(D - A) - L_{A,D}(t)]`, the loss leg `sum DF(t) dE[L_{A,D}(t)]`, with
#' `DF(t) = exp(-r t)`, and the fair spread their ratio (per year, as a fraction).
#'
#' `compute_sequential_cmo()` pays the collateral's principal to the tranches in order (I-11.4); with
#' `z_last = TRUE` the last tranche accrues while an earlier one is outstanding: its interest is added to its
#' balance and that cash is paid as principal to the earlier tranches (I-11.6). `compute_pac_schedule()` is the
#' smaller of two principal streams each month (the pool at the band's two prepayment speeds), and
#' `compute_pac_support_cmo()` pays the PAC its schedule plus any shortfall first, then the support, and the PAC
#' again once the support is retired (I-11.5). Coupons are annual and paid monthly on the opening balance; what is
#' left of the collateral's cash is the residual.
#'
#' @param loss_rate Portfolio loss as a fraction.
#' @param attachment,detachment Tranche bounds, fractions of the portfolio (vectors for several tranches).
#' @param distribution From `compute_credit_loss_distribution()`.
#' @param total Portfolio notional.
#' @param confidence_level For the tranche VaR.
#' @param pd,lgd,rho The large pool's obligors.
#' @param panels Panels of the quadrature over the common factor.
#' @param hazard,ead Per obligor (with `lgd`, `rho`), recycled.
#' @param unit The loss lattice (see `compute_credit_loss_distribution()`).
#' @param times Payment times in years; `discount_rate` continuously compounded.
#' @param discount_rate Continuously compounded rate.
#' @param collateral Monthly cash flows of the pool: a tibble from `compute_mbs_cashflows()`, or a list with
#'   `principal` and `interest`.
#' @param balances,coupons Tranches in payment order.
#' @param z_last Whether the last tranche accrues.
#' @param low,high Collateral at the two prepayment speeds of the PAC band.
#' @param schedule The PAC's scheduled principal by month.
#' @return `compute_tranche_loss()`, `compute_lhp_expected_tranche_loss()`, `compute_pac_schedule()` numeric
#'   vectors. `compute_tranche_risk()` a tibble by tranche. `compute_tranche_legs()` a list with `legs` (a tibble
#'   by date) and the totals. The CMO functions a list with `cashflows` (a tibble by month and tranche) and `wal`.
#' @examples
#' compute_tranche_loss(c(0.01, 0.05, 0.12), 0.03, 0.07)
#' pd <- seq(0.005, 0.05, length.out = 100)
#' d <- compute_credit_loss_distribution(pd, 100, 0.6, 0.2, unit = 60)
#' compute_tranche_risk(d, 10000, c(0, 0.03, 0.07, 0.10), c(0.03, 0.07, 0.10, 1))
#' pool <- compute_mbs_cashflows(create_mbs_pass_through(1000, 0.055, 0.05), 0.055)
#' compute_sequential_cmo(pool, c(400, 300, 200, 100), 0.05)$wal
#' @export
compute_tranche_loss <- function(loss_rate, attachment, detachment) {
  checked(.Call(wrap__compute_tranche_loss, as.numeric(loss_rate), as.numeric(attachment), as.numeric(detachment)))$value
}

#' @rdname compute_tranche_loss
#' @export
compute_tranche_risk <- function(distribution, total, attachment, detachment, confidence_level = 0.99) {
  unit <- attr(distribution, "unit")
  if (is.null(unit)) stop("`distribution` must come from compute_credit_loss_distribution().", call. = FALSE)
  r <- checked(.Call(wrap__compute_tranche_risk, distribution$probability, unit, as.numeric(total), as.numeric(attachment),
                     as.numeric(detachment), as.numeric(confidence_level)))
  tibble::tibble(attachment = as.numeric(attachment), detachment = as.numeric(detachment), expected_tranche_loss = r$expected_tranche_loss,
                 expected_loss_of_tranche = r$expected_tranche_loss / (as.numeric(detachment) - as.numeric(attachment)),
                 hit_probability = r$hit_probability, wipeout_probability = r$wipeout_probability, var = r$var)
}

#' @rdname compute_tranche_loss
#' @export
compute_lhp_expected_tranche_loss <- function(pd, lgd, rho, attachment, detachment, panels = 50L) {
  checked(.Call(wrap__compute_lhp_expected_tranche_loss, as.numeric(pd), as.numeric(lgd), as.numeric(rho), as.numeric(attachment),
                as.numeric(detachment), as.integer(panels)))$value
}

#' @rdname compute_tranche_loss
#' @export
compute_tranche_legs <- function(hazard, ead, lgd, rho, unit, times, discount_rate, attachment, detachment, panels = 200L) {
  n <- max(length(hazard), length(ead), length(lgd), length(rho))
  r <- checked(.Call(wrap__compute_tranche_legs, rep_len(as.numeric(hazard), n), rep_len(as.numeric(ead), n), rep_len(as.numeric(lgd), n),
                     rep_len(as.numeric(rho), n), as.numeric(unit), as.numeric(times), as.numeric(discount_rate), as.numeric(attachment),
                     as.numeric(detachment), as.integer(panels)))
  list(legs = tibble::tibble(time = as.numeric(times), discount = r$discount, expected_tranche_loss = r$expected_tranche_loss),
       premium_per_unit_spread = r$premium_per_unit_spread, loss_leg = r$loss_leg, fair_spread = r$fair_spread)
}

cmo_collateral <- function(collateral) {
  if (all(c("scheduled_principal", "prepaid_principal", "interest") %in% names(collateral))) {
    return(list(principal = collateral$scheduled_principal + collateral$prepaid_principal, interest = collateral$interest))
  }
  if (all(c("principal", "interest") %in% names(collateral))) return(list(principal = as.numeric(collateral$principal), interest = as.numeric(collateral$interest)))
  stop("`collateral` must come from compute_mbs_cashflows(), or have `principal` and `interest`.", call. = FALSE)
}

cmo_result <- function(r, n, months) {
  by_tranche <- function(x) as.numeric(x)
  list(
    cashflows = tibble::tibble(month = rep(seq_len(months), n), tranche = rep(seq_len(n), each = months), interest = by_tranche(r$interest),
                               principal = by_tranche(r$principal), accrued = by_tranche(r$accrued), balance = by_tranche(r$balance)),
    residual = r$residual,
    wal = r$weighted_average_life
  )
}

#' @rdname compute_tranche_loss
#' @export
compute_sequential_cmo <- function(collateral, balances, coupons, z_last = TRUE) {
  c <- cmo_collateral(collateral)
  coupons <- rep_len(as.numeric(coupons), length(balances))
  r <- checked(.Call(wrap__compute_sequential_cmo, c$principal, c$interest, as.numeric(balances), coupons, as_flag(z_last)))
  cmo_result(r, length(balances), length(c$principal))
}

#' @rdname compute_tranche_loss
#' @export
compute_pac_schedule <- function(low, high) {
  pmin(cmo_collateral(low)$principal, cmo_collateral(high)$principal)
}

#' @rdname compute_tranche_loss
#' @export
compute_pac_support_cmo <- function(collateral, schedule, balances, coupons) {
  c <- cmo_collateral(collateral)
  r <- checked(.Call(wrap__compute_pac_support_cmo, c$principal, c$interest, as.numeric(schedule), as.numeric(balances), rep_len(as.numeric(coupons), 2L)))
  cmo_result(r, 2L, length(c$principal))
}
