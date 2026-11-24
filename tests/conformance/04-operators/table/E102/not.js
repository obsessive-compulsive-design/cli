// expect: E102 ambiguous-truthiness @8:21
// expect: E102 ambiguous-truthiness @10:21
// expect: E102 ambiguous-truthiness @12:21
// expect: E102 ambiguous-truthiness @14:21
// expect: E102 ambiguous-truthiness @16:21
// Generated from the table of SPEC.md §4.1, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test operators
export const use1 = !1.5

export const use2 = !1

export const use3 = !2n

export const use4 = !"a"

export const use5 = !undefined
