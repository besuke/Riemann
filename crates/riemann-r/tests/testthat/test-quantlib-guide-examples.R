# The QuantLib Guide example scripts run from the installed package, and their exact rows agree.

guide_script <- function(name) system.file("examples", "quantlib_guide", name, package = "RiemannFinMath")

test_that("the example scripts are installed with the package", {
  for (name in c("bonds.R", "cds.R", "curves.R", "dates.R", "options.R", "spreads.R", "README.md")) expect_true(nzchar(guide_script(name)), info = name)
})

test_that("the bond example runs and its unrounded Guide values agree", {
  r <- NULL
  capture.output(r <- source(guide_script("bonds.R"), local = new.env())$value)
  exact <- r[grepl("Clean price at 3.5%|duration|Convexity|Accrued amount per 100 at|9 May 2027", r$item), ]
  expect_lt(max(abs(exact$riemann_minus_guide)), 1e-10)
})

test_that("the spread example reproduces the z-spread", {
  r <- NULL
  capture.output(r <- source(guide_script("spreads.R"), local = new.env())$value)
  expect_lt(abs(r$riemann_minus_guide[startsWith(r$item, "Z-spread")]), 1e-8)
  if (requireNamespace("QuantLib", quietly = TRUE)) expect_lt(abs(r$riemann_minus_quantlib[startsWith(r$item, "I-spread")]), 1e-8)
})

test_that("the option example runs", {
  r <- NULL
  capture.output(r <- source(guide_script("options.R"), local = new.env())$value)
  analytic <- r[r$note == "", ]
  expect_lt(max(abs(analytic$riemann_minus_guide)), 1e-10)
})

test_that("the curve example reproduces the Guide's discount factors and prices", {
  r <- NULL
  capture.output(r <- source(guide_script("curves.R"), local = new.env())$value)
  expect_lt(max(abs(r$riemann_minus_guide[r$note == "" | grepl("moves the reference date", r$note)])), 1e-12)
})

test_that("the CDS example reproduces the Guide's nodes and survival curve, up to its version differences", {
  r <- NULL
  capture.output(r <- source(guide_script("cds.R"), local = new.env())$value)
  s <- r$spread_nodes
  expect_equal(round(s$riemann_hazard_pct, 2), s$guide_hazard_pct)
  expect_equal(s$node, s$guide_node)
  u <- r$upfront_nodes
  expect_equal(u$node, u$guide_node)
  # the Guide's prints, 2 decimals; one hazard rate (19 September, first node) and the default
  # probabilities differ from QuantLib 1.42 by up to 0.011 and 0.009 points
  expect_lt(max(abs(u$riemann_hazard_pct - u$guide_hazard_pct)), 0.011)
  expect_true(all(u$riemann_default_pct - u$guide_default_pct >= 0 & u$riemann_default_pct - u$guide_default_pct < 0.0095))
  # the survival-probability curve matches the Guide's print
  expect_equal(r$checks$riemann[8:9], r$checks$guide[8:9], tolerance = 1e-14)
  # the check CDS is fair at the quote: worth 0 with its fair upfront the quote
  expect_lt(max(abs(r$checks$riemann[c(1, 4)])), 1e-9)
  expect_equal(r$checks$riemann[c(2, 5)], c(-0.008, -0.008), tolerance = 1e-12)
  if (requireNamespace("QuantLib", quietly = TRUE)) {
    expect_lt(max(abs(u$riemann_hazard_pct - u$quantlib_hazard_pct)), 1e-10)
    expect_lt(max(abs(r$checks$riemann - r$checks$quantlib_local)), 1e-10)
  }
})

test_that("the dates example matches every printed value", {
  r <- NULL
  capture.output(r <- source(guide_script("dates.R"), local = new.env())$value)
  expect_true(all(r$same))
})
