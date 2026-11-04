//! `BooleanOp`: a 2-input boolean function, stored as a 4-entry truth table.
//!
//! Bit `(first << 1) | second` of the table is the result for `(first, second)`. The constant order
//! (and so the table values) match the declaration order in vanilla's `BooleanOp`.

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct BooleanOp(u8);

impl BooleanOp {
    pub const FALSE: BooleanOp = BooleanOp(0b0000);
    pub const NOT_OR: BooleanOp = BooleanOp(0b0001);
    pub const ONLY_SECOND: BooleanOp = BooleanOp(0b0010);
    pub const NOT_FIRST: BooleanOp = BooleanOp(0b0011);
    pub const ONLY_FIRST: BooleanOp = BooleanOp(0b0100);
    pub const NOT_SECOND: BooleanOp = BooleanOp(0b0101);
    pub const NOT_SAME: BooleanOp = BooleanOp(0b0110);
    pub const NOT_AND: BooleanOp = BooleanOp(0b0111);
    pub const AND: BooleanOp = BooleanOp(0b1000);
    pub const SAME: BooleanOp = BooleanOp(0b1001);
    pub const SECOND: BooleanOp = BooleanOp(0b1010);
    pub const CAUSES: BooleanOp = BooleanOp(0b1011);
    pub const FIRST: BooleanOp = BooleanOp(0b1100);
    pub const CAUSED_BY: BooleanOp = BooleanOp(0b1101);
    pub const OR: BooleanOp = BooleanOp(0b1110);
    pub const TRUE: BooleanOp = BooleanOp(0b1111);

    /// All 16 operations in vanilla declaration order.
    pub const ALL: [BooleanOp; 16] = [
        Self::FALSE,
        Self::NOT_OR,
        Self::ONLY_SECOND,
        Self::NOT_FIRST,
        Self::ONLY_FIRST,
        Self::NOT_SECOND,
        Self::NOT_SAME,
        Self::NOT_AND,
        Self::AND,
        Self::SAME,
        Self::SECOND,
        Self::CAUSES,
        Self::FIRST,
        Self::CAUSED_BY,
        Self::OR,
        Self::TRUE,
    ];

    #[inline]
    pub const fn apply(self, first: bool, second: bool) -> bool {
        (self.0 >> (((first as u8) << 1) | second as u8)) & 1 != 0
    }
}

#[cfg(test)]
mod tests {
    use super::BooleanOp as B;

    #[test]
    fn truth_tables_match_vanilla_lambdas() {
        type F = fn(bool, bool) -> bool;
        let cases: [(B, F); 16] = [
            (B::FALSE, |_, _| false),
            (B::NOT_OR, |a, b| !a && !b),
            (B::ONLY_SECOND, |a, b| b && !a),
            (B::NOT_FIRST, |a, _| !a),
            (B::ONLY_FIRST, |a, b| a && !b),
            (B::NOT_SECOND, |_, b| !b),
            (B::NOT_SAME, |a, b| a != b),
            (B::NOT_AND, |a, b| !a || !b),
            (B::AND, |a, b| a && b),
            (B::SAME, |a, b| a == b),
            (B::SECOND, |_, b| b),
            (B::CAUSES, |a, b| !a || b),
            (B::FIRST, |a, _| a),
            (B::CAUSED_BY, |a, b| a || !b),
            (B::OR, |a, b| a || b),
            (B::TRUE, |_, _| true),
        ];
        for (op, f) in cases {
            for a in [false, true] {
                for b in [false, true] {
                    assert_eq!(op.apply(a, b), f(a, b), "{op:?} {a} {b}");
                }
            }
        }
    }
}
