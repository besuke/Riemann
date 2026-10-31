//! Japanese bond conventions: remaining years, simple yield, compound yield on a geometric
//! series of coupons, and risk measures with yields in percent.
//!
//! These are the VBA-compatible definitions carried over from GaussQuant
//! (`R/bond_yen_GQL.R`, commit 4ff081c "Add VBA-compatible yen bond analytics"; GaussQuant is
//! by the same author as Riemann). The same rules, arithmetic order and truncation are kept
//! so that results match GaussQuant; see `docs/spec_bonds.md` chapter 5.
//!
//! Every formula takes the remaining years `t`; [`remaining_years`] computes it from dates.
//! Rates, yields and coupons are in percent (6.515 means 6.515%); prices are per 100 face.

use crate::time::Date;
use crate::Error;

/// R's `^` for doubles: 1 when the base is 1 or the exponent 0, `x * x` for the exponent 2,
/// otherwise `pow`.
fn r_pow(x: f64, y: f64) -> f64 {
    if x == 1.0 || y == 0.0 {
        1.0
    } else if y == 2.0 {
        x * x
    } else {
        x.powf(y)
    }
}

/// Truncates toward zero to `digits` decimal places (GaussQuant `truncate_decimal_YEN_GQH`).
pub fn truncate_decimal(value: f64, digits: u32) -> f64 {
    let scale = r_pow(10.0, digits as f64);
    (value * scale).trunc() / scale
}

fn truncate_opt(value: f64, digits: Option<u32>) -> f64 {
    digits.map_or(value, |d| truncate_decimal(value, d))
}

fn invalid(reason: &str) -> Error {
    Error::InvalidInput {
        reason: reason.into(),
    }
}

/// Remaining years from `start` to `end`, truncated to 7 decimals (GaussQuant
/// `bond_year_fraction_YEN_GQL`).
///
/// If the calendar years differ by more than one, whole years are counted from the calendar
/// years and the rest is the days between the two month-days placed in the non-leap years
/// 1990 and 1991, over 365 (February 29 is taken as February 28). Otherwise the actual days
/// over 365, so a February 29 in the period counts as a day.
pub fn remaining_years(start: Date, end: Date) -> Result<f64, Error> {
    if end < start {
        return Err(invalid(
            "`start_date` and `end_date` must be scalar dates with start_date <= end_date.",
        ));
    }
    let year_difference = end.year() - start.year();
    let result = if year_difference > 1 {
        let reference = |d: Date, year: i32| {
            let (_, m, day) = d.ymd();
            let day = if m == 2 && day == 29 { 28 } else { day };
            Date::from_ymd(year, m, day)
        };
        let days = reference(start, 1990)?.days_until(reference(end, 1991)?) as f64;
        days / 365.0 + year_difference as f64 - 1.0
    } else {
        start.days_until(end) as f64 / 365.0
    };
    Ok(truncate_decimal(result, 7))
}

/// Price from a simple yield (GaussQuant `bond_price_from_simple_rate_YEN_GQL`):
/// `(R + C t) / (1 + Y t / 100)`, truncated to `digits` decimals (GaussQuant default 3).
pub fn price_from_simple_rate(
    t: f64,
    coupon_rate: f64,
    simple_rate: f64,
    redemption: f64,
    digits: Option<u32>,
) -> Result<f64, Error> {
    let denominator = 1.0 + simple_rate * t / 100.0;
    if !denominator.is_finite() || denominator <= 0.0 {
        return Err(invalid(
            "The simple rate produces a non-positive discount denominator.",
        ));
    }
    Ok(truncate_opt(
        (redemption + coupon_rate * t) / denominator,
        digits,
    ))
}

/// GaussQuant `newton_rate_YEN_GQH`: Newton steps from `initial_rate`; stops at the first
/// step whose price difference is below `10^-(digits + 1)` and returns the updated rate.
fn newton_rate(
    price: impl Fn(f64) -> Result<f64, Error>,
    derivative: impl Fn(f64) -> Result<f64, Error>,
    target_price: f64,
    initial_rate: f64,
    digits: u32,
    max_iterations: usize,
) -> Result<f64, Error> {
    let epsilon = r_pow(10.0, -((digits + 1) as f64));
    let mut rate = initial_rate;
    for _ in 0..max_iterations {
        let difference = price(rate)? - target_price;
        let d = derivative(rate)?;
        if !d.is_finite() || d.abs() <= f64::EPSILON {
            return Err(Error::NoConvergence {
                reason: "Newton's method encountered an invalid derivative.".into(),
            });
        }
        let next = rate - difference / d;
        if next.is_finite() && difference.abs() < epsilon {
            return Ok(next);
        }
        rate = next;
    }
    Err(Error::NoConvergence {
        reason: "Newton's method did not converge.".into(),
    })
}

/// Simple yield from a price (GaussQuant `bond_simple_rate_from_price_YEN_GQL`), solved by
/// Newton's method and truncated to `digits` decimals (GaussQuant defaults: 7 digits, 100
/// iterations).
pub fn simple_rate_from_price(
    t: f64,
    coupon_rate: f64,
    price: f64,
    redemption: f64,
    digits: u32,
    max_iterations: usize,
) -> Result<f64, Error> {
    if !price.is_finite() || price <= 0.0 || t <= 0.0 {
        return Err(invalid(
            "`price` and the time to maturity must be positive.",
        ));
    }
    let numerator = redemption + coupon_rate * t;
    let initial_rate = 100.0 / t * (numerator / price - 1.0);
    let rate = newton_rate(
        |r| Ok(numerator / (1.0 + r * t / 100.0)),
        |r| Ok(-numerator * (t / 100.0) / r_pow(1.0 + r * t / 100.0, 2.0)),
        price,
        initial_rate,
        digits,
        max_iterations,
    )?;
    Ok(truncate_decimal(rate, digits))
}

/// Coupon and yield-compounding frequencies per year of the Japanese compound yield.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompoundFrequencies {
    pub coupon: u32,
    pub compound: u32,
}

impl Default for CompoundFrequencies {
    /// Semiannual coupons and semiannual compounding (GaussQuant defaults).
    fn default() -> Self {
        CompoundFrequencies {
            coupon: 2,
            compound: 2,
        }
    }
}

impl CompoundFrequencies {
    fn validate(self) -> Result<(f64, f64), Error> {
        if self.coupon == 0 || self.compound == 0 {
            return Err(invalid("Coupon and compound frequencies must be positive."));
        }
        Ok((self.coupon as f64, self.compound as f64))
    }
}

/// Price and its first and second derivatives with respect to the yield in percent.
#[derive(Debug, Clone, Copy, PartialEq)]
struct CompoundValues {
    price: f64,
    dp_dy_pct: f64,
    d2p_dy2_pct: f64,
}

/// GaussQuant `compound_values_YEN_GQH`: coupons `C / f` at `f t` periods discounted with
/// `alpha = (1 + Y / (100 m))^(-m / f)`, summed as a geometric series.
fn compound_values(
    t: f64,
    coupon_rate: f64,
    compound_yield: f64,
    redemption: f64,
    coupon_frequency: f64,
    compound_frequency: f64,
) -> Result<CompoundValues, Error> {
    let base = 1.0 + compound_yield / (100.0 * compound_frequency);
    if !base.is_finite() || base <= 0.0 {
        return Err(invalid("The compound yield is outside its valid domain."));
    }
    let periods = coupon_frequency * t;
    let ratio = compound_frequency / coupon_frequency;
    let alpha = r_pow(base, -ratio);
    let coupon = coupon_rate / coupon_frequency;

    let (price, p_alpha, p_alpha2) = if compound_yield == 0.0 {
        let price = redemption + coupon_rate * t;
        if coupon_frequency == 2.0 && compound_frequency == 2.0 {
            let d1 = -(coupon_rate * t * (2.0 * t + 1.0) / 400.0 + redemption * t / 100.0);
            let d2 = t * (2.0 * t + 1.0) / 20000.0 * (coupon_rate * (t + 1.0) / 3.0 + redemption);
            return Ok(CompoundValues {
                price,
                dp_dy_pct: d1,
                d2p_dy2_pct: d2,
            });
        }
        let p_alpha = coupon * periods * (periods + 1.0) / 2.0 + redemption * periods;
        let p_alpha2 = coupon * periods * (periods + 1.0) * (periods - 1.0) / 3.0
            + redemption * periods * (periods - 1.0);
        (price, p_alpha, p_alpha2)
    } else {
        let numerator = alpha - r_pow(alpha, periods + 1.0);
        let denominator = 1.0 - alpha;
        let numerator_d1 = 1.0 - (periods + 1.0) * r_pow(alpha, periods);
        let numerator_d2 = -periods * (periods + 1.0) * r_pow(alpha, periods - 1.0);
        let quotient = numerator_d1 * denominator + numerator;
        let series = numerator / denominator;
        let series_d1 = quotient / r_pow(denominator, 2.0);
        let series_d2 = numerator_d2 / denominator + 2.0 * quotient / r_pow(denominator, 3.0);
        let price = coupon * series + redemption * r_pow(alpha, periods);
        let p_alpha = coupon * series_d1 + redemption * periods * r_pow(alpha, periods - 1.0);
        let p_alpha2 = coupon * series_d2
            + redemption * periods * (periods - 1.0) * r_pow(alpha, periods - 2.0);
        (price, p_alpha, p_alpha2)
    };

    let alpha_d1 = -alpha / (100.0 * coupon_frequency * base);
    let alpha_d2 = alpha * ratio * (ratio + 1.0)
        / (10000.0 * r_pow(compound_frequency, 2.0) * r_pow(base, 2.0));
    Ok(CompoundValues {
        price,
        dp_dy_pct: p_alpha * alpha_d1,
        d2p_dy2_pct: p_alpha2 * r_pow(alpha_d1, 2.0) + p_alpha * alpha_d2,
    })
}

/// Price from a compound yield (GaussQuant `bond_price_from_compound_yield_YEN_GQL`),
/// truncated to `digits` decimals (GaussQuant default 3).
pub fn price_from_compound_yield(
    t: f64,
    coupon_rate: f64,
    compound_yield: f64,
    redemption: f64,
    frequencies: CompoundFrequencies,
    digits: Option<u32>,
) -> Result<f64, Error> {
    let (cf, mf) = frequencies.validate()?;
    let base = 1.0 + compound_yield / (100.0 * mf);
    if !base.is_finite() || base <= 0.0 {
        return Err(invalid(
            "The compound yield must be greater than -100 * compound_frequency.",
        ));
    }
    let values = compound_values(t, coupon_rate, compound_yield, redemption, cf, mf)?;
    Ok(truncate_opt(values.price, digits))
}

/// Compound yield from a price (GaussQuant `bond_compound_yield_from_price_YEN_GQL`), solved
/// by Newton's method from the simple yield and truncated to `digits` decimals (GaussQuant
/// defaults: 7 digits, 100 iterations).
pub fn compound_yield_from_price(
    t: f64,
    coupon_rate: f64,
    price: f64,
    redemption: f64,
    frequencies: CompoundFrequencies,
    digits: u32,
    max_iterations: usize,
) -> Result<f64, Error> {
    if !price.is_finite() || price <= 0.0 {
        return Err(invalid("`price` must be positive."));
    }
    let (cf, mf) = frequencies.validate()?;
    let initial_rate = 100.0 / t * ((redemption + coupon_rate * t) / price - 1.0);
    let values = |r: f64| compound_values(t, coupon_rate, r, redemption, cf, mf);
    let rate = newton_rate(
        |r| Ok(values(r)?.price),
        |r| Ok(values(r)?.dp_dy_pct),
        price,
        initial_rate,
        digits,
        max_iterations,
    )?;
    Ok(truncate_decimal(rate, digits))
}

/// Risk measures with the yield in percent (GaussQuant `risk_tibble_YEN_GQH`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct YenBondRisk {
    /// Price from the yield, not truncated.
    pub price: f64,
    pub accrued_interest: f64,
    /// `price + accrued_interest`
    pub dirty_price: f64,
    /// dP/dY with Y in percent.
    pub dp_dy_pct: f64,
    /// d²P/dY² with Y in percent.
    pub d2p_dy2_pct: f64,
    /// `-dP/dY / 100`: price change per 100 face for 1bp, in yen.
    pub bpv_yen: f64,
    /// `-dP/dY`: the same in sen.
    pub bpv_sen: f64,
    /// `-100 dP/dY / dirty price`
    pub modified_duration: f64,
    /// `10000 d²P/dY² / dirty price`
    pub convexity: f64,
}

fn risk(
    price: f64,
    accrued_interest: f64,
    dp_dy_pct: f64,
    d2p_dy2_pct: f64,
) -> Result<YenBondRisk, Error> {
    let dirty_price = price + accrued_interest;
    if !dirty_price.is_finite() || dirty_price <= 0.0 {
        return Err(invalid("Price plus accrued interest must be positive."));
    }
    Ok(YenBondRisk {
        price,
        accrued_interest,
        dirty_price,
        dp_dy_pct,
        d2p_dy2_pct,
        bpv_yen: -dp_dy_pct / 100.0,
        bpv_sen: -dp_dy_pct,
        modified_duration: -100.0 * dp_dy_pct / dirty_price,
        convexity: 10000.0 * d2p_dy2_pct / dirty_price,
    })
}

/// Risk measures at a simple yield (GaussQuant `bond_risk_from_simple_rate_YEN_GQL`).
pub fn risk_from_simple_rate(
    t: f64,
    coupon_rate: f64,
    simple_rate: f64,
    redemption: f64,
    accrued_interest: f64,
) -> Result<YenBondRisk, Error> {
    let denominator = 1.0 + simple_rate * t / 100.0;
    let numerator = redemption + coupon_rate * t;
    let price = numerator / denominator;
    let d1 = -numerator * (t / 100.0) / r_pow(denominator, 2.0);
    let d2 = 2.0 * numerator * r_pow(t / 100.0, 2.0) / r_pow(denominator, 3.0);
    risk(price, accrued_interest, d1, d2)
}

/// Risk measures at a compound yield (GaussQuant `bond_risk_from_compound_yield_YEN_GQL`).
pub fn risk_from_compound_yield(
    t: f64,
    coupon_rate: f64,
    compound_yield: f64,
    redemption: f64,
    frequencies: CompoundFrequencies,
    accrued_interest: f64,
) -> Result<YenBondRisk, Error> {
    let (cf, mf) = frequencies.validate()?;
    let values = compound_values(t, coupon_rate, compound_yield, redemption, cf, mf)?;
    risk(
        values.price,
        accrued_interest,
        values.dp_dy_pct,
        values.d2p_dy2_pct,
    )
}
