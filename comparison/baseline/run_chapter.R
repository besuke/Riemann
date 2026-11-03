# Evaluates one LagrangeFinance chapter, expression by expression, in a scratch copy, and saves plain results.
a <- commandArgs(TRUE); root <- a[1]; qmd <- a[2]; out <- a[3]
setwd(root); options(warn=-1); suppressPackageStartupMessages({library(tidyverse)})
tmp <- tempfile(fileext=".R"); invisible(knitr::purl(qmd, output=tmp, quiet=TRUE, documentation=0))
ex <- parse(tmp, encoding="UTF-8"); env <- new.env(parent=globalenv()); errs <- character(); summaries <- list()
sink(file.path(dirname(out), paste0(basename(out), ".log")))
pdf(NULL)
for (e in ex) { s <- paste(deparse(e), collapse=" ")
  if (grepl("anim_|gganimate|ggsave|ggplot[(]|show_plot|jgb_yield_url|plan[(]|future_", s)) next
  r <- tryCatch({ v <- eval(e, env)
    # the chapters show these summaries and keep them nowhere, so they are kept here
    if (grepl("ir_show_swap_summary_GQH", s)) summaries[[as.character(e$title)]] <- v
    NULL }, error=function(x) conditionMessage(x))
  if (!is.null(r)) errs <- c(errs, paste0(substr(s,1,120)," :: ",r)) }
sink()
assign("swap_summaries", summaries, envir=env)
clean <- function(x, d=0) {
  if (is.data.frame(x)) { keep <- vapply(x, function(c) is.atomic(c)||inherits(c,"Date"), TRUE); return(tibble::as_tibble(x[keep])) }
  if (is.atomic(x)||inherits(x,"Date")) return(x)
  if (is.list(x) && !is.object(x) && d<3) { y <- lapply(x, clean, d+1); return(y[!vapply(y,is.null,TRUE)]) }
  NULL }
res <- list()
for (n in ls(env, all.names=TRUE)) { v <- get(n, env); if (is.function(v)) next; c <- tryCatch(clean(v), error=function(e) NULL); if(!is.null(c)) res[[n]] <- c }
saveRDS(list(values=res, errors=errs), out); cat(qmd, "objects", length(res), "errors", length(errs), "\n"); writeLines(substr(errs,1,240))
