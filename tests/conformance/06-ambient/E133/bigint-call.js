// expect: E133 removed-global-or-member @3:20
// `BigInt` is kept for `asIntN` and `asUintN`: a bigint is written as a literal.
export const big = BigInt(10)
