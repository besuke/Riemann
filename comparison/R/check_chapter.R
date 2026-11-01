# Runs chapters without Quarto, to read their results quickly.
# Usage, from comparison/: Rscript R/check_chapter.R ch01_dates_calendars [more chapters]
for (chapter in commandArgs(TRUE)) {
  knitr::opts_knit$set(root.dir = getwd())
  knitr::opts_chunk$set(error = FALSE, warning = FALSE, message = FALSE)
  output <- file.path(tempdir(), paste0(chapter, ".md"))
  knitr::knit(file.path("chapters", paste0(chapter, ".qmd")), output = output, quiet = TRUE, envir = new.env())
  lines <- readLines(output, encoding = "UTF-8")
  writeLines(substr(lines[startsWith(lines, "|") | startsWith(lines, "Table:")], 1, 170))
}
