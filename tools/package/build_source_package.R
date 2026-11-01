# Builds a self-contained source package of the R package RiemannFinMath, which installs outside this repository and
# without a network: it holds the core crate and its Rust dependencies, and QuantLib's license
# notices. The repository itself keeps its path dependency; only a temporary copy is changed.
#
# Usage (from the Riemann root): Rscript tools/package/build_source_package.R <output directory>
# Needs cargo, and tar and xz (from Rtools on Windows) on the PATH.

args <- commandArgs(trailingOnly = TRUE)
stopifnot(length(args) == 1L, file.exists("crates/riemann-r/DESCRIPTION"))
output <- normalizePath(args[1], winslash = "/", mustWork = FALSE)
dir.create(output, recursive = TRUE, showWarnings = FALSE)

staging <- tempfile("rfm-")
dir.create(staging)
package <- file.path(staging, "RiemannFinMath")
on.exit(unlink(staging, recursive = TRUE), add = TRUE)

# Output goes to a file, and is shown only when the command fails.
run <- function(command, arguments, directory = getwd()) {
  old <- setwd(directory)
  on.exit(setwd(old))
  log <- tempfile(fileext = ".txt")
  status <- suppressWarnings(system2(command, arguments, stdout = log, stderr = log))
  if (status != 0L) {
    writeLines(tail(readLines(log, warn = FALSE), 30L))
    stop(command, " ", paste(arguments, collapse = " "), " failed with status ", status, call. = FALSE)
  }
  invisible(log)
}

copy_tree <- function(from, to) {
  dir.create(to, recursive = TRUE, showWarnings = FALSE)
  copied <- file.copy(list.files(from, full.names = TRUE, all.files = TRUE, no.. = TRUE), to, recursive = TRUE)
  stopifnot(all(copied))
  invisible(copied)
}

# ---- the package, without build products --------------------------------------------------------
copy_tree("crates/riemann-r", package)
unlink(file.path(package, "src/rust/target"), recursive = TRUE)
unlink(list.files(file.path(package, "src"), pattern = "[.](o|dll|so)$", full.names = TRUE))

# ---- the core crate inside the package ----------------------------------------------------------
core <- file.path(package, "src/rust/riemann-core")
dir.create(core, recursive = TRUE)
copy_tree("crates/riemann/src", file.path(core, "src"))
manifest <- readLines("crates/riemann/Cargo.toml")
workspace <- readLines("Cargo.toml")
inherited <- function(field) {
  line <- grep(paste0("^", field, " *="), workspace, value = TRUE)
  stopifnot(length(line) == 1L)
  line
}
for (field in c("version", "edition", "license", "repository", "rust-version")) {
  manifest[grepl(paste0("^", field, "[.]workspace"), manifest)] <- inherited(field)
}
stopifnot(!any(grepl("[.]workspace", manifest)))
writeLines(manifest, file.path(core, "Cargo.toml"))

rust_manifest <- file.path(package, "src/rust/Cargo.toml")
text <- readLines(rust_manifest)
dependency <- grepl("^riemann = \\{ path = ", text)
stopifnot(sum(dependency) == 1L)
text[dependency] <- 'riemann = { path = "riemann-core" }'
writeLines(text, rust_manifest)

# ---- the license notices ------------------------------------------------------------------------
dir.create(file.path(package, "inst"), showWarnings = FALSE)
notices <- c("THIRD_PARTY_NOTICES", "LICENSE-MIT", "LICENSE-APACHE")
stopifnot(all(file.copy(notices, file.path(package, "inst", notices))))

# ---- the Rust dependencies ----------------------------------------------------------------------
rust <- file.path(package, "src/rust")
run("cargo", c("vendor", "--locked", "vendor"), rust)
# the license of each bundled crate; each crate's own files stay in vendor.tar.xz
field_of <- function(lines, field) {
  hit <- grep(paste0("^", field, " *= *\""), lines, value = TRUE)
  if (length(hit)) sub("\"[[:space:]]*$", "", sub("^[^\"]*\"", "", hit[1])) else "not stated"
}
crate_lines <- vapply(list.dirs(file.path(rust, "vendor"), recursive = FALSE), function(directory) {
  lines <- readLines(file.path(directory, "Cargo.toml"), warn = FALSE)
  paste(field_of(lines, "name"), field_of(lines, "version"), field_of(lines, "license"), sep = " | ")
}, "")
writeLines(c("Rust crates bundled in src/rust/vendor.tar.xz of the source package: name | version | license",
             "They are statically linked into the package's library.", "", sort(unname(crate_lines))),
           file.path(package, "inst", "RUST_DEPENDENCIES"))
run("tar", c("-cJf", "vendor.tar.xz", "vendor"), rust)
unlink(file.path(rust, "vendor"), recursive = TRUE)
writeLines(c(
  "[source.crates-io]",
  'replace-with = "vendored-sources"',
  "",
  "[source.vendored-sources]",
  'directory = "vendor"'
), file.path(rust, "vendor-config.toml"))

# ---- the source package -------------------------------------------------------------------------
r_command <- file.path(R.home("bin"), "R")
run(r_command, c("CMD", "build", "--no-build-vignettes", shQuote(package)), output)
built <- list.files(output, pattern = "^RiemannFinMath_.*[.]tar[.]gz$", full.names = TRUE)
stopifnot(length(built) == 1L)
cat("Source package:", built, "\n")
cat("Size in MB:", round(file.size(built) / 1e6, 1), "\n")
