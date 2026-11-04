# QuantLib official examples in RiemannFinMath

Each script reproduces parts of an official QuantLib example (QuantLib v1.42, `Examples/`) with
RiemannFinMath's Rust functions, called from R. The C++ examples print their values only when they run,
so each script compares its values with the same engines of the local QuantLib R package, when it is
installed. The parts that need a feature RiemannFinMath lacks are listed at the top of each script. They
are not replaced by other calculations. The full inventory is `docs/quantlib_guide_inventory.md` in the
repository.

Run one from an installed package:

```r
source(system.file("examples", "quantlib_examples", "equity_option.R", package = "RiemannFinMath"))
```

| Script | Example | State | Result |
|---|---|---|---|
| `equity_option.R` | EquityOption | Partly reproduced: no Bermudan exercise, other models, QD+, finite differences, other trees or QuantLib's Monte Carlo | The Black-Scholes value, the Barone-Adesi-Whaley and Bjerksund-Stensland approximations and the 801-step Cox-Ross-Rubinstein trees equal QuantLib 1.42 within 7.8e-12 |
