# Builds a CRAN-like repository of rmql for GitHub Pages (the gh-pages branch,
# https://besuke.github.io/rmql):
#
#   install.packages("rmql",
#                    repos = c("https://besuke.github.io/rmql", "https://cloud.r-project.org"))
#
# On Windows, install.packages() takes the binary when its version is the same as the source package's,
# so neither Rust nor Rtools is needed, and the CRAN dependencies (purrr, tibble) are installed too.
# Elsewhere it takes the source package, which needs Rust. The source package must be there: with
# type = "both" (the Windows default), R looks at the source index first and reports a package that
# is only in the binary index as "not available".
# remotes::install_github() always builds from source and cannot use this repository.
#
# Usage: Rscript tools/package/build_binary_repo.R <zip> <tar.gz> <repository folder>
# The zip goes to bin/windows/contrib/<R major.minor of the zip>/, the tar.gz to src/contrib/, and the
# PACKAGES indexes are rewritten. Older files are kept; install.packages() takes the newest version.

args <- commandArgs(trailingOnly = TRUE)
stopifnot(length(args) == 3L, file.exists(args[1]), file.exists(args[2]))
zip <- normalizePath(args[1], winslash = "/")
source_package <- normalizePath(args[2], winslash = "/")
repository <- args[3]

# The R version a binary was built for is in the Built field of its DESCRIPTION.
listing <- utils::unzip(zip, list = TRUE)$Name
description_path <- grep("^[^/]+/DESCRIPTION$", listing, value = TRUE)
stopifnot(length(description_path) == 1L)
unpacked <- tempfile("description")
utils::unzip(zip, files = description_path, exdir = unpacked)
fields <- read.dcf(file.path(unpacked, description_path), fields = c("Package", "Version", "Built"))
built <- fields[1, "Built"]
r_version <- sub("^R ([0-9]+\\.[0-9]+)\\..*$", "\\1", built)
stopifnot(grepl("^[0-9]+\\.[0-9]+$", r_version), grepl("windows|mingw", built))
# The binary and the source package must be the same version, or Windows would build the newer source.
stopifnot(basename(source_package) == sprintf("%s_%s.tar.gz", fields[1, "Package"], fields[1, "Version"]))

contrib <- file.path(repository, "bin", "windows", "contrib", r_version)
source_contrib <- file.path(repository, "src", "contrib")
dir.create(contrib, recursive = TRUE, showWarnings = FALSE)
dir.create(source_contrib, recursive = TRUE, showWarnings = FALSE)
stopifnot(file.copy(zip, contrib, overwrite = TRUE), file.copy(source_package, source_contrib, overwrite = TRUE))
tools::write_PACKAGES(contrib, type = "win.binary", addFiles = TRUE)
tools::write_PACKAGES(source_contrib, type = "source", addFiles = TRUE)
# GitHub Pages would otherwise run Jekyll on the folder.
file.create(file.path(repository, ".nojekyll"))

cat(sprintf("%s %s (%s) -> %s and %s\n", fields[1, "Package"], fields[1, "Version"], built, contrib, source_contrib))
