# Fetch the month-end BB JGB closing prices of Japan Bond Trading (JBTS) and save them normalized.
#
#   Rscript tools/market/fetch_jbts_bb_close.R 2026-09 2026-10
#
# Source: https://www.bb.jbts.co.jp/ja/historical/bb_hikene.html (the page shows the last three months; a month appears
# on the fifth business day of the next month). Writes, under market_data/bb_price/ (ignored by git: market data is
# not committed or shown, see CLAUDE.md):
#
#   raw/bb_hikene_<retrieved date>.html     the page as fetched
#   bb_price_<yyyymmdd>.txt                 one table per month, '|'-separated with a header; yyyymmdd is the month's
#                                           reference date (the site's reference-date column, most rows)
#   PROVENANCE.md                           what was fetched and when
#
# Each month's tables are classified by their issue names, not by their position (a month may lack a table). Values
# are kept as published: read as text, nothing rounded or recomputed; "-" and blanks become empty fields.

suppressPackageStartupMessages({
  library(dplyr)
  library(purrr)
  library(stringr)
  library(tibble)
  library(readr)
  library(rvest)
})

source_url <- "https://www.bb.jbts.co.jp/ja/historical/bb_hikene.html"
out_dir <- file.path("market_data", "bb_price")

columns <- c(
  "month", "reference_date", "category", "issue_label", "bond_type", "series", "when_issued",
  "coupon_pct", "remaining_years", "maturity_date", "simple_yield_pct", "compound_yield_pct", "price"
)
published_columns <- c(
  "issue_label", "coupon_pct", "remaining_years", "maturity_date",
  "simple_yield_pct", "compound_yield_pct", "price", "reference_date"
)
bond_types <- c(
  "40Y" = "JGB40", "30Y" = "JGB30", "20Y" = "JGB20", "10Y" = "JGB10",
  "5Y" = "JGB5", "2Y" = "JGB2", "TDB" = "TDB", "TIPS" = "JGBi10"
)

# Category, series and when-issued flag from a published issue label: 40/19, 384, WI10-11, T1414/Y, TIPS10/31.
classify_issue <- function(label) {
  tibble(issue_label = label) |>
    mutate(
      category = case_when(
        str_detect(issue_label, "^WI\\d+-\\d+$") ~ str_c(str_match(issue_label, "^WI(\\d+)-")[, 2], "Y"),
        str_detect(issue_label, "^TIPS10/\\d+$") ~ "TIPS",
        str_detect(issue_label, "^T\\d+/\\w+$") ~ "TDB",
        str_detect(issue_label, "^(40|30|20|5|2)/\\d+$") ~ str_c(str_extract(issue_label, "^\\d+"), "Y"),
        str_detect(issue_label, "^\\d+$") ~ "10Y",
        .default = NA_character_
      ),
      series = case_when(
        str_detect(issue_label, "^WI") ~ str_c("WI-", str_extract(issue_label, "\\d+$")),
        str_detect(issue_label, "^TIPS10/") ~ str_remove(issue_label, "^TIPS10/"),
        str_detect(issue_label, "^T\\d") ~ str_remove(issue_label, "^T"),
        str_detect(issue_label, "/") ~ str_extract(issue_label, "\\d+$"),
        .default = issue_label
      ),
      when_issued = str_detect(issue_label, "^WI")
    )
}

clean_number <- function(x) if_else(str_trim(x) %in% c("", "-"), NA_character_, str_trim(x))
clean_date <- function(x) {
  if_else(str_trim(x) %in% c("", "-"), NA_character_, format(as.Date(str_trim(x), format = "%Y/%m/%d")))
}

# Rows of each table on the page, as published (text), in page order.
read_tables <- function(page) {
  page |>
    html_elements("table.tbCore") |>
    map(\(table) {
      table |>
        html_elements("tr") |>
        map(\(row) html_text2(html_elements(row, "td"))) |>
        keep(\(cells) length(cells) == 8L) |>
        map(\(cells) set_names(as.list(cells), published_columns)) |>
        bind_rows()
    }) |>
    keep(\(tbl) nrow(tbl) > 0L) |>
    map(\(tbl) filter(tbl, !issue_label %in% c("銘柄名（回号）", "-"))) |>
    keep(\(tbl) nrow(tbl) > 0L)
}

# The tables come month by month, newest first, each month starting with the 40-year table.
parse_page <- function(page) {
  months <- page |>
    html_elements("a[href^='#hikene0']") |>
    html_text2() |>
    unique() |>
    str_replace("/", "-")
  tables <- read_tables(page)
  month_index <- tables |>
    map_chr(\(tbl) classify_issue(tbl$issue_label[[1]])$category) |>
    (\(first_category) cumsum(first_category == "40Y"))()
  if (max(month_index) != length(months)) {
    stop(sprintf("%d groups of tables for %d months", max(month_index), length(months)), call. = FALSE)
  }
  map2(tables, months[month_index], \(tbl, month) mutate(tbl, month = month)) |>
    bind_rows() |>
    left_join(classify_issue(unique(tables |> map("issue_label") |> unlist())), by = "issue_label", relationship = "many-to-one") |>
    mutate(
      bond_type = unname(bond_types[category]),
      when_issued = if_else(when_issued, "TRUE", "FALSE"),
      across(c(coupon_pct, remaining_years, simple_yield_pct, compound_yield_pct, price), clean_number),
      across(c(maturity_date, reference_date), clean_date)
    ) |>
    select(all_of(columns))
}

write_month <- function(rows) {
  main_reference_date <- rows |>
    count(reference_date, sort = TRUE) |>
    slice(1) |>
    pull(reference_date)
  path <- file.path(out_dir, str_c("bb_price_", str_remove_all(main_reference_date, "-"), ".txt"))
  write_delim(rows, path, delim = "|", na = "", eol = "\n")
  path
}

main <- function(wanted) {
  retrieved <- Sys.time()
  dir.create(file.path(out_dir, "raw"), recursive = TRUE, showWarnings = FALSE)
  raw_path <- file.path(out_dir, "raw", str_c("bb_hikene_", format(retrieved, "%Y-%m-%d"), ".html"))
  download.file(source_url, raw_path, quiet = TRUE, mode = "wb")
  page <- read_html(raw_path, encoding = "UTF-8")
  data <- parse_page(page)
  available <- unique(data$month)
  if (anyNA(data$category)) stop("unknown issue labels: ", str_c(unique(data$issue_label[is.na(data$category)]), collapse = ", "), call. = FALSE)

  log <- wanted |>
    map_chr(\(month_wanted) {
      if (!month_wanted %in% available) {
        message(sprintf("%s: not on the page (available: %s)", month_wanted, str_c(available, collapse = ", ")))
        return(sprintf("| %s | not published yet (the page has %s) | | | |", month_wanted, str_c(available, collapse = ", ")))
      }
      rows <- filter(data, month == month_wanted)
      path <- write_month(rows)
      reference_dates <- sort(unique(rows$reference_date))
      per_category <- rows |>
        count(category) |>
        mutate(category = factor(category, levels = names(bond_types))) |>
        arrange(category) |>
        glue::glue_data("{category} {n}") |>
        str_c(collapse = ", ")
      message(sprintf("%s: %d rows -> %s  reference dates %s", month_wanted, nrow(rows), path, str_c(reference_dates, collapse = ", ")))
      sprintf("| %s | %s | %d | %s | %s |", month_wanted, basename(path), nrow(rows), str_c(reference_dates, collapse = ", "), per_category)
    })

  provenance <- file.path(out_dir, "PROVENANCE.md")
  if (!file.exists(provenance)) {
    write_lines(c(
      "# JBTS BB JGB closing prices", "",
      sprintf("Source: %s (Japan Bond Trading Co., month-end BB closing prices). Fetched by `tools/market/fetch_jbts_bb_close.R`.", source_url),
      "Market data: kept out of git (`.gitignore`) and not shown in tables or output (CLAUDE.md). Check the site's terms before any redistribution.", "",
      sprintf("Files: `bb_price_<yyyymmdd>.txt`, UTF-8, `|`-separated with a header. Columns: %s. Values as published; empty when not published (when-issued rows have no coupon or price). `reference_date` is the site's reference-date column, as published.",
              str_c("`", columns, "`", collapse = ", ")), "",
      "| month | file | rows | reference dates | rows per category |", "|---|---|---|---|---|"
    ), provenance)
  }
  write_lines(c(sprintf("| fetched %s | raw/%s | | | |", format(retrieved, "%Y-%m-%d %H:%M %z"), basename(raw_path)), log), provenance, append = TRUE)
  invisible(data)
}

args <- commandArgs(trailingOnly = TRUE)
main(if (length(args)) args else c("2026-09", "2026-10", "2026-11"))
