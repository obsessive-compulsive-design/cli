// expect: E138 type-mismatch @8:21
// expect: E138 type-mismatch @10:21
// expect: E138 type-mismatch @12:21
// expect: E138 type-mismatch @14:21
// expect: E138 type-mismatch @16:21
// Generated from the table of SPEC.md §4.1, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test operators
export const use1 = 1.5 ? 1 : 2

export const use2 = 1 ? 1 : 2

export const use3 = 2n ? 1 : 2

export const use4 = "a" ? 1 : 2

export const use5 = undefined ? 1 : 2
