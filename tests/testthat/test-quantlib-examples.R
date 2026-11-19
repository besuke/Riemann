# The QuantLib official example scripts run from the installed package.

example_script <- function(name) system.file("examples", "quantlib_examples", name, package = "rmql")

test_that("the example scripts are installed with the package", {
  purrr::walk(c("equity_option.R", "README.md"), \(name) expect_true(nzchar(example_script(name)), info = name))
})

test_that("the EquityOption example runs and agrees with QuantLib", {
  r <- NULL
  capture.output(r <- source(example_script("equity_option.R"), local = new.env())$value)
  expect_equal(nrow(r), 5)
  expect_true(all(is.finite(r$rmql)))
  # an American put is worth at least the European one
  expect_gt(min(r$rmql[r$method != "Black-Scholes" & !(r$method == "Binomial Cox-Ross-Rubinstein" & r$exercise == "european")]),
            r$rmql[r$method == "Black-Scholes"])
  if (requireNamespace("QuantLib", quietly = TRUE)) expect_lt(max(abs(r$rmql_minus_quantlib)), 1e-10)
})
