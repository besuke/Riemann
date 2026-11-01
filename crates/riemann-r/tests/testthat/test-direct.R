# The R entry point against QuantLib values and the behaviour of its border.

example_bond <- function() {
  fixed_rate_bond(
    settlement_days = 1L, face_amount = 100, effective_date = "2020-03-20", termination_date = as.Date("2030-03-20"),
    tenor = "6M", calendar = "japan", convention = "following", termination_convention = "following", rule = "backward",
    end_of_month = FALSE, coupon_rate = 0.015, day_count = "act_act_isma", payment_convention = "following",
    redemption = 100, issue_date = "2020-03-20"
  )
}

example_instruments <- function() {
  ois_instruments(c("ON", "1Y", "2Y", "5Y", "10Y"), c(0.002, 0.004, 0.005, 0.009, 0.014), 2L, 1L, 0L)
}

test_that("a clean price equals the QuantLib R package on the same bond", {
  # QuantLib R package Version 1.42: BondFunctions_cleanPrice on the same inputs
  settlement <- settlement_date(example_bond(), "2026-11-02")
  expect_equal(settlement, as.Date("2026-11-04"))
  expect_identical(
    clean_price(example_bond(), 0.02, "act_act_isma", "compounded", 2L, settlement),
    98.370204869913721
  )
})

test_that("dates can be Date values or ISO strings", {
  bond <- example_bond()
  from_string <- bond_values(bond, 0.02, "act_act_isma", "compounded", 2L, "2026-11-04")
  from_date <- bond_values(bond, 0.02, "act_act_isma", "compounded", 2L, as.Date("2026-11-04"))
  expect_identical(from_string, from_date)
  expect_error(bond_values(bond, 0.02, "act_act_isma", "compounded", 2L, "04/11/2026"), "ISO date")
  expect_error(bond_values(bond, 0.02, "act_act_isma", "compounded", 2L, 46330), "Date or an ISO")
})

test_that("a yield and the price computed from it agree", {
  bond <- example_bond()
  y <- yield_from_price(bond, 98.5, "clean", "act_act_isma", "compounded", 2L, "2026-11-04")
  expect_equal(clean_price(bond, y, "act_act_isma", "compounded", 2L, "2026-11-04"), 98.5, tolerance = 1e-8)
})

test_that("bond_values returns a tibble with one row per input", {
  values <- bond_values(example_bond(), c(0.01, 0.02, 0.03), "act_act_isma", "compounded", 2L, "2026-11-04")
  expect_s3_class(values, "tbl_df")
  expect_equal(nrow(values), 3L)
  expect_true(all(diff(values$clean_price) < 0))
  expect_true(all(is.na(values$error)))
})

test_that("names that do not exist are errors", {
  bond <- example_bond()
  expect_error(bond_values(bond, 0.02, "actual_999", "compounded", 2L, "2026-11-04"), "unknown day count")
  expect_error(is_business_day("mars", "2026-01-01"), "unknown calendar")
  expect_error(build_ois_curve("2026-11-02", "2026-11-02", "libor", "actual_365_fixed", example_instruments()), "unknown overnight index")
})

test_that("all four indices build curves that reprice their quotes", {
  for (index in c("tonar", "estr", "sofr", "sonia")) {
    curve <- build_ois_curve("2026-11-02", "2026-11-02", index, "actual_365_fixed", example_instruments())
    repricing <- quote_repricing(curve)
    expect_true(all(repricing$reproduction_error <= 1e-9), info = index)
    expect_identical(discount(curve, "2026-11-02"), 1)
  }
})

test_that("a curve stays the same value after with_extrapolation", {
  curve <- build_ois_curve("2026-11-02", "2026-11-02", "tonar", "actual_365_fixed", example_instruments())
  far <- as.Date("2060-01-01")
  extended <- with_extrapolation(curve, TRUE)
  expect_true(discount(extended, far) > 0)
  expect_error(discount(curve, far), "max curve time")
})

test_that("failing rows stop, or give NA and the message", {
  curve <- build_ois_curve("2026-11-02", "2026-11-02", "tonar", "actual_365_fixed", example_instruments())
  dates <- as.Date(c("2027-01-01", "2060-01-01"))
  expect_error(discount(curve, dates), "row 2")
  expect_warning(values <- discount(curve, dates, on_error = "na"), "1 rows failed")
  expect_false(is.na(values[1]))
  expect_true(is.na(values[2]))
})

test_that("calendars keep the failures of QuantLib", {
  expect_error(is_business_day("russia_moex", "2005-06-01"), "does not exist")
  expect_true(is_business_day("russia_moex", "2020-06-01"))
  expect_false(is_business_day("brazil_exchange", "2026-12-24"))
  expect_false(is_business_day("germany_euwax", "2026-12-25"))
})

test_that("the printed quickstart runs", {
  path <- system.file("examples", "quickstart.R", package = "RiemannFinMath")
  expect_true(nzchar(path))
  expect_output(source(path, local = TRUE), "Settlement date")
})

test_that("a defect in Rust stays visible and leaves R running", {
  # the panic itself is printed on stderr by the default hook, which the package does not change
  expect_error(.Call(RiemannFinMath:::wrap__panic_for_test), "deliberate panic")
  expect_identical(is_business_day("japan", "2026-11-02"), TRUE)
})

test_that("a simple or continuous yield has no Macaulay duration, and the other measures stay", {
  bond <- example_bond()
  values <- bond_values(bond, 0.02, "act_act_isma", "continuous", 1L, "2026-11-04")
  expect_true(is.na(values$duration_macaulay))
  expect_false(is.na(values$clean_price))
  expect_false(is.na(values$duration_modified))
  expect_true(is.na(values$error))
  expect_error(duration(bond, 0.02, "act_act_isma", "continuous", 1L, "2026-11-04", "macaulay"))
  expect_gt(duration(bond, 0.02, "act_act_isma", "compounded", 2L, "2026-11-04", "macaulay"), 0)
})

batch_bonds <- function(n = 40L) {
  tibble::tibble(
    settlement_days = 1L, face_amount = 100, effective_date = as.Date("2020-03-20"), termination_date = as.Date("2030-03-20"),
    tenor = "6M", calendar = "japan", convention = "following", termination_convention = "following", rule = "backward",
    end_of_month = FALSE, coupon_rate = 0.015, day_count = "act_act_isma", payment_convention = "following", redemption = 100,
    issue_date = as.Date("2020-03-20"), rate = seq(0.001, 0.05, length.out = n), yield_day_count = "act_act_isma",
    compounding = "compounded", frequency = 2L, evaluation_date = as.Date("2026-11-02")
  )
}

batch_curves <- function() {
  indices <- rep(c("tonar", "estr", "sofr", "sonia"), each = 5L)
  tibble::tibble(
    evaluation_date = as.Date("2026-11-02"), reference_date = as.Date("2026-11-02"), index = indices, curve_day_count = "actual_365_fixed",
    instruments = lapply(seq_along(indices), function(i) ois_instruments(c("ON", "1Y", "2Y", "5Y", "10Y"), c(0.002, 0.004, 0.005, 0.009, 0.014) + i / 1e4, 2L, 1L, 0L)),
    dates = lapply(seq_along(indices), function(i) as.Date("2026-11-02") + c(30L, 365L, 1000L))
  )
}

test_that("batch results agree with direct calls within the tolerances, row by row", {
  # batches run on worker threads and direct calls on R's main thread, with another x87 precision

  bonds <- batch_bonds()
  batch <- bond_values_batch(bonds, threads = 4L)
  single <- bond_values(example_bond(), bonds$rate, "act_act_isma", "compounded", 2L, batch$settlement_date)
  expect_equal(batch$clean_price, single$clean_price, tolerance = 1e-12)
  expect_equal(batch$duration_modified, single$duration_modified, tolerance = 1e-12)

  curves <- batch_curves()
  values <- curve_values_batch(curves, threads = 4L)
  expect_equal(nrow(values), 60L)
  one <- build_ois_curve("2026-11-02", "2026-11-02", curves$index[7], "actual_365_fixed", curves$instruments[[7]])
  expect_equal(values$discount[values$curve == 7L], discount(one, curves$dates[[7]]), tolerance = 1e-12)
})

test_that("the number of threads does not change any result", {
  bonds <- batch_bonds()
  curves <- batch_curves()
  first_bonds <- bond_values_batch(bonds, threads = 1L)
  first_curves <- curve_values_batch(curves, threads = 1L)
  for (threads in c(2L, 4L, 8L, 0L)) {
    expect_identical(bond_values_batch(bonds, threads = threads), first_bonds, info = as.character(threads))
    expect_identical(curve_values_batch(curves, threads = threads), first_curves, info = as.character(threads))
  }
})

test_that("a row that fails does not affect the others", {
  bonds <- batch_bonds(3L)
  bonds$evaluation_date[2] <- as.Date("2199-12-31")
  values <- bond_values_batch(bonds, threads = 2L)
  expect_false(is.na(values$clean_price[1]))
  expect_false(is.na(values$error[2]))
  expect_false(is.na(values$clean_price[3]))

  curves <- batch_curves()[1:2, ]
  curves$instruments[[2]]$rate[1] <- 1.5
  values <- curve_values_batch(curves, threads = 2L)
  expect_false(is.na(values$discount[1]))
  failed <- values[values$curve == 2L, ]
  expect_equal(nrow(failed), 1L)
  expect_match(failed$error, "root not bracketed")
})

test_that("logical arguments are logical, and NA is an error and never FALSE", {
  args <- list(settlement_days = 1L, face_amount = 100, effective_date = "2020-03-20", termination_date = "2030-03-20",
               tenor = "6M", calendar = "japan", convention = "following", termination_convention = "following",
               rule = "backward", coupon_rate = 0.015, day_count = "act_act_isma", payment_convention = "following", redemption = 100)
  expect_error(do.call(fixed_rate_bond, c(args, list(end_of_month = NA))), "TRUE or FALSE")
  expect_error(do.call(fixed_rate_bond, c(args, list(end_of_month = 1L))), "TRUE or FALSE")
  expect_error(do.call(fixed_rate_bond, c(args, list(end_of_month = "FALSE"))), "TRUE or FALSE")

  bonds <- batch_bonds(3L)
  bonds$end_of_month[2] <- NA
  expect_error(bond_values_batch(bonds), "missing value in row 2")
  bonds$end_of_month <- c(0L, 0L, 0L)
  expect_error(bond_values_batch(bonds), "must be logical")

  curve <- build_ois_curve("2026-11-02", "2026-11-02", "tonar", "actual_365_fixed", example_instruments())
  expect_error(with_extrapolation(curve, NA), "TRUE or FALSE")
  expect_error(holidays("japan", "2026-01-01", "2026-12-31", include_weekends = NA), "TRUE or FALSE")
})

test_that("logical results are logical, and NA means failed", {
  expect_type(is_business_day("japan", "2026-11-02"), "logical")
  expect_type(quote_repricing(build_ois_curve("2026-11-02", "2026-11-02", "tonar", "actual_365_fixed", example_instruments()))$used_in_bootstrap, "logical")
  expect_warning(values <- is_business_day("russia_moex", c("2005-06-01", "2020-06-01"), on_error = "na"), "1 rows failed")
  expect_identical(values, c(NA, TRUE))
})

test_that("batch rows keep the input order for every number of threads", {
  bonds <- batch_bonds(30L)
  shuffled <- bonds[c(30:16, 1:15), ]
  reference <- bond_values_batch(bonds, threads = 1L)
  for (threads in c(1L, 2L, 4L, 8L, 0L)) {
    values <- bond_values_batch(shuffled, threads = threads)
    expect_identical(values$clean_price, reference$clean_price[c(30:16, 1:15)], info = as.character(threads))
  }

  curves <- batch_curves()
  # dates out of order, different in each curve
  curves$dates <- lapply(seq_len(nrow(curves)), function(i) as.Date("2026-11-02") + c(1000L, 30L, 365L, i))
  expected_date <- do.call(c, curves$dates)
  for (threads in c(1L, 2L, 4L, 8L, 0L)) {
    values <- curve_values_batch(curves, threads = threads)
    expect_identical(values$curve, rep(seq_len(nrow(curves)), each = 4L), info = as.character(threads))
    expect_identical(values$date, expected_date, info = as.character(threads))
  }
})
