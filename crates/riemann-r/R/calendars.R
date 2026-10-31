# Calendars, by the names of tests/golden/calendar_markets.csv, for example "japan" or "brazil_exchange".
# Agreement with QuantLib is not a check against actual market holidays; year tables end at
# different years (docs/spec_dates_calendars.md, section 3.5).

#' Whether each date is a business day, as logical. Some calendars fail on some dates, as in
#' QuantLib: that stops, or with `on_error = "na"` gives NA, which means failed and not a holiday.
is_business_day <- function(calendar, date, on_error = "stop") {
  as.logical(finish_values(checked(.Call(wrap__is_business_day, calendar, as_serial(date))), on_error))
}

#' The holidays between two dates, both included.
holidays <- function(calendar, from, to, include_weekends = FALSE) {
  as_date(checked(.Call(wrap__holidays, calendar, as_serial(from), as_serial(to), as_flag(include_weekends)))$value)
}
