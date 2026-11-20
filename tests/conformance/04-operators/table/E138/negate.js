// expect: E138 type-mismatch @6:22
// expect: E138 type-mismatch @8:22
// expect: E138 type-mismatch @10:22
// Generated from the table of SPEC.md §4.1, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test operators
export const use1 = -"a"

export const use2 = -true

export const use3 = -undefined
