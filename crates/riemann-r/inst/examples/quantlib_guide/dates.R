# QuantLib Guide: calendars, schedules and day counts
#
# What it computes: the calendar adjustments of "Calendars and holidays", the schedules of "Schedules" and
# the year fractions of "Dangerous day-count conventions" (https://www.quantlibguide.com/), with
# RiemannFinMath's Rust functions. The table compares them with the Guide's printed values.
#
# Run:   source(system.file("examples", "quantlib_guide", "dates.R", package = "RiemannFinMath"))
# Needs: RiemannFinMath and tibble.
#
# Not reproduced (docs/quantlib_guide_inventory.md): adding holidays to a calendar, joint and bespoke
# calendars, Actual/Actual (ISMA) given a whole schedule instead of a reference period.

library(RiemannFinMath)
rows <- list()
add <- function(page, item, guide, riemann) {
  rows[[length(rows) + 1L]] <<- tibble::tibble(page = page, item = item, guide = as.character(guide), riemann = as.character(riemann),
                                               same = identical(as.character(guide), as.character(riemann)))
}
d1 <- as.Date("2023-05-01"); d2 <- as.Date("2023-06-19")
page <- "Calendars and holidays"
add(page, "TARGET: 1 May 2023 is a holiday", TRUE, !is_business_day("target", d1))
add(page, "US government bond: 1 May 2023 is a holiday", FALSE, !is_business_day("us_government_bond", d1))
add(page, "US government bond: 19 June 2023 is a business day", FALSE, as.logical(is_business_day("us_government_bond", d2)))
add(page, "TARGET, Following: 1 May 2023", "2023-05-02", format(compute_adjusted_date("target", d1, "following")))
add(page, "US government bond, Preceding: 19 June 2023", "2023-06-16", format(compute_adjusted_date("us_government_bond", d2, "preceding")))
add(page, "TARGET: end of month of 1 May 2023", "2023-05-31", format(compute_end_of_month("target", d1)))
add(page, "TARGET: advance 19 June 2023 by 2 months", "2023-08-21", format(compute_advanced_date("target", d2, "2M")))
add(page, "TARGET: advance 19 June 2023 by 7 business days", "2023-06-28", format(compute_advanced_date("target", d2, "7D")))

page <- "Schedules"
dates <- function(s) paste(format(s$date), collapse = " ")
add(page, "Semiannual, TARGET, backward, 11 May 2021 to 11 February 2025",
    "2021-05-11 2021-08-11 2022-02-11 2022-08-11 2023-02-13 2023-08-11 2024-02-12 2024-08-12 2025-02-11",
    dates(generate_schedule("2021-05-11", "2025-02-11", "6M", "target", rule = "backward")))
add(page, "End of month, Modified Following, forward, 28 February 2019 to 2023",
    "2019-02-28 2019-08-30 2020-02-28 2020-08-31 2021-02-26 2021-08-31 2022-02-28 2022-08-31 2023-02-28",
    dates(generate_schedule("2019-02-28", "2023-02-28", "6M", "target", "modified_following", rule = "forward", end_of_month = TRUE)))
cds <- generate_schedule("2020-12-21", compute_cds_maturity("2021-03-11", "5Y", "cds2015"), "3M", "target", rule = "cds2015")
add(page, "CDS2015: maturity of a 5-year trade on 11 March 2021", "2025-12-20", format(compute_cds_maturity("2021-03-11", "5Y", "cds2015")))
add(page, "CDS2015: number of dates, first and last", "21 2020-12-21 2025-12-20", paste(nrow(cds), format(cds$date[1]), format(tail(cds$date, 1))))

page <- "Dangerous day-count conventions"
add(page, "30/360 (USA): 1 to 15 January plus 15 to 31 January 2018", 0.08333333333333334,
    sum(compute_year_fraction("thirty_360_usa", c("2018-01-01", "2018-01-15"), c("2018-01-15", "2018-01-31"))))
add(page, "30/360 (USA): 30 to 31 January 2018", 0, compute_year_fraction("thirty_360_usa", "2018-01-30", "2018-01-31"))
add(page, "Act/Act (ISMA), 1 to 15 January 2018, reference period to 1 July", 0.03867403314917127,
    compute_year_fraction("act_act_isma", "2018-01-01", "2018-01-15", "2018-01-01", "2018-07-01"))
add(page, "Act/Act (ISMA), 1 to 15 January 2018, no reference period", 0.038356164383561646,
    compute_year_fraction("act_act_isma", "2018-01-01", "2018-01-15"))

results <- do.call(rbind, rows)
cat("QuantLib Guide, calendars, schedules and day counts\n\n")
old <- options(width = 250)
print(as.data.frame(results), row.names = FALSE, right = FALSE)
options(old)
invisible(results)
