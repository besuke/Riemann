# SAMPLE: fetch TONAR / SOFR / €STR OIS par rates (1Y, 2Y, 5Y, 10Y) from Bloomberg Data License (REST, HistoryRequest).
#
#   Rscript tools/market/fetch_bbg_dl_ois.R            # dry run: build the request, write SYNTHETIC data (no network)
#   Rscript tools/market/fetch_bbg_dl_ois.R --live     # send the request to Data License (needs credentials + httr2)
#
# Status: [design draft] The request body, the endpoints and the token flow follow Bloomberg's public Data License
# REST (Hypermedia API) samples as remembered; they have NOT been checked against a live account. The tickers are
# candidates and must be confirmed on the Data License side. The dry run never touches the network.
#
# Live mode reads:
#   BBG_DL_CREDENTIALS   path to the credential JSON issued by the Data License console (client_id, client_secret)
#   BBG_DL_CATALOG       the account's catalog id (default "bbg" is a placeholder)
#
# Writes, under market_data/bbg_dl/ (ignored by git; market quotes are not committed or shown, see CLAUDE.md):
#   request_<id>.json                       the HistoryRequest body
#   ois_px_last_<mode>_<start>_<end>.csv    long format: date, currency, index, tenor, ticker, field, value, source
#   ois_px_last_<mode>_<start>_<end>.parquet
# In the dry run <mode> is "synthetic" and source is "SYNTHETIC": the values are a seeded random walk, not quotes.

suppressPackageStartupMessages({
  library(dplyr)
  library(purrr)
  library(tibble)
  library(readr)
  library(jsonlite)
})

args <- commandArgs(trailingOnly = TRUE)
live <- "--live" %in% args

start_date <- as.Date("2025-11-15")
end_date <- as.Date("2026-11-15")
field <- "PX_LAST"
pricing_source <- "BGN"
out_dir <- file.path("market_data", "bbg_dl")

# Candidate tickers (to be confirmed in Data License). Overnight fixings for reference: MUTKCALM Index (TONA),
# SOFRRATE Index (SOFR), ESTRON Index (€STR).
universe <- tribble(
  ~currency, ~index,  ~prefix,
  "JPY",     "TONAR", "JYSO",
  "USD",     "SOFR",  "USOSFR",
  "EUR",     "ESTR",  "EESWE"
) |>
  tidyr::crossing(tenor_years = c(1L, 2L, 5L, 10L)) |>
  mutate(tenor = paste0(tenor_years, "Y"), ticker = paste0(prefix, tenor_years, " Curncy")) |>
  select(currency, index, tenor, tenor_years, ticker)

# ---- request ---------------------------------------------------------------------------------------------------

create_history_request <- function(tickers, field, start_date, end_date, pricing_source, name) {
  list(
    `@type` = "HistoryRequest",
    name = name,
    description = "RiemannFinMath OIS par rates (sample)",
    universe = list(
      `@type` = "Universe",
      contains = map(tickers, \(t) list(`@type` = "Identifier", identifierType = "TICKER", identifierValue = t))
    ),
    fieldList = list(`@type` = "HistoryFieldList", contains = list(list(mnemonic = field))),
    trigger = list(`@type` = "SubmitTrigger"),
    runtimeOptions = list(
      `@type` = "HistoryRuntimeOptions",
      dateRange = list(
        `@type` = "IntervalDateRange",
        startDate = format(start_date), endDate = format(end_date)
      )
    ),
    pricingSourceOptions = list(`@type` = "HistoryPricingSourceOptions", prefer = list(mnemonic = pricing_source)),
    formatting = list(`@type` = "MediaType", outputMediaType = "application/json")
  )
}

# ---- normalize -------------------------------------------------------------------------------------------------

# Data License history output: one record per (identifier, date) with the field as a column. Column names are
# matched case-insensitively because the exact output header has not been checked.
normalize_history <- function(records, universe, field, source) {
  names(records) <- toupper(names(records))
  id_col <- intersect(c("IDENTIFIER", "SECURITY", "TICKER"), names(records))[1]
  stopifnot(!is.na(id_col), "DATE" %in% names(records), toupper(field) %in% names(records))
  tibble(
    ticker = records[[id_col]],
    date = as.Date(records[["DATE"]]),
    value = suppressWarnings(as.numeric(records[[toupper(field)]]))
  ) |>
    inner_join(universe, by = "ticker") |>
    transmute(date, currency, index, tenor, ticker, field = field, value, source = source) |>
    filter(!is.na(value)) |>
    arrange(currency, factor(tenor, levels = unique(universe$tenor)), date)
}

# ---- synthetic (dry run) ---------------------------------------------------------------------------------------

generate_synthetic_records <- function(universe, start_date, end_date, field, seed = 20261115L) {
  set.seed(seed)
  days <- seq(start_date, end_date, by = "day")
  days <- days[!format(days, "%u") %in% c("6", "7")]  # weekdays only; no holiday calendar in the sample
  base <- c(JPY = 0.9, USD = 3.6, EUR = 2.1)            # arbitrary levels in percent, not market quotes
  universe |>
    pmap_dfr(\(currency, index, tenor, tenor_years, ticker) {
      level <- base[[currency]] + 0.08 * log(tenor_years)
      tibble(IDENTIFIER = ticker, DATE = format(days), !!field := round(level + cumsum(rnorm(length(days), 0, 0.02)), 4))
    })
}

# ---- live (Data License REST) ----------------------------------------------------------------------------------

dl_base <- "https://api.bloomberg.com"
dl_token_url <- "https://bsso.blpprofessional.com/ext/api/as/token.oauth2"

fetch_live_records <- function(body, request_id) {
  if (!requireNamespace("httr2", quietly = TRUE)) stop("--live needs the httr2 package: install.packages('httr2')")
  cred_path <- Sys.getenv("BBG_DL_CREDENTIALS")
  if (!nzchar(cred_path) || !file.exists(cred_path)) stop("set BBG_DL_CREDENTIALS to the credential JSON file")
  cred <- fromJSON(cred_path)
  catalog <- Sys.getenv("BBG_DL_CATALOG", "bbg")
  client <- httr2::oauth_client(id = cred$client_id, secret = cred$client_secret, token_url = dl_token_url)

  dl_req <- function(path) {
    httr2::request(dl_base) |>
      httr2::req_url_path(path) |>
      httr2::req_headers(`api-version` = "2") |>
      httr2::req_oauth_client_credentials(client)
  }

  # 1. submit
  dl_req(sprintf("/eap/catalogs/%s/requests/", catalog)) |>
    httr2::req_body_json(body, auto_unbox = TRUE) |>
    httr2::req_perform()
  message("submitted ", request_id, "; waiting for the output")

  # 2. poll the responses for this request (history requests may take minutes)
  key <- NULL
  for (i in seq_len(60)) {
    resp <- dl_req(sprintf("/eap/catalogs/%s/content/responses/", catalog)) |>
      httr2::req_url_query(requestIdentifier = request_id) |>
      httr2::req_perform() |>
      httr2::resp_body_json()
    if (length(resp$contains) > 0) { key <- resp$contains[[1]]$key; break }
    Sys.sleep(30)
  }
  if (is.null(key)) stop("no output for ", request_id, " after 30 minutes")

  # 3. download (keep the raw file next to the normalized output)
  raw_path <- file.path(out_dir, "raw", basename(key))
  dir.create(dirname(raw_path), recursive = TRUE, showWarnings = FALSE)
  dl_req(sprintf("/eap/catalogs/%s/content/responses/%s", catalog, key)) |>
    httr2::req_perform(path = raw_path)
  txt <- if (grepl("\\.gz$", raw_path)) readLines(gzfile(raw_path), warn = FALSE) else readLines(raw_path, warn = FALSE)
  as_tibble(fromJSON(paste(txt, collapse = "\n"), flatten = TRUE))
}

# ---- main ------------------------------------------------------------------------------------------------------

dir.create(out_dir, recursive = TRUE, showWarnings = FALSE)
request_id <- paste0("rfmOis", format(Sys.time(), "%Y%m%d%H%M%S"))
body <- create_history_request(universe$ticker, field, start_date, end_date, pricing_source, request_id)
body$identifier <- request_id
write_json(body, file.path(out_dir, paste0("request_", request_id, ".json")), auto_unbox = TRUE, pretty = TRUE)

mode <- if (live) "live" else "synthetic"
records <- if (live) fetch_live_records(body, request_id) else generate_synthetic_records(universe, start_date, end_date, field)
rates <- normalize_history(records, universe, field, source = if (live) paste0("BBG_DL_", pricing_source) else "SYNTHETIC")

stem <- file.path(out_dir, sprintf("ois_px_last_%s_%s_%s", mode, format(start_date, "%Y%m%d"), format(end_date, "%Y%m%d")))
write_csv(rates, paste0(stem, ".csv"))
if (requireNamespace("arrow", quietly = TRUE)) arrow::write_parquet(rates, paste0(stem, ".parquet"))

# Counts only: quotes are not printed (CLAUDE.md).
summary_tbl <- rates |> count(currency, index, tenor, name = "rows") |>
  arrange(currency, as.integer(sub("Y", "", tenor)))
message(sprintf("mode=%s  rows=%d  dates %s..%s", mode, nrow(rates), min(rates$date), max(rates$date)))
print(summary_tbl, n = Inf)
message("wrote ", stem, ".{csv,parquet}")
