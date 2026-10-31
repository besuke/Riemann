# Helpers shared by verify_bonds.R and verify_ois.R. Source this file first.

suppressPackageStartupMessages({
  library(tidyverse)
  library(QuantLib)
})

find_riemann_root <- function(start = getwd()) {
  candidates <- accumulate(seq_len(8), \(path, i) dirname(path), .init = normalizePath(start, winslash = "/"))
  root <- detect(candidates, \(path) file.exists(file.path(path, "Cargo.toml")))
  if (is.null(root)) stop("Riemann root not found from: ", start, call. = FALSE)
  root
}

date_to_serial <- function(d) as.integer(as.numeric(d) + 25569)
serial_to_date <- function(serial) as.Date(serial - 25569, origin = "1970-01-01")
ql_date <- function(serial) QuantLib::Date(as.integer(serial))
format_number <- function(x) sprintf("%.17g", x)
to_number <- function(x) suppressWarnings(as.numeric(x)) # ERROR becomes NA

cargo_path <- function() {
  on_path <- Sys.which("cargo")
  if (nzchar(on_path)) return(unname(on_path))
  file.path(Sys.getenv("USERPROFILE"), ".cargo", "bin", "cargo.exe")
}

# Runs an example program of the riemann crate on an input CSV. Windows can report a transient
# LNK1104 for a new executable; one retry is enough.
run_riemann_example <- function(example, input_path, output_path, root, attempts = 2L) {
  arguments <- c("run", "--release", "--quiet", "--example", example, "--", input_path, output_path)
  owd <- setwd(root)
  on.exit(setwd(owd), add = TRUE)
  log <- map(seq_len(attempts), \(i) {
    if (file.exists(output_path)) return(0L)
    suppressWarnings(system2(cargo_path(), arguments, stdout = FALSE, stderr = FALSE))
  })
  if (!file.exists(output_path)) stop(example, " example failed after ", attempts, " attempts.", call. = FALSE)
  invisible(log)
}

# The R package of Riemann, called directly. Install it with: R CMD INSTALL crates/riemann-r
require_riemann_package <- function() {
  if (!requireNamespace("riemann", quietly = TRUE)) {
    stop("The riemann package is not installed. Run: R CMD INSTALL crates/riemann-r", call. = FALSE)
  }
  invisible(TRUE)
}

# Compares the same values from the direct call and through CSV. The CSV path writes numbers as
# text, so R's reading of them can move the last bits.
compare_direct_with_csv <- function(direct, csv, by, value = "value") {
  joined <- full_join(rename(direct, direct = all_of(value)), rename(csv, csv = all_of(value)), by = by)
  tibble(
    compared = nrow(joined),
    exact = sum(joined$direct == joined$csv, na.rm = TRUE),
    both_error = sum(is.na(joined$direct) & is.na(joined$csv)),
    one_sided = sum(is.na(joined$direct) != is.na(joined$csv)),
    max_abs_diff = if (any(!is.na(joined$direct) & !is.na(joined$csv))) max(abs(joined$direct - joined$csv), na.rm = TRUE) else 0
  )
}
