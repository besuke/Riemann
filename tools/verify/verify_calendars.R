# Compares every Riemann calendar with QuantLib C++ v1.42 built from source
# (tests/golden/calendar_*.csv, from tools/golden/p1_all_calendars.cpp), on every valid date, and
# cross-checks the QuantLib R package Version 1.42, whose exact C++ commit is unknown.
# Agreement with QuantLib is not a check against actual market holidays.
#
# Riemann is called directly through its R package RiemannFinMath (crates/riemann-r). The joint and bespoke
# calendars are not in that entry point yet, so they and the comparison use
# crates/riemann/examples/verify_calendars.rs.
#
# Usage from an R session started anywhere in the repository:
#   source("tools/verify/verify_common.R"); source("tools/verify/verify_calendars.R")
#   results <- verify_calendars(); print(results$by_market)

first_serial <- 367L
last_serial <- 109574L

golden_path <- function(root, file) file.path(root, "tests", "golden", file)

calendar_markets <- function(root) {
  read_csv(golden_path(root, "calendar_markets.csv"), comment = "#", na = character(), show_col_types = FALSE)
}

calendar_coverage <- function(root) {
  read_csv(file.path(root, "tools", "verify", "calendar_coverage.csv"), na = "", show_col_types = FALSE)
}

# The C++ values: H and W rows from the region files, E rows from the error ranges.
quantlib_cpp_calendar_values <- function(markets, root) {
  regions <- unique(markets$region)
  days <- map(regions, \(region) {
    read_csv(golden_path(root, paste0("calendar_all_", region, ".csv")), comment = "#", show_col_types = FALSE)
  }) |>
    list_rbind() |>
    rename(key = calendar)
  errors <- read_csv(golden_path(root, "calendar_all_errors.csv"), comment = "#", show_col_types = FALSE) |>
    transmute(key = calendar, serial = map2(first, last, seq.int), kind = "E") |>
    unnest(serial)
  bind_rows(days, errors)
}

riemann_calendar_direct <- function(markets) {
  require_riemann_package()
  serials <- first_serial:last_serial
  dates <- serial_to_date(serials)
  # numbers with Sunday = 1, as in calendar_markets.csv
  weekday <- as.integer(format(dates, "%w")) + 1L
  markets |>
    filter(region != "common") |>
    pmap(\(key, weekend, ...) {
      weekend_days <- as.integer(strsplit(weekend, " ", fixed = TRUE)[[1]])
      business <- suppressWarnings(RiemannFinMath::is_business_day(key, dates, on_error = "na"))
      on_weekend <- weekday %in% weekend_days
      kind <- case_when(is.na(business) ~ "E", !business & !on_weekend ~ "H", business & on_weekend ~ "W", .default = NA_character_)
      tibble(key = key, serial = serials[!is.na(kind)], kind = kind[!is.na(kind)])
    }) |>
    list_rbind()
}

riemann_calendar_values <- function(markets, root) {
  input_path <- tempfile(fileext = ".csv")
  output_path <- tempfile(fileext = ".csv")
  on.exit(unlink(c(input_path, output_path)), add = TRUE)
  write_csv(select(markets, key), input_path)
  run_riemann_example("verify_calendars", input_path, output_path, root)
  read_csv(output_path, col_types = cols(key = col_character(), serial = col_integer(), kind = col_character()))
}

# The R package has no constructor for these two markets: its enums lack them.
r_package_constructor <- function(class, market) {
  if ((class == "Germany" && market == "Euwax") || (class == "Indonesia" && market == "IDX")) return(NULL)
  build <- get(class, envir = asNamespace("QuantLib"))
  if (market == "") build() else build(market)
}

ql_serials <- function(vector) {
  n <- QuantLib::DateVector___len__(vector)
  if (n == 0) return(integer())
  map_int(seq_len(n) - 1L, \(i) as.integer(QuantLib::Date_serialNumber(QuantLib::DateVector___getitem__(vector, i))))
}

# Holidays from the R package on the dates where it does not fail, without the last valid date:
# holidayList steps one day past its end. Weekend business days cannot be read from this list.
r_package_holidays <- function(market_row, cpp_errors) {
  if (market_row$region == "common") return(NULL)
  calendar <- r_package_constructor(market_row$class, market_row$market)
  if (is.null(calendar)) return(NULL)
  failing <- cpp_errors |> filter(key == market_row$key) |> pull(serial)
  early <- failing[failing < 60000L]
  from <- if (length(early)) max(early) + 1L else first_serial
  to <- last_serial - 1L
  holidays <- ql_serials(QuantLib::Calendar_holidayList(calendar, ql_date(from), ql_date(to), FALSE))
  list(from = from, to = to, holidays = holidays)
}

compare_with_r_package <- function(markets, cpp) {
  cpp_errors <- filter(cpp, kind == "E")
  map(seq_len(nrow(markets)), \(i) {
    row <- markets[i, ]
    values <- r_package_holidays(row, cpp_errors)
    if (is.null(values)) {
      return(tibble(key = row$key, r_status = if (row$region == "common") "not compared" else "no constructor in R",
                    r_from = NA_integer_, r_holidays = NA_integer_, r_diff = NA_integer_))
    }
    expected <- cpp |> filter(key == row$key, kind == "H", serial >= values$from, serial <= values$to) |> pull(serial)
    tibble(key = row$key, r_status = "compared", r_from = values$from, r_holidays = length(values$holidays),
           r_diff = length(setdiff(values$holidays, expected)) + length(setdiff(expected, values$holidays)))
  }) |> list_rbind()
}

compare_calendars <- function(markets, cpp, riemann) {
  counts <- function(values, prefix) {
    values |>
      count(key, kind) |>
      pivot_wider(names_from = kind, values_from = n, values_fill = 0L, names_prefix = prefix)
  }
  differences <- bind_rows(
    anti_join(cpp, riemann, by = c("key", "serial", "kind")),
    anti_join(riemann, cpp, by = c("key", "serial", "kind"))
  ) |>
    count(key, name = "differences")
  markets |>
    left_join(counts(cpp, "cpp_"), by = "key") |>
    left_join(counts(riemann, "riemann_"), by = "key") |>
    left_join(differences, by = "key") |>
    mutate(
      across(c(starts_with("cpp_"), starts_with("riemann_"), differences), \(x) replace_na(x, 0L)),
      dates = last_serial - first_serial + 1L,
      result = if_else(differences == 0L, "PASS", "FAIL")
    )
}

# The Riemann value of a market, as written in Rust.
riemann_value <- function(class, market, markets_in_class) {
  rust_case <- function(x) if_else(x == toupper(x), paste0(substr(x, 1, 1), tolower(substr(x, 2, nchar(x)))), x)
  enum <- case_when(class == "UnitedStates" ~ "UsMarket", class == "UnitedKingdom" ~ "UkMarket", .default = paste0(class, "Market"))
  variant <- if_else(class == "TARGET", "Target", class)
  if_else(markets_in_class > 1L,
          paste0("Calendar::", variant, "(", enum, "::", rust_case(market), ")"),
          paste0("Calendar::", variant))
}

# One row per country or market, for the report and the specification.
calendar_status <- function(by_market) {
  kinds <- c(rules = "規則のみ", rules_and_one_offs = "規則と一度きりの休日", year_tables = "年別データ")
  regions <- c(americas = "南北アメリカ", europe = "欧州", middle_east_africa = "中東・アフリカ", asia_pacific = "アジア太平洋")
  by_market |>
    filter(region != "common") |>
    add_count(class, name = "markets_in_class") |>
    transmute(
      region = factor(regions[region], regions), class, market = if_else(market == "", "（区分なし）", market),
      riemann = riemann_value(class, if_else(market == "", "", market), markets_in_class),
      kind = kinds[data_kind],
      years = if_else(is.na(first_year), "—", if_else(first_year == last_year, as.character(first_year), paste0(first_year, "〜", last_year))),
      note = replace_na(note, ""),
      holidays = cpp_H, weekend_business_days = cpp_W, error_days = cpp_E, differences,
      r_package = case_when(r_status == "compared" ~ paste0("差 ", r_diff), .default = "R で作れない"),
      result
    ) |>
    arrange(region, class)
}

# Holidays per year, to show where year tables end.
holidays_per_year <- function(cpp) {
  cpp |>
    filter(kind == "H") |>
    mutate(year = as.integer(format(serial_to_date(serial), "%Y"))) |>
    count(key, year, name = "holidays")
}

verify_calendars <- function(root = find_riemann_root()) {
  markets <- calendar_markets(root)
  message("Reading QuantLib C++ v1.42 values for ", nrow(markets), " calendars ...")
  cpp <- quantlib_cpp_calendar_values(markets, root)
  message("Computing the same calendars with Riemann on ", last_serial - first_serial + 1L, " dates each ...")
  riemann_csv <- riemann_calendar_values(markets, root)
  direct <- riemann_calendar_direct(markets)
  regional <- riemann_csv |> semi_join(filter(markets, region != "common"), by = "key")
  direct_vs_csv <- tibble(
    calendars = n_distinct(markets$key[markets$region != "common"]),
    dates = (last_serial - first_serial + 1L) * calendars,
    differences = nrow(anti_join(direct, regional, by = c("key", "serial", "kind"))) + nrow(anti_join(regional, direct, by = c("key", "serial", "kind")))
  )
  # the direct values for the markets, the example program for the joint and bespoke calendars
  riemann <- bind_rows(direct, riemann_csv |> anti_join(direct, by = "key"))
  message("Cross-checking with QuantLib R package ", as.character(packageVersion("QuantLib")), " (exact C++ commit unknown) ...")
  r_package <- compare_with_r_package(markets, cpp)
  by_market <- compare_calendars(markets, cpp, riemann) |>
    left_join(r_package, by = "key") |>
    left_join(calendar_coverage(root), by = c("class"))
  message(sprintf("Calendars %d, differences %d, failed %d; R package compared %d, differences %d",
                  nrow(by_market), sum(by_market$differences), sum(by_market$result == "FAIL"),
                  sum(by_market$r_status == "compared"), sum(by_market$r_diff, na.rm = TRUE)))
  list(by_market = by_market, per_year = holidays_per_year(cpp), direct_vs_csv = direct_vs_csv)
}
