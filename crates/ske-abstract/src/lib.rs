//! Pure integer interval domain (v0.4). No SQL, no Z3, no solver.
//!
//! Implements the `intervals` loop abstraction described in §19 v0.4 and
//! ADR-021: closed integer intervals over machine sorts (width 1..=128,
//! signed or unsigned), Kleene iteration with widening then one narrowing
//! step, and transfer functions for `ADD`/`SUB`/`MUL` where any operation
//! that can overflow yields the full range of the type.
use std::fmt;
pub mod cir;

/// A machine integer sort: bit width and signedness.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct IntType {
    pub width: u32,
    pub signed: bool,
}

impl IntType {
    pub fn bitvec(width: u32, signed: bool) -> Result<Self, String> {
        if !(1..=128).contains(&width) {
            return Err(format!("bitvector width {width} outside 1..=128"));
        }
        Ok(Self { width, signed })
    }

    /// Minimum value of the sort as a 128-bit two's-complement value.
    pub fn min_value(&self) -> i128 {
        if self.signed && self.width == 128 {
            i128::MIN
        } else if self.signed {
            -(1i128 << (self.width - 1))
        } else {
            0
        }
    }

    /// Maximum value of the sort as a 128-bit two's-complement value.
    pub fn max_value(&self) -> i128 {
        if self.signed && self.width == 128 {
            i128::MAX
        } else if self.signed {
            (1i128 << (self.width - 1)) - 1
        } else if self.width == 128 {
            i128::MAX
        } else {
            (1i128 << self.width) - 1
        }
    }

    /// Wrap a 128-bit signed value into the sort's representation.
    pub fn wrap(&self, value: i128) -> i128 {
        let mask = u128::MAX >> (128 - self.width);
        let bits = value as u128 & mask;
        if self.signed {
            ((bits << (128 - self.width)) as i128) >> (128 - self.width)
        } else {
            bits as i128
        }
    }
}

/// The full range of a sort (top).
pub fn full_range(ty: IntType) -> Interval {
    if !ty.signed && ty.width == 128 {
        Interval::Unsigned128Top
    } else {
        Interval::Range {
            lo: ty.min_value(),
            hi: ty.max_value(),
        }
    }
}

/// A closed interval or bottom (empty).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Interval {
    Bottom,
    Range {
        lo: i128,
        hi: i128,
    },
    /// Full uint128 range. Upper-half uint128 constants are conservatively
    /// rounded to this top, never clamped to i128::MAX.
    Unsigned128Top,
}

impl Interval {
    pub fn bottom() -> Self {
        Interval::Bottom
    }

    pub fn constant(ty: IntType, value: i128) -> Self {
        if !ty.signed && ty.width == 128 && value < 0 {
            return Self::Unsigned128Top;
        }
        let value = ty.wrap(value);
        Interval::Range {
            lo: value,
            hi: value,
        }
    }

    pub fn range(ty: IntType, lo: i128, hi: i128) -> Self {
        if lo > hi {
            return Self::Bottom;
        }
        if lo < ty.min_value() || hi > ty.max_value() {
            return full_range(ty);
        }
        Interval::Range { lo, hi }
    }

    pub fn is_bottom(&self) -> bool {
        matches!(self, Interval::Bottom)
    }

    pub fn contains(&self, value: i128) -> bool {
        match self {
            Interval::Bottom => false,
            Interval::Unsigned128Top => value >= 0,
            Interval::Range { lo, hi } => *lo <= value && value <= *hi,
        }
    }

    /// Intersection; empty intersection yields Bottom.
    pub fn meet(&self, other: &Self) -> Self {
        match (self, other) {
            (Interval::Bottom, _) | (_, Interval::Bottom) => Interval::Bottom,
            (Interval::Unsigned128Top, b) | (b, Interval::Unsigned128Top) => *b,
            (Interval::Range { lo: l1, hi: h1 }, Interval::Range { lo: l2, hi: h2 }) => {
                let lo = *l1.max(l2);
                let hi = *h1.min(h2);
                if lo <= hi {
                    Interval::Range { lo, hi }
                } else {
                    Interval::Bottom
                }
            }
        }
    }

    /// Union hull (join).
    pub fn join(&self, other: &Self) -> Self {
        match (self, other) {
            (Interval::Bottom, b) => *b,
            (a, Interval::Bottom) => *a,
            (Interval::Unsigned128Top, _) | (_, Interval::Unsigned128Top) => {
                Interval::Unsigned128Top
            }
            (Interval::Range { lo: l1, hi: h1 }, Interval::Range { lo: l2, hi: h2 }) => {
                Interval::Range {
                    lo: *l1.min(l2),
                    hi: *h1.max(h2),
                }
            }
        }
    }

    /// One widening step: a bound that jumped outward jumps to the type bound.
    /// (`widen(old, new)`: `new` is the value after one more iteration.)
    pub fn widen(&self, next: &Self, ty: IntType) -> Self {
        match (self, next) {
            (Interval::Bottom, b) => *b,
            (a, Interval::Bottom) => *a,
            (Interval::Unsigned128Top, _) | (_, Interval::Unsigned128Top) => {
                Interval::Unsigned128Top
            }
            (Interval::Range { lo: l1, hi: h1 }, Interval::Range { lo: l2, hi: h2 }) => {
                let lo = if *l2 < *l1 { ty.min_value() } else { *l1 };
                let hi = if *h2 > *h1 { ty.max_value() } else { *h1 };
                Interval::Range { lo, hi }
            }
        }
    }

    /// One narrowing step from a widened bound toward a concrete bound.
    /// Keeps soundness (never smaller than the concrete bound on the downward
    /// side beyond what the meet proves).
    pub fn narrow(&self, other: &Self, ty: IntType) -> Self {
        // narrowing(old, new) where old is the widened value and new the next
        // iterate: move uncertain bounds toward the new value.
        match (self, other) {
            (Interval::Bottom, b) => *b,
            (a, Interval::Bottom) => *a,
            (Interval::Unsigned128Top, b) => *b,
            (_, Interval::Unsigned128Top) => *self,
            (Interval::Range { lo: l1, hi: h1 }, Interval::Range { lo: l2, hi: h2 }) => {
                let lo = if *l1 == ty.min_value() && *l2 > ty.min_value() {
                    *l2
                } else {
                    *l1
                };
                let hi = if *h1 == ty.max_value() && *h2 < ty.max_value() {
                    *h2
                } else {
                    *h1
                };
                Interval::Range { lo, hi }
            }
        }
    }
}

impl fmt::Display for Interval {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Interval::Bottom => write!(f, "⊥"),
            Interval::Unsigned128Top => write!(f, "[0, {}]", u128::MAX),
            Interval::Range { lo, hi } => write!(f, "[{lo}, {hi}]"),
        }
    }
}

/// Result of comparing two intervals: always/never/maybe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rel {
    DefinitelyTrue,
    DefinitelyFalse,
    Maybe,
}

fn checked_add(ty: IntType, a: i128, b: i128) -> Option<i128> {
    let sum = a.checked_add(b)?;
    if sum < ty.min_value() || sum > ty.max_value() {
        None
    } else {
        Some(sum)
    }
}

fn checked_sub(ty: IntType, a: i128, b: i128) -> Option<i128> {
    let diff = a.checked_sub(b)?;
    if diff < ty.min_value() || diff > ty.max_value() {
        None
    } else {
        Some(diff)
    }
}

fn checked_mul(ty: IntType, a: i128, b: i128) -> Option<i128> {
    let prod = a.checked_mul(b)?;
    if prod < ty.min_value() || prod > ty.max_value() {
        None
    } else {
        Some(prod)
    }
}

/// Transfer ADD: exact interval when no overflow possible across the whole
/// operand ranges; otherwise the full type range.
pub fn add(ty: IntType, a: Interval, b: Interval) -> Interval {
    match (a, b) {
        (Interval::Bottom, _) | (_, Interval::Bottom) => Interval::Bottom,
        (Interval::Unsigned128Top, _) | (_, Interval::Unsigned128Top) => full_range(ty),
        (Interval::Range { lo: l1, hi: h1 }, Interval::Range { lo: l2, hi: h2 }) => {
            match (checked_add(ty, l1, l2), checked_add(ty, h1, h2)) {
                (Some(lo), Some(hi)) => Interval::Range { lo, hi },
                _ => full_range(ty),
            }
        }
    }
}

/// Transfer SUB.
pub fn sub(ty: IntType, a: Interval, b: Interval) -> Interval {
    match (a, b) {
        (Interval::Bottom, _) | (_, Interval::Bottom) => Interval::Bottom,
        (Interval::Unsigned128Top, _) | (_, Interval::Unsigned128Top) => full_range(ty),
        (Interval::Range { lo: l1, hi: h1 }, Interval::Range { lo: l2, hi: h2 }) => {
            match (checked_sub(ty, l1, h2), checked_sub(ty, h1, l2)) {
                (Some(lo), Some(hi)) => Interval::Range { lo, hi },
                _ => full_range(ty),
            }
        }
    }
}

/// Transfer MUL.
pub fn mul(ty: IntType, a: Interval, b: Interval) -> Interval {
    match (a, b) {
        (Interval::Bottom, _) | (_, Interval::Bottom) => Interval::Bottom,
        (Interval::Unsigned128Top, _) | (_, Interval::Unsigned128Top) => full_range(ty),
        (Interval::Range { lo: l1, hi: h1 }, Interval::Range { lo: l2, hi: h2 }) => {
            let products = [
                checked_mul(ty, l1, l2),
                checked_mul(ty, l1, h2),
                checked_mul(ty, h1, l2),
                checked_mul(ty, h1, h2),
            ];
            if products.iter().any(Option::is_none) {
                return full_range(ty);
            }
            let lo = products.iter().flatten().min().copied();
            let hi = products.iter().flatten().max().copied();
            match (lo, hi) {
                (Some(lo), Some(hi)) => Interval::Range { lo, hi },
                _ => full_range(ty),
            }
        }
    }
}

/// One comparison step, returning a definite or maybe result.
/// `op` is one of `LT`, `LE`, `GT`, `GE`, `EQ`, `NE`.
pub fn compare(_ty: IntType, a: Interval, b: Interval, op: &str) -> Rel {
    use Rel::*;
    match (a, b) {
        (Interval::Bottom, _) | (_, Interval::Bottom) => Maybe,
        (Interval::Unsigned128Top, _) | (_, Interval::Unsigned128Top) => Maybe,
        (Interval::Range { lo: l1, hi: h1 }, Interval::Range { lo: l2, hi: h2 }) => {
            // Over-approximate: the interval result over the type range is
            // decisive when all combinations satisfy / violate the relation.
            let all_true = match op {
                "LT" => h1 < l2,
                "LE" => h1 <= l2,
                "GT" => l1 > h2,
                "GE" => l1 >= h2,
                "EQ" => l1 == l2 && h1 == h2 && l1 == h1,
                "NE" => h1 < l2 || h2 < l1,
                _ => return Maybe,
            };
            if all_true {
                DefinitelyTrue
            } else {
                // Check whether NO combination can be true (all false).
                let all_false = match op {
                    "LT" => l1 >= h2,
                    "LE" => l1 > h2,
                    "GT" => h1 <= l2,
                    "GE" => h1 < l2,
                    "EQ" => h1 < l2 || h2 < l1,
                    "NE" => l1 == l2 && h1 == h2 && l1 == h1,
                    _ => return Maybe,
                };
                if all_false {
                    DefinitelyFalse
                } else {
                    Maybe
                }
            }
        }
    }
}

/// Kleene iteration with widening then one narrowing step.
///
/// `step` maps an abstract value (the join of all visited values) to the next
/// iteration's contribution. `widening_threshold` is the number of ordinary
/// steps before widening kicks in. Returns a sound (over-approximating) fixed
/// point after widening and one narrowing.
pub fn iterate_loop(
    ty: IntType,
    init: Interval,
    mut step: impl FnMut(Interval) -> Interval,
) -> Interval {
    // Sequence: x0 = init
    //          x_{n+1} = x_n ⊔ step(x_n)
    //          if x_{n+1} == x_n, done.
    //          else after threshold iterations, widen and narrow.
    let mut current = init;
    for _ in 0..32 {
        let next = init.join(&step(current));
        if current.join(&next) == current {
            // One narrowing, accepted only if the resulting interval remains
            // a post-fixpoint. This check also defends non-monotone callbacks.
            let narrowed = current.narrow(&next, ty);
            let image = init.join(&step(narrowed));
            return if narrowed.join(&image) == narrowed {
                narrowed
            } else {
                current
            };
        }
        current = current.widen(&next, ty);
    }
    // Safety net: return a sound over-approximation (top).
    full_range(ty)
}

#[cfg(test)]
mod tests {
    use super::*;
    use Rel::*;

    fn int64() -> IntType {
        IntType::bitvec(64, true).unwrap()
    }

    #[test]
    fn constant_and_full_range() {
        let ty = int64();
        assert_eq!(Interval::constant(ty, 0), Interval::Range { lo: 0, hi: 0 });
        let full = full_range(ty);
        assert_eq!(full.lo(), ty.min_value());
        assert_eq!(full.hi(), ty.max_value());
    }

    #[test]
    fn add_overflow_detection() {
        let ty = int64();
        // Small ranges: exact.
        let a = Interval::Range { lo: 1, hi: 2 };
        let b = Interval::Range { lo: 3, hi: 4 };
        assert_eq!(add(ty, a, b), Interval::Range { lo: 4, hi: 6 });
        // Bounds that near i64::MAX: overflow => full range.
        let hi = Interval::Range {
            lo: i64::MAX as i128 - 1,
            hi: i64::MAX as i128,
        };
        let r = add(ty, hi, Interval::Range { lo: 1, hi: 1 });
        assert_eq!(r, full_range(ty));
    }

    #[test]
    fn sub_mul_transfer() {
        let ty = int64();
        let a = Interval::Range { lo: -5, hi: 5 };
        let b = Interval::Range { lo: 2, hi: 3 };
        assert_eq!(sub(ty, a, b), Interval::Range { lo: -8, hi: 3 });
        // MUL endpoints: minimal = -5*3, maximal = 5*3.
        assert_eq!(mul(ty, a, b), Interval::Range { lo: -15, hi: 15 });
    }

    #[test]
    fn mul_double_overflow_yields_full_range() {
        let ty = int64();
        let a = Interval::Range {
            lo: i64::MAX as i128,
            hi: i64::MAX as i128,
        };
        let b = Interval::Range { lo: 2, hi: 2 };
        assert_eq!(mul(ty, a, b), full_range(ty));
    }

    #[test]
    fn mul_one_overflowing_endpoint_also_yields_top() {
        let ty = int64();
        let a = Interval::Range {
            lo: 0,
            hi: i64::MAX as i128,
        };
        let b = Interval::Range { lo: 0, hi: 2 };
        assert_eq!(mul(ty, a, b), full_range(ty));
    }

    #[test]
    fn full_128_bit_sorts_do_not_panic_or_lose_unsigned_values() {
        let signed = IntType::bitvec(128, true).unwrap();
        assert_eq!(
            full_range(signed),
            Interval::Range {
                lo: i128::MIN,
                hi: i128::MAX
            }
        );
        assert_eq!(
            Interval::constant(signed, i128::MIN),
            Interval::Range {
                lo: i128::MIN,
                hi: i128::MIN
            }
        );
        let unsigned = IntType::bitvec(128, false).unwrap();
        assert_eq!(full_range(unsigned), Interval::Unsigned128Top);
        assert_eq!(Interval::constant(unsigned, -1), Interval::Unsigned128Top);
        assert_eq!(
            Interval::constant(IntType::bitvec(8, true).unwrap(), 255),
            Interval::Range { lo: -1, hi: -1 }
        );
    }

    #[test]
    fn widening_stabilizes() {
        let ty = int64();
        // A loop variable i that repeatedly grows by 1 from 0: i := i + 1
        let step = |x: Interval| add(ty, x, Interval::constant(ty, 1));
        let r = iterate_loop(ty, Interval::constant(ty, 0), step);
        // With widening the unstable upper bound jumps to top.
        assert_eq!(r.hi(), ty.max_value());
        // The next ADD can overflow: the normative transfer then returns
        // the full type range, including its negative half.
        assert_eq!(r.lo(), ty.min_value());
    }

    #[test]
    fn narrowing_one_step_stays_sound() {
        let ty = IntType::bitvec(64, true).unwrap();
        // A loop bounded above by n=10: i := i+1 while i < 10
        let bound = 10i128;
        let step = |x: Interval| {
            // i' = i+1, clipped by the exit condition i < bound.
            let next = add(ty, x, Interval::constant(ty, 1));
            match next {
                Interval::Range { hi, .. } if hi >= bound => Interval::Range { lo: 0, hi: bound },
                other => other,
            }
        };
        let r = iterate_loop(ty, Interval::constant(ty, 0), step);
        // After widening the upper bound may reach top; the single narrowing
        // recovers a bound that still contains every concrete value below 10.
        assert!(r.lo() <= 0);
        assert!(r.hi() >= 0);
        // Soundness: every concrete i in [0, 9] is contained.
        for i in 0..10 {
            assert!(r.contains(i), "interval {r} must contain {i}");
        }
    }

    #[test]
    fn meet_join_contains() {
        let a = Interval::Range { lo: 1, hi: 5 };
        let b = Interval::Range { lo: 3, hi: 8 };
        assert_eq!(a.meet(&b), Interval::Range { lo: 3, hi: 5 });
        assert_eq!(a.join(&b), Interval::Range { lo: 1, hi: 8 });
        assert!(a.contains(3));
        assert!(!a.contains(9));
    }

    #[test]
    fn comparisons() {
        let ty = int64();
        let a = Interval::Range { lo: 1, hi: 2 };
        let b = Interval::Range { lo: 3, hi: 4 };
        assert_eq!(compare(ty, a, b, "LT"), DefinitelyTrue);
        assert_eq!(compare(ty, b, a, "LT"), DefinitelyFalse);
        let overlap = Interval::Range { lo: 2, hi: 3 };
        assert_eq!(compare(ty, a, overlap, "LT"), Maybe);
    }

    #[test]
    fn intervals_contain_concrete() {
        // AC-0.4-09: for Sum's loop variables i and s, every observed concrete
        // value during real executions of Sum is contained in the interval.
        // Sum(n): s=0; for i in 0..n { s += i }; return s.
        let mut rng = randish::Lcg::new(0xC0FFEE);
        let ty = int64();
        for _ in 0..200 {
            let n = rng.gen_range(-10, 31);
            let (i_iv, s_iv) = sum_intervals(ty, n);
            // Concrete run, collecting observed i and s.
            let (mut i, mut s) = (0i128, 0i128);
            let mut observed_i = Vec::new();
            let mut observed_s = Vec::new();
            while i < n {
                observed_i.push(i);
                observed_s.push(s);
                s += i;
                i += 1;
            }
            observed_s.push(s);
            for v in &observed_i {
                assert!(i_iv.contains(*v), "n={n}: i={v} not in {i_iv} (range miss)");
            }
            for v in &observed_s {
                assert!(s_iv.contains(*v), "n={n}: s={v} not in {s_iv} (range miss)");
            }
        }
    }

    /// Abstract interpretation of Sum's loop for a concrete bound `n`.
    fn sum_intervals(ty: IntType, n: i128) -> (Interval, Interval) {
        if n <= 0 {
            // Loop body never runs: i stays 0, s stays 0.
            return (Interval::constant(ty, 0), Interval::constant(ty, 0));
        }
        let zero = Interval::constant(ty, 0);
        let one = Interval::constant(ty, 1);
        let body_guard = Interval::range(ty, 0, n - 1);
        let mut state = (zero, zero);
        let transfer = |(i, s): (Interval, Interval)| {
            let active_i = i.meet(&body_guard);
            (
                zero.join(&add(ty, active_i, one)),
                zero.join(&add(ty, s, active_i)),
            )
        };
        for _ in 0..32 {
            let next = transfer(state);
            if state.0.join(&next.0) == state.0 && state.1.join(&next.1) == state.1 {
                let narrowed = (state.0.narrow(&next.0, ty), state.1.narrow(&next.1, ty));
                let image = transfer(narrowed);
                if narrowed.0.join(&image.0) == narrowed.0
                    && narrowed.1.join(&image.1) == narrowed.1
                {
                    return narrowed;
                }
                return state;
            }
            state = (state.0.widen(&next.0, ty), state.1.widen(&next.1, ty));
        }
        panic!("Sum widening did not converge");
    }
}

// Extension helpers used by tests only.
#[cfg(test)]
impl Interval {
    fn lo(&self) -> i128 {
        match self {
            Interval::Bottom => i128::MIN,
            Interval::Unsigned128Top => 0,
            Interval::Range { lo, .. } => *lo,
        }
    }
    fn hi(&self) -> i128 {
        match self {
            Interval::Bottom => i128::MAX,
            Interval::Unsigned128Top => i128::MAX,
            Interval::Range { hi, .. } => *hi,
        }
    }
}

// Deterministic PRNG for tests (no external dependency).
#[cfg(test)]
mod randish {
    pub struct Lcg(u64);
    impl Lcg {
        pub fn new(seed: u64) -> Self {
            Self(seed)
        }
        fn next(&mut self) -> u64 {
            // Numerical Recipes LCG; deterministic across runs.
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            self.0 >> 33
        }
        pub fn gen_range(&mut self, lo: i128, hi: i128) -> i128 {
            // hi exclusive. Simple modulo for positive range.
            let span = (hi - lo) as u64;
            lo + (self.next() % span) as i128
        }
    }
}
