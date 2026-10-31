# Translates QuantLib C++ calendars to Rust, so that the conditions are not retyped by hand.
#
# Usage (from the Riemann root): Rscript tools/golden/port_calendars.R <quantlib-v1.42>
# Writes crates/riemann/src/time/calendar/rules/{americas,europe,middle_east_africa,asia_pacific,tables}.rs.
# Israel, Saudi Arabia, Russia and China's interbank market are written by hand in special.rs,
# because they use date arithmetic, switches or sets; their date tables are generated here.
# The result is checked against QuantLib C++ on every valid date by tests/golden_all_calendars.rs.

args <- commandArgs(trailingOnly = TRUE)
stopifnot(length(args) == 1)
source_dir <- file.path(args[1], "ql", "time", "calendars")
out_dir <- file.path("crates", "riemann", "src", "time", "calendar", "rules")
dir.create(out_dir, showWarnings = FALSE)

regions <- list(
  americas = c("argentina", "brazil", "canada", "chile", "mexico"),
  europe = c("austria", "czechrepublic", "denmark", "finland", "france", "germany", "hungary", "iceland", "italy",
             "norway", "poland", "romania", "slovakia", "sweden", "switzerland", "ukraine"),
  middle_east_africa = c("botswana", "southafrica", "turkey"),
  asia_pacific = c("australia", "china", "hongkong", "india", "indonesia", "newzealand", "singapore", "southkorea",
                   "taiwan", "thailand")
)
prefixes <- c(czechrepublic = "czech_republic", hongkong = "hong_kong", newzealand = "new_zealand",
              southafrica = "south_africa", southkorea = "south_korea")
orthodox <- c("romania", "ukraine")
skipped <- c(china = "Ib") # written by hand

month_names <- c(January = 1, February = 2, March = 3, April = 4, May = 5, June = 6, July = 7, August = 8,
                 September = 9, October = 10, November = 11, December = 12, Jan = 1, Feb = 2, Mar = 3, Apr = 4,
                 Jun = 6, Jul = 7, Aug = 8, Sep = 9, Oct = 10, Nov = 11, Dec = 12)

read_source <- function(name) {
  text <- readLines(file.path(source_dir, paste0(name, ".cpp")), warn = FALSE, encoding = "UTF-8")
  gsub("\t", "    ", enc2utf8(text))
}

copyrights <- function(name) {
  text <- read_source(name)
  trimws(sub("^ *", "", grep("Copyright \\(C\\)", text, value = TRUE)))
}

snake <- function(x) tolower(gsub("([a-z])([A-Z])", "\\1_\\2", x))
function_name <- function(file, impl) {
  prefix <- if (file %in% names(prefixes)) prefixes[[file]] else file
  if (impl == "") prefix else paste0(prefix, "_", snake(impl))
}

# Position just after the bracket closing the one at `open`, ignoring comments.
closing <- function(chars, open, left, right) {
  depth <- 0L
  i <- open
  while (i <= length(chars)) {
    if (chars[i] == "/" && chars[i + 1L] == "/") {
      while (chars[i] != "\n") i <- i + 1L
    } else if (chars[i] == "/" && chars[i + 1L] == "*") {
      i <- i + 2L
      while (!(chars[i] == "*" && chars[i + 1L] == "/")) i <- i + 1L
      i <- i + 1L
    } else if (chars[i] == left) {
      depth <- depth + 1L
    } else if (chars[i] == right) {
      depth <- depth - 1L
      if (depth == 0L) return(i)
    }
    i <- i + 1L
  }
  stop("unbalanced ", left)
}

# Rewrites one piece of C++ code, leaving comments alone.
translate_code <- function(code, file) {
  lines <- strsplit(code, "\n", fixed = TRUE)[[1]]
  translated <- vapply(lines, function(line) {
    at <- regexpr("//", line, fixed = TRUE)
    head <- if (at > 0) substr(line, 1, at - 1) else line
    tail <- if (at > 0) substr(line, at, nchar(line)) else ""
    tail <- sub(" *// NOLINT.*$", "", tail)
    tail <- sub("^///+", "//", tail) # not documentation comments in Rust
    for (name in names(month_names)) head <- gsub(paste0("\\b", name, "\\b"), month_names[[name]], head)
    head <- gsub("\\bisWeekend\\(w\\)", "is_weekend(w)", head)
    head <- gsub("\\bisAboriginalPeopleDay\\(", "is_aboriginal_people_day(", head)
    head <- gsub("\\b0([1-9])\\b", "\\1", head)
    calls <- gregexpr("([A-Za-z]+::)?[A-Za-z]*Impl::isBusinessDay\\(date\\)", head)
    hits <- regmatches(head, calls)[[1]]
    for (hit in hits) {
      impl <- sub("Impl.*$", "", sub("^.*::([A-Za-z]*Impl)::.*$", "\\1", paste0("::", hit)))
      head <- sub(hit, paste0(function_name(file, impl), "(x)"), head, fixed = TRUE)
    }
    paste0(head, tail)
  }, "")
  paste(translated, collapse = "\n")
}

skip_space <- function(chars, i) {
  while (i <= length(chars) && chars[i] %in% c(" ", "\n", "\r")) i <- i + 1L
  i
}

# Translates a statement list to Rust lines, with comments kept in place.
translate_statements <- function(text, file) {
  chars <- strsplit(text, "")[[1]]
  out <- character()
  i <- 1L
  emit <- function(...) out <<- c(out, ...)
  statement_at <- function(i) {
    i <- skip_space(chars, i)
    if (i > length(chars)) return(list(lines = character(), next_at = i))
    rest <- paste(chars[i:min(i + 12L, length(chars))], collapse = "")
    if (startsWith(rest, "//")) {
      end <- i
      while (end <= length(chars) && chars[end] != "\n") end <- end + 1L
      comment <- paste(chars[i:(end - 1L)], collapse = "")
      return(list(lines = sub(" *// NOLINT.*$", "", comment), next_at = end))
    }
    if (chars[i] == "{") {
      end <- closing(chars, i, "{", "}")
      inner <- paste(chars[(i + 1L):(end - 1L)], collapse = "")
      return(list(lines = translate_statements(inner, file), next_at = end + 1L))
    }
    if (grepl("^if *\\(", rest)) {
      open <- i
      while (chars[open] != "(") open <- open + 1L
      end <- closing(chars, open, "(", ")")
      condition <- translate_code(paste(chars[(open + 1L):(end - 1L)], collapse = ""), file)
      body <- character()
      j <- skip_space(chars, end + 1L)
      while (j + 1L <= length(chars) && chars[j] == "/" && chars[j + 1L] == "/") {
        piece <- statement_at(j)
        body <- c(body, piece$lines)
        j <- skip_space(chars, piece$next_at)
      }
      piece <- statement_at(j)
      body <- c(body, piece$lines)
      # a line comment at the end of the condition would swallow the brace
      opening <- if (grepl("//[^\n]*$", trimws(condition))) "\n{" else " {"
      result <- c(paste0("if ", trimws(condition), opening), paste0("    ", body), "}")
      j <- skip_space(chars, piece$next_at)
      if (j + 3L <= length(chars) && paste(chars[j:(j + 3L)], collapse = "") == "else") {
        other <- statement_at(j + 4L)
        tail_lines <- other$lines
        if (startsWith(tail_lines[1], "if ")) {
          result[length(result)] <- paste0("} else ", tail_lines[1])
          result <- c(result, tail_lines[-1])
        } else {
          result[length(result)] <- "} else {"
          result <- c(result, paste0("    ", tail_lines), "}")
        }
        j <- other$next_at
      }
      return(list(lines = result, next_at = j))
    }
    end <- i
    while (chars[end] != ";") end <- end + 1L
    code <- paste(chars[i:(end - 1L)], collapse = "")
    if (grepl("^(const )?(Weekday|Day|Month|Year) ", code)) return(list(lines = character(), next_at = end + 1L))
    if (grepl("^return ", code)) {
      j <- end + 1L
      # a comment on the same line stays with the statement
      return(list(lines = paste0(translate_code(trimws(code), file), ";"), next_at = j))
    }
    stop("unknown statement in ", file, ": ", code)
  }
  while (i <= length(chars)) {
    piece <- statement_at(i)
    emit(piece$lines)
    i <- piece$next_at
  }
  out
}

translate_function <- function(file, impl, body) {
  statements <- translate_statements(body, file)
  code <- sub("//.*$", "", unlist(strsplit(statements, "\n", fixed = TRUE)))
  uses <- function(name) any(grepl(paste0("\\b", name, "\\b"), code))
  fields <- Filter(uses, c("d", "m", "y", "w", "dd"))
  needs_em <- grepl("easterMonday", body)
  if (needs_em && !"y" %in% fields) fields <- c(fields, "y")
  binding <- if (length(fields) == 0) character()
    else if (length(fields) == 1) sprintf("let %s = x.%s;", fields, fields)
    else sprintf("let (%s) = (%s);", paste(fields, collapse = ", "), paste0("x.", fields, collapse = ", "))
  easter <- if (needs_em) sprintf("let em = %s(y);", if (file %in% orthodox) "orthodox_easter_monday" else "easter_monday")
  last <- length(statements)
  if (last > 0 && grepl("^return (true|false);$", statements[last])) statements[last] <- sub("^return (.*);$", "\\1", statements[last])
  c(sprintf("pub(in crate::time::calendar) fn %s(x: &Day) -> bool {", function_name(file, impl)),
    paste0("    ", c(binding, easter, statements)), "}", "")
}

functions_of <- function(file) {
  text <- paste(read_source(file), collapse = "\n")
  starts <- gregexpr("bool [A-Za-z]+::[A-Za-z]*Impl::isBusinessDay\\(\\s*const Date& date\\) const \\{", text)[[1]]
  lengths <- attr(starts, "match.length")
  chars <- strsplit(text, "")[[1]]
  unlist(lapply(seq_along(starts), function(k) {
    header <- substr(text, starts[k], starts[k] + lengths[k] - 1L)
    impl <- sub("Impl$", "", sub("^bool [A-Za-z]+::([A-Za-z]*Impl)::.*$", "\\1", header))
    if (file %in% names(skipped) && impl == skipped[[file]]) return(character())
    open <- starts[k] + lengths[k] - 1L
    end <- closing(chars, open, "{", "}")
    translate_function(file, impl, paste(chars[(open + 1L):(end - 1L)], collapse = ""))
  }))
}

chile_helper <- c(
  "fn is_aboriginal_people_day(d: i32, m: i32, y: i32) -> bool {",
  "    m == 6 && y >= 2021 && d == CHILE_ABORIGINAL_PEOPLE_DAY[(y - 2021) as usize]",
  "}", "")

write_region <- function(region, files) {
  header <- c(
    sprintf("//! Holiday rules of QuantLib C++ v1.42 (commit 41b0e14) for the region %s.", region),
    "//!",
    "//! Generated by `tools/golden/port_calendars.R`, which keeps QuantLib's conditions and comments and",
    "//! writes months as numbers. Checked on every valid date by `tests/golden_all_calendars.rs`.",
    "//! Derived from QuantLib (modified BSD license; see THIRD_PARTY_NOTICES):",
    "//!",
    unlist(lapply(files, function(file) c(sprintf("//! - `ql/time/calendars/%s.cpp`", file), paste0("//!   ", copyrights(file))))),
    "",
    "// The conditions stay as in QuantLib, so that they can be compared with the source.",
    "#![allow(unused_parens)]",
    "#![allow(clippy::manual_range_contains, clippy::nonminimal_bool, clippy::if_same_then_else, clippy::needless_return)]",
    "#![allow(clippy::collapsible_if, clippy::collapsible_else_if)]",
    "",
    if ("chile" %in% files) "use super::tables::CHILE_ABORIGINAL_PEOPLE_DAY;",
    if (any(files %in% orthodox)) "use super::orthodox_easter_monday;",
    "use super::{easter_monday, is_weekend, Day};",
    "#[allow(unused_imports)]",
    "use crate::time::Weekday::{Friday, Monday, Saturday, Sunday, Thursday, Tuesday, Wednesday};",
    ""
  )
  body <- unlist(lapply(files, functions_of))
  if ("chile" %in% files) body <- c(chile_helper, body)
  writeLines(enc2utf8(c(header, body)), file.path(out_dir, paste0(region, ".rs")), useBytes = TRUE)
}

iwalk <- function(x, f) invisible(Map(f, names(x), x))
iwalk(regions, write_region)

# ---- date tables ------------------------------------------------------------------------

date_tuples <- function(text, pattern) {
  hits <- regmatches(text, gregexpr(pattern, text, perl = TRUE))[[1]]
  # every year in the block must have been read, or the pattern has missed a form of date
  code <- gsub("//[^\n]*", "", text)
  years <- regmatches(code, gregexpr("\\b(19|20)[0-9]{2}\\b", code))[[1]]
  stopifnot(length(years) == length(hits))
  parts <- regmatches(hits, regexec(pattern, hits, perl = TRUE))
  vapply(parts, function(p) sprintf("(%s, %d, %d)", p[4], month_names[[p[3]]], as.integer(p[2])), "")
}

block_after <- function(text, anchor) {
  at <- regexpr(anchor, text, fixed = TRUE)
  stopifnot(at > 0)
  chars <- strsplit(substr(text, at, nchar(text)), "")[[1]]
  open <- which(chars == "{")[1]
  end <- closing(chars, open, "{", "}")
  paste(chars[open:end], collapse = "")
}

table_lines <- function(name, tuples) {
  c(sprintf("pub(super) const %s: &[(i32, i32, i32)] = &[", name),
    paste0("    ", vapply(split(tuples, ceiling(seq_along(tuples) / 5)), paste, "", collapse = ", "), ","), "];", "")
}

israel <- paste(read_source("israel"), collapse = "\n")
israel_tables <- c(ISRAEL_PURIM = "bool isPurim", ISRAEL_PASSOVER_1ST = "bool isPassover1st", ISRAEL_INDEPENDENCE_DAY = "bool isIndependenceDay",
                   ISRAEL_SHAVUOT = "bool isShavuot", ISRAEL_FAST_DAY = "bool isFastDay", ISRAEL_NEW_YEARS_DAY = "bool isNewYearsDay")
brace_date <- "\\{ *([0-9]+) *, *([A-Za-z]+) *, *([0-9]{4}) *\\}"
call_date <- "Date\\( *([0-9]+) *, *([A-Za-z]+) *, *([0-9]{4}) *\\)"
saudi <- paste(read_source("saudiarabia"), collapse = "\n")
china <- paste(read_source("china"), collapse = "\n")

chile <- paste(read_source("chile"), collapse = "
")
aboriginal <- block_after(chile, "aboriginalPeopleDay[] = ")
aboriginal <- gsub("//[^
]*", "", aboriginal)
aboriginal <- regmatches(aboriginal, gregexpr("[0-9]+", aboriginal))[[1]]
chile_table <- c("/// Day of June of the National Day of Indigenous Peoples, from 2021 to 2199.",
  sprintf("pub(super) const CHILE_ABORIGINAL_PEOPLE_DAY: [i32; %d] = [", length(aboriginal)),
  paste0("    ", vapply(split(aboriginal, ceiling(seq_along(aboriginal) / 10)), paste, "", collapse = ", "), ","), "];", "")

tables <- c(
  "//! Date tables of QuantLib C++ v1.42 (commit 41b0e14), as (year, month, day).",
  "//!",
  "//! Generated by `tools/golden/port_calendars.R`. Derived from QuantLib (modified BSD license; see",
  "//! THIRD_PARTY_NOTICES):",
  "//!",
  unlist(lapply(c("israel", "saudiarabia", "china", "chile"), function(file) c(sprintf("//! - `ql/time/calendars/%s.cpp`", file), paste0("//!   ", copyrights(file))))),
  "",
  unlist(Map(function(name, anchor) table_lines(name, date_tuples(block_after(israel, paste0(anchor, "(const Date& d)")), brace_date)),
             names(israel_tables), israel_tables)),
  table_lines("SAUDI_EID_AL_ADHA", date_tuples(block_after(saudi, "EidAlAdha = "), call_date)),
  table_lines("SAUDI_EID_AL_FITR", date_tuples(block_after(saudi, "EidAlFitr = "), call_date)),
  table_lines("CHINA_IB_WORKING_WEEKENDS", date_tuples(block_after(china, "workingWeekends = "), call_date)),
  chile_table
)
writeLines(enc2utf8(tables), file.path(out_dir, "tables.rs"), useBytes = TRUE)
cat("written to", out_dir, "\n")
