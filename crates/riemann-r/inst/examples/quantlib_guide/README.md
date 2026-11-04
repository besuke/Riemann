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
| `bonds.R` | A taste of QuantLib; Cash flows and bonds; Cash-flow analysis; Vanilla bonds | Partly reproduced: no zero-coupon bond or floaters | Cash flows, settlement dates, accrued amounts, prices, durations and convexity equal QuantLib 1.42; prices, NPVs and the cash-flow NPV and BPS on a flat curve equal the Guide, also after the evaluation date moves; the step-up and amortizing bonds equal the Guide; two yields (from a price of 98, and with SimpleThenCompounded) equal QuantLib 1.42 and differ from the Guide's print by 2.5e-11 and 1.1e-6, differences of QuantLib versions; the Guide prints `nan` for the BPS of the redemption, which is 0 here |
| `cds.R` | Default-probability curves | Partly reproduced: no default density | The curves from upfront quotes (mid-point and ISDA, three evaluation dates), the check CDS, the hazard rates, the survival-probability curve and the old-conventions curve from running spreads equal QuantLib 1.42; the survival-probability curve equals the Guide's print. Against the Guide's other prints, differences of QuantLib versions: the check CDS is worth -0.2777 (one day of the premium) in the Guide and 0 here and in 1.42, and the default probabilities are up to 0.009 points above the Guide's 2-decimal values |
| `curves.R` | Term structures and their reference dates; The common interface of interest-rate term structures | Partly reproduced: no moving reference date, no Euribor | Flat, discount-factor and spreaded curves: discount factors and bond prices equal the Guide; forward rates agree to the Guide's 6 printed decimals |
| `dates.R` | Calendars and holidays; Schedules; Dangerous day-count conventions | Partly reproduced: no added, joint or bespoke calendars | All 16 printed values equal: holidays, adjustments, advances, month ends, three schedules, four year fractions |
| `duration_risk.R` | Assessing duration risk | Reproduced | A Treasury curve bootstrapped from twelve par yields with bond helpers, linear in zero rates; the clean prices of eighteen bonds and their changes for +1 bp of yield, +1 bp of zero rates, a flattening, a steepening and the curve a year later. All 108 values equal QuantLib 1.42 within 7.1e-14 and the Guide's 6-decimal prints within their rounding, except the +1 bp yield moves (up to 1.3e-5): QuantLib 1.42 solves the yield to its default accuracy of 1e-8, more loosely than the Guide's version did, a difference of QuantLib versions |
| `options.R` | Instruments and pricing engines; Numerical Greeks | Partly reproduced: no Heston | European value and Greeks equal; the barrier Greeks are exact derivatives, against the Guide's finite differences; the Guide's theta divides one Actual/360 day by 1/365 |
| `spreads.R` | Spread calculations | Partly reproduced: no OAS or equivalentRate | The z-spread over a bootstrapped SOFR curve equals the Guide and QuantLib 1.42 (64.7566 bp); the SimpleThenCompounded yield and the i-spread equal QuantLib 1.42 and differ from the Guide's print by 2.7e-7 in the yield, a difference of QuantLib versions |

Values in the Guide are often printed rounded; a difference of that size is the rounding.
