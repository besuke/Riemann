# Baseline values

`lagrange_values.csv` holds the results of the text LagrangeFinance, which are the baseline of every chapter except the calendars. The calendars use `tests/golden/calendar_all_*.csv`, from a build of QuantLib C++ v1.42 (`tools/golden/p1_all_calendars.sh`).

To make the file again, copy LagrangeFinance to a scratch directory, because some of its chapters write files, and then run from this directory's parent:

```
git -C ../../LagrangeFinance archive HEAD R data/raw/market data/demo part2_quantive_riemann part3_enterprise_riemann images _quarto.yml | tar -x -C <copy>
Rscript baseline/run_chapter.R <copy> part2_quantive_riemann/ch01_deterministic_cashflow.qmd <results>/ch01_deterministic_cashflow.rds
Rscript baseline/extract_values.R <results>
```

`run_chapter.R` runs once for each chapter that `extract_values.R` reads. The file holds calculated results only: no market quotes and no bond prices of the deliverable basket.
