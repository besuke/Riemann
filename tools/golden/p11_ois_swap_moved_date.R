# A swap of LagrangeFinance II-3.3.2 after the evaluation date moves, from the QuantLib R package Version 1.42.
# Usage (from the Riemann root): Rscript tools/golden/p11_ois_swap_moved_date.R
# Writes tests/golden/ois_swap_moved_date.csv.
#
# The curve has a fixed reference date and date-relative helpers. When the evaluation date moves, the helpers'
# dates move and the curve solves again, but it keeps its node dates, because IterativeBootstrap::calculate only
# initializes again for a moving curve (ql/termstructures/iterativebootstrap.hpp, v1.42). This writes those nodes,
# so that a test can give Riemann the same curve and compare the swap alone. The quotes are the legacy text's.

suppressMessages(library(QuantLib))
fmt <- function(x) sprintf("%.17g", x)
at <- function(x) QuantLib::Date(as.integer(as.Date(x)) + 25569L)
number_of <- function(d) as.integer(QuantLib::Date_serialNumber(d))

chapter <- readLines("../LagrangeFinance/part2_quantive_riemann/ch03_ois_swaps_multicurve.qmd", encoding = "UTF-8")
start <- which(startsWith(chapter, "tona_quotes <- tibble::tribble("))
end <- start + which(chapter[(start + 1):length(chapter)] == ")")[1]
trade_date_tona <- "2022-08-19"
quotes <- eval(parse(text = chapter[start:end]))

QuantLib::Settings_instance()$setEvaluationDate(at("2022-08-19"))
day_count <- QuantLib::Actual365Fixed()
index <- QuantLib::OvernightIndex("TONA", 0L, QuantLib::JPYCurrency(), QuantLib::Japan(), day_count)
period_of <- function(t) QuantLib::Period(as.integer(sub("[A-Z]$", "", t)), c(D = "Days", W = "Weeks", M = "Months", Y = "Years")[[sub("^[0-9]+", "", t)]])
handle_of <- function(r) QuantLib::QuoteHandle(QuantLib::SimpleQuote(r))
helpers <- QuantLib::RateHelperVector()
for (i in seq_len(nrow(quotes))) {
  helper <- if (quotes$kind[i] == "overnight_deposit") {
    QuantLib::DepositRateHelper(handle_of(quotes$rate[i]), index)
  } else {
    QuantLib::OISRateHelper(2L, period_of(quotes$tenor[i]), handle_of(quotes$rate[i]), index)
  }
  QuantLib::RateHelperVector_append(helpers, helper)
}
curve <- QuantLib::PiecewiseLogLinearDiscount(at("2022-08-19"), helpers, day_count)
QuantLib::TermStructure_enableExtrapolation(curve)
invisible(curve$discount(at("2023-08-19")))

# In the order of the legacy text: the swap on the first date, then the fixings, then the new date.
curve_index <- QuantLib::OvernightIndex("TONA", 0L, QuantLib::JPYCurrency(), QuantLib::Japan(), day_count, QuantLib::YieldTermStructureHandle(curve))
builder <- QuantLib::MakeOIS(swapTenor = QuantLib::Period(7L, "Days"), overnightIndex = curve_index, fixedRate = -0.00015)
builder <- QuantLib::MakeOIS_withEffectiveDate(builder, at("2022-08-23"))
builder <- QuantLib::MakeOIS_withTerminationDate(builder, at("2022-08-30"))
builder <- QuantLib::MakeOIS_withNominal(builder, 1e7)
builder <- QuantLib::MakeOIS_withDiscountingTermStructure(builder, QuantLib::YieldTermStructureHandle(curve))
swap <- QuantLib::MakeOIS_makeOIS(builder)
invisible(swap$NPV())
curve_index$addFixing(at("2022-08-23"), -0.000090)
curve_index$addFixing(at("2022-08-24"), -0.000085)
QuantLib::Settings_instance()$setEvaluationDate(at("2022-08-25"))
npv <- swap$NPV()
dates <- curve$dates()
serials <- vapply(seq_len(dates$size()) - 1L, function(i) number_of(QuantLib::DateVector___getitem__(dates, i)), 0L)
discounts <- as.numeric(curve$data())

path <- file.path("tests", "golden", "ois_swap_moved_date.csv")
writeLines(c(
  paste0("# QuantLib R package ", packageVersion("QuantLib"), "; quotes of LagrangeFinance II-3.2.1; tools/golden/p11_ois_swap_moved_date.R"),
  "nodes,npv,fixed_npv,overnight_npv,fair_rate",
  paste(c(paste(paste0(serials, ":", fmt(discounts)), collapse = " "),
          fmt(c(npv, QuantLib::FixedVsFloatingSwap_fixedLegNPV(swap), QuantLib::OvernightIndexedSwap_overnightLegNPV(swap), QuantLib::FixedVsFloatingSwap_fairRate(swap)))), collapse = ",")
), path)
cat("NPV", fmt(npv), " legacy text: 6.6505\n")
