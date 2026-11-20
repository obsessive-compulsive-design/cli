// expect: E138 type-mismatch @8:22
// expect: E138 type-mismatch @10:22
// expect: E138 type-mismatch @12:22
// expect: E138 type-mismatch @14:22
// expect: E138 type-mismatch @16:22
// Generated from the table of SPEC.md §4.1, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test operators
export const use1 = !1.5

export const use2 = !1

export const use3 = !2n

export const use4 = !"a"

export const use5 = !undefined
