// expect: E138 type-mismatch @34:25
// expect: E138 type-mismatch @36:25
// expect: E138 type-mismatch @38:25
// expect: E138 type-mismatch @40:25
// expect: E138 type-mismatch @42:23
// expect: E138 type-mismatch @44:23
// expect: E138 type-mismatch @46:23
// expect: E138 type-mismatch @48:23
// expect: E138 type-mismatch @50:24
// expect: E138 type-mismatch @52:25
// expect: E138 type-mismatch @54:25
// expect: E138 type-mismatch @56:25
// expect: E138 type-mismatch @58:25
// expect: E138 type-mismatch @60:26
// expect: E138 type-mismatch @62:26
// expect: E138 type-mismatch @64:26
// expect: E138 type-mismatch @66:26
// expect: E138 type-mismatch @68:26
// expect: E138 type-mismatch @70:26
// expect: E138 type-mismatch @72:27
// expect: E138 type-mismatch @74:27
// expect: E138 type-mismatch @76:27
// expect: E138 type-mismatch @78:27
// expect: E138 type-mismatch @80:27
// expect: E138 type-mismatch @82:27
// expect: E138 type-mismatch @84:32
// expect: E138 type-mismatch @86:32
// expect: E138 type-mismatch @88:32
// expect: E138 type-mismatch @90:32
// expect: E138 type-mismatch @92:32
// expect: E138 type-mismatch @94:32
// Generated from the table of SPEC.md §4.1, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test operators
export const use1 = 1.5 | 2n

export const use2 = 1.5 | "a"

export const use3 = 1.5 | true

export const use4 = 1.5 | undefined

export const use5 = 1 | 2n

export const use6 = 1 | "a"

export const use7 = 1 | true

export const use8 = 1 | undefined

export const use9 = 2n | 1.5

export const use10 = 2n | 1

export const use11 = 2n | "a"

export const use12 = 2n | true

export const use13 = 2n | undefined

export const use14 = "a" | 1.5

export const use15 = "a" | 1

export const use16 = "a" | 2n

export const use17 = "a" | "a"

export const use18 = "a" | true

export const use19 = "a" | undefined

export const use20 = true | 1.5

export const use21 = true | 1

export const use22 = true | 2n

export const use23 = true | "a"

export const use24 = true | true

export const use25 = true | undefined

export const use26 = undefined | 1.5

export const use27 = undefined | 1

export const use28 = undefined | 2n

export const use29 = undefined | "a"

export const use30 = undefined | true

export const use31 = undefined | undefined
