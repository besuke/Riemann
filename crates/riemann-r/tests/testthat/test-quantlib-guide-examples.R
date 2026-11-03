# The QuantLib Guide example scripts run from the installed package, and their exact rows agree.

guide_script <- function(name) system.file("examples", "quantlib_guide", name, package = "RiemannFinMath")

test_that("the example scripts are installed with the package", {
  for (name in c("bonds.R", "curves.R", "options.R", "spreads.R", "README.md")) expect_true(nzchar(guide_script(name)), info = name)
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
  expect_lt(abs(r$riemann_minus_guide[1]), 1e-8)
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
