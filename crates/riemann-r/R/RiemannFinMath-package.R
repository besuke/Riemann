#' RiemannFinMath: bond and OIS curve calculations from the Riemann Rust library
#'
#' The calculations are in Rust. The R functions convert dates and return tibbles. There is no
#' global state such as an evaluation date: dates, calendars and conventions are arguments.
#'
#' Results are compared with QuantLib, not with market data. Agreement with QuantLib is not a
#' check against actual market holidays or market conventions, and QuantLib's defaults are not
#' market conventions. Conventions therefore have no defaults here.
#'
#' Names of choices are snake_case strings:
#' \describe{
#'   \item{Day counts}{`actual_360`, `actual_365_fixed`, `act_act_isda`, `act_act_isma`,
#'     `thirty_360_bond_basis` and others; `business_252:<calendar>` for Business/252.}
#'   \item{Compounding}{`simple`, `compounded`, `continuous`.}
#'   \item{Business day conventions}{`following`, `modified_following`, `preceding`,
#'     `modified_preceding`, `unadjusted`, `half_month_modified_following`, `nearest`.}
#'   \item{Date generation rules}{`backward`, `forward`, `zero`, `third_wednesday`,
#'     `third_wednesday_inclusive`, `twentieth`, `twentieth_imm`, `old_cds`, `cds`.}
#'   \item{Calendars}{See [is_business_day()].}
#'   \item{Overnight indices}{`tonar`, `estr`, `sofr`, `sonia`.}
#' }
#'
#' Dates are `Date` values or ISO strings (`YYYY-MM-DD`). Functions with `on_error` stop at the
#' first row that fails with `"stop"`; with `"na"` they keep `NA` for that row.
#'
#' A defect in the Rust code panics: its file and line are printed on stderr, and R receives an
#' error and keeps running.
#'
#' @keywords internal
#' @useDynLib RiemannFinMath, .registration = TRUE
#' @importFrom tibble as_tibble tibble
"_PACKAGE"
