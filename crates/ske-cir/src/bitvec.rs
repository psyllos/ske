//! Exact 1..=128 bit machine arithmetic. Shift counts are never truncated.
/// Construct through [`BitVec::new`] so invalid widths and unmasked bits cannot
/// enter arithmetic. Accessors expose the normalized machine representation.
///
/// ```compile_fail
/// use ske_cir::bitvec::BitVec;
/// let invalid = BitVec { width: 0, bits: 256 };
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BitVec {
    width: u32,
    bits: u128,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArithmeticError {
    InvalidWidth,
    WidthMismatch,
    DivByZero,
    NegativeShift,
}
use ArithmeticError::*;
impl BitVec {
    pub fn width(self) -> u32 {
        self.width
    }
    pub fn bits(self) -> u128 {
        self.bits
    }
    pub fn new(width: u32, bits: u128) -> Result<Self, ArithmeticError> {
        if !(1..=128).contains(&width) {
            return Err(InvalidWidth);
        }
        Ok(Self {
            width,
            bits: bits & (u128::MAX >> (128 - width)),
        })
    }
    pub fn signed(self) -> i128 {
        ((self.bits << (128 - self.width)) as i128) >> (128 - self.width)
    }
    pub fn binary(self, op: &str, rhs: Self, signed: bool) -> Result<Self, ArithmeticError> {
        if self.width != rhs.width {
            return Err(WidthMismatch);
        }
        let bits = match op {
            "ADD" => self.bits.wrapping_add(rhs.bits),
            "SUB" => self.bits.wrapping_sub(rhs.bits),
            "MUL" => self.bits.wrapping_mul(rhs.bits),
            "BIT_AND" => self.bits & rhs.bits,
            "BIT_OR" => self.bits | rhs.bits,
            "XOR" => self.bits ^ rhs.bits,
            "DIV" | "REM" => {
                if rhs.bits == 0 {
                    return Err(DivByZero);
                }
                if signed {
                    let a = self.signed();
                    let b = rhs.signed();
                    if op == "DIV" {
                        a.wrapping_div(b) as u128
                    } else {
                        a.wrapping_rem(b) as u128
                    }
                } else if op == "DIV" {
                    self.bits / rhs.bits
                } else {
                    self.bits % rhs.bits
                }
            }
            _ => return Err(WidthMismatch),
        };
        Self::new(self.width, bits)
    }
    pub fn bit_not(self) -> Self {
        Self::new(self.width, !self.bits).unwrap()
    }
    pub fn lt(self, rhs: Self, signed: bool) -> Result<bool, ArithmeticError> {
        if self.width != rhs.width {
            return Err(WidthMismatch);
        }
        Ok(if signed {
            self.signed() < rhs.signed()
        } else {
            self.bits < rhs.bits
        })
    }
    pub fn shift(
        self,
        count: Self,
        count_signed: bool,
        right: bool,
        arithmetic: bool,
    ) -> Result<Self, ArithmeticError> {
        if count_signed && count.signed() < 0 {
            return Err(NegativeShift);
        }
        let bits = if count.bits >= self.width as u128 {
            if right && arithmetic && self.signed() < 0 {
                u128::MAX
            } else {
                0
            }
        } else if right && arithmetic {
            (self.signed() >> count.bits as u32) as u128
        } else if right {
            self.bits >> count.bits as u32
        } else {
            self.bits << count.bits as u32
        };
        Self::new(self.width, bits)
    }
    pub fn resize(self, width: u32, signed: bool) -> Result<Self, ArithmeticError> {
        Self::new(
            width,
            if signed {
                self.signed() as u128
            } else {
                self.bits
            },
        )
    }
}
impl std::ops::Neg for BitVec {
    type Output = Self;
    fn neg(self) -> Self::Output {
        Self::new(self.width, self.bits.wrapping_neg()).unwrap()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::ops::Neg;
    fn b(w: u32, n: i128) -> BitVec {
        BitVec::new(w, n as u128).unwrap()
    }
    #[test]
    fn bitvec_vectors() {
        for (op, a, c, s, want) in [
            ("ADD", 127, 1, true, -128),
            ("ADD", 200, 100, false, 44),
            ("SUB", 0, 1, false, 255),
            ("MUL", 64, 2, true, -128),
            ("MUL", 16, 16, false, 0),
            ("DIV", -128, -1, true, -128),
            ("REM", -128, -1, true, 0),
            ("DIV", -7, 2, true, -3),
            ("REM", -7, 2, true, -1),
            ("DIV", 7, -2, true, -3),
            ("REM", 7, -2, true, 1),
        ] {
            let r = b(8, a).binary(op, b(8, c), s).unwrap();
            assert_eq!(if s { r.signed() } else { r.bits as i128 }, want);
        }
        assert_eq!(b(8, -128).neg().signed(), -128);
        assert_eq!(b(8, 5).binary("DIV", b(8, 0), true), Err(DivByZero));
        for (a, c, right, arith, want) in [
            (1, 7, false, false, -128),
            (1, 8, false, false, 0),
            (-128, 7, true, true, -1),
            (-128, 100, true, true, -1),
            (128, 7, true, false, 1),
            (128, 8, true, false, 0),
        ] {
            assert_eq!(
                b(8, a)
                    .shift(b(8, c), false, right, arith)
                    .unwrap()
                    .signed(),
                want
            );
        }
        assert_eq!(
            b(8, 1).shift(b(8, -1), true, false, false),
            Err(NegativeShift)
        );
        assert_eq!(
            b(8, 1).shift(b(16, 256), false, false, false).unwrap().bits,
            0
        );
        assert!(b(8, -1).lt(b(8, 1), true).unwrap());
        assert!(!b(8, 255).lt(b(8, 1), false).unwrap());
        assert_eq!(b(8, -1).resize(16, true).unwrap().bits, 65535);
        assert_eq!(b(8, 255).resize(16, false).unwrap().bits, 255);
        assert_eq!(b(16, 300).resize(8, false).unwrap().bits, 44);
        assert_eq!(b(8, -1).resize(8, false).unwrap().bits, 255);
    }
}
