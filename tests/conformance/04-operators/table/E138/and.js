// expect: E138 type-mismatch @38:21
// expect: E138 type-mismatch @40:21
// expect: E138 type-mismatch @42:21
// expect: E138 type-mismatch @44:21
// expect: E138 type-mismatch @46:21
// expect: E138 type-mismatch @48:21
// expect: E138 type-mismatch @50:21
// expect: E138 type-mismatch @52:21
// expect: E138 type-mismatch @54:21
// expect: E138 type-mismatch @56:22
// expect: E138 type-mismatch @58:22
// expect: E138 type-mismatch @60:22
// expect: E138 type-mismatch @62:22
// expect: E138 type-mismatch @64:22
// expect: E138 type-mismatch @66:22
// expect: E138 type-mismatch @68:22
// expect: E138 type-mismatch @70:22
// expect: E138 type-mismatch @72:22
// expect: E138 type-mismatch @74:22
// expect: E138 type-mismatch @76:22
// expect: E138 type-mismatch @78:22
// expect: E138 type-mismatch @80:22
// expect: E138 type-mismatch @82:22
// expect: E138 type-mismatch @84:22
// expect: E138 type-mismatch @86:30
// expect: E138 type-mismatch @88:30
// expect: E138 type-mismatch @90:30
// expect: E138 type-mismatch @92:30
// expect: E138 type-mismatch @94:30
// expect: E138 type-mismatch @96:22
// expect: E138 type-mismatch @98:22
// expect: E138 type-mismatch @100:22
// expect: E138 type-mismatch @102:22
// expect: E138 type-mismatch @104:22
// expect: E138 type-mismatch @106:22
// Generated from the table of SPEC.md §4.1, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test operators
export const use1 = 1.5 && 1.5

export const use2 = 1.5 && 1

export const use3 = 1.5 && 2n

export const use4 = 1.5 && "a"

export const use5 = 1.5 && true

export const use6 = 1.5 && undefined

export const use7 = 1 && 1.5

export const use8 = 1 && 1

export const use9 = 1 && 2n

export const use10 = 1 && "a"

export const use11 = 1 && true

export const use12 = 1 && undefined

export const use13 = 2n && 1.5

export const use14 = 2n && 1

export const use15 = 2n && 2n

export const use16 = 2n && "a"

export const use17 = 2n && true

export const use18 = 2n && undefined

export const use19 = "a" && 1.5

export const use20 = "a" && 1

export const use21 = "a" && 2n

export const use22 = "a" && "a"

export const use23 = "a" && true

export const use24 = "a" && undefined

export const use25 = true && 1.5

export const use26 = true && 1

export const use27 = true && 2n

export const use28 = true && "a"

export const use29 = true && undefined

export const use30 = undefined && 1.5

export const use31 = undefined && 1

export const use32 = undefined && 2n

export const use33 = undefined && "a"

export const use34 = undefined && true

export const use35 = undefined && undefined
