# QuantLib Guide examples in RiemannFinMath

Each script reproduces worked examples of the QuantLib Guide (https://www.quantlibguide.com/) with
RiemannFinMath's Rust functions, called from R. It prints what it computes, and a table with the value
printed in the Guide, the same call to the local QuantLib when the QuantLib R package is installed, and
the RiemannFinMath value. Parts that need a feature RiemannFinMath lacks are listed at the top of each
script and are not replaced by other calculations. The full inventory is `docs/quantlib_guide_inventory.md`
in the repository.

Run one from an installed package:

```r
source(system.file("examples", "quantlib_guide", "bonds.R", package = "RiemannFinMath"))
```

| Script | Guide pages | Status | Main results |
|---|---|---|---|
| `bonds.R` | A taste of QuantLib; Cash flows and bonds; Cash-flow analysis; Vanilla bonds | Partly reproduced: from a yield, and on a flat curve built on each evaluation date | Cash flows, settlement dates, accrued amounts, prices, durations and convexity equal QuantLib 1.42; prices and NPVs on the curve equal the Guide, also after the evaluation date moves; the yield from a price of 98 differs from the Guide's print by 2.5e-11 and equals QuantLib 1.42, so the difference is one of QuantLib versions |
| `curves.R` | Term structures and their reference dates; The common interface of interest-rate term structures | Partly reproduced: no moving reference date, no Euribor | Flat, discount-factor and spreaded curves: discount factors and bond prices equal the Guide; forward rates agree to the Guide's 6 printed decimals |
| `options.R` | Instruments and pricing engines; Numerical Greeks | Partly reproduced: no Heston | European value and Greeks equal; the barrier Greeks are exact derivatives, against the Guide's finite differences; the Guide's theta divides one Actual/360 day by 1/365 |
| `spreads.R` | Spread calculations | Partly reproduced: the z-spread | The z-spread over a bootstrapped SOFR curve equals the Guide and QuantLib 1.42 (64.7566 bp); no SimpleThenCompounded yield, i-spread or OAS |

Values in the Guide are often printed rounded; a difference of that size is the rounding.
