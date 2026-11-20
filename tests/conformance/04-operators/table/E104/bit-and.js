// expect: E104 bitwise-on-double @6:25
// expect: E104 bitwise-on-double @8:25
// expect: E104 bitwise-on-double @10:23
// Generated from the table of SPEC.md §4.1, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test operators
export const use1 = 1.5 & 1.5

export const use2 = 1.5 & 1

export const use3 = 1 & 1.5
