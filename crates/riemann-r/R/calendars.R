#' Business days and holidays
#'
#' Every calendar and market of QuantLib C++ v1.42 except the joint and bespoke calendars.
#'
#' **Agreement with QuantLib is not a check against actual market holidays.** Many calendars hold
#' year-specific holidays for a limited period only, and outside it holidays that depend on the
#' year are missing without an error. For example, year tables end in 2014 for `indonesia_bej`,
#' 2025 for `india_nse` and `thailand`, 2026 for `china_sse`, 2029 for `saudi_arabia_tadawul` and
#' 2034 for `turkey`.
#'
#' Some calendars fail on some dates, as in QuantLib: `russia_moex` before 2012, and the `israel_*`
#' calendars near both ends of the valid date range (1901-01-01 to 2199-12-31).
#'
#' Calendar names: `null_calendar`, `weekends_only`, `argentina_merval`, `australia_settlement`,
#' `australia_asx`, `austria_settlement`, `austria_exchange`, `botswana`, `brazil_settlement`,
#' `brazil_exchange`, `canada_settlement`, `canada_tsx`, `chile_sse`, `china_sse`, `china_ib`,
#' `czech_republic_pse`, `denmark`, `finland`, `france_settlement`, `france_exchange`,
#' `germany_settlement`, `germany_frankfurt_stock_exchange`, `germany_xetra`, `germany_eurex`,
#' `germany_euwax`, `hong_kong_hkex`, `hungary`, `iceland_icex`, `india_nse`, `indonesia_bej`,
#' `indonesia_jsx`, `indonesia_idx`, `israel_settlement`, `israel_tase`, `israel_shir`,
#' `italy_settlement`, `italy_exchange`, `japan`, `mexico_bmv`, `new_zealand_wellington`,
#' `new_zealand_auckland`, `norway`, `poland_settlement`, `poland_wse`, `romania_public`,
#' `romania_bvb`, `russia_settlement`, `russia_moex`, `saudi_arabia_tadawul`, `singapore_sgx`,
#' `slovakia_bsse`, `south_africa`, `south_korea_settlement`, `south_korea_krx`, `sweden`,
#' `switzerland`, `taiwan_tsec`, `target`, `thailand`, `turkey`, `ukraine_use`, `uk_settlement`,
#' `uk_exchange`, `uk_metals`, `us_settlement`, `us_nyse`, `us_government_bond`, `us_nerc`,
#' `us_libor_impact`, `us_federal_reserve`, `us_sofr`.
#'
#' @param calendar A calendar name.
#' @param date,from,to Dates: `Date` or ISO strings.
#' @param include_weekends `TRUE` or `FALSE`. `NA` is an error.
#' @param on_error `"stop"` or `"na"`. With `"na"`, `NA` means the calendar failed on that date, not
#'   that it is a holiday.
#' @return `is_business_day()` returns a logical vector. `holidays()` returns a `Date` vector.
#' @examples
#' is_business_day("turkey", c("2026-05-19", "2026-05-20"))
#' holidays("brazil_settlement", "2026-01-01", "2026-12-31")
#' @export
is_business_day <- function(calendar, date, on_error = "stop") {
  as.logical(finish_values(checked(.Call(wrap__is_business_day, calendar, as_serial(date))), on_error))
}

#' @rdname is_business_day
#' @export
holidays <- function(calendar, from, to, include_weekends = FALSE) {
  as_date(checked(.Call(wrap__holidays, calendar, as_serial(from), as_serial(to), as_flag(include_weekends)))$value)
}
