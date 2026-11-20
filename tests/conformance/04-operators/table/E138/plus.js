// expect: E138 type-mismatch @33:25
// expect: E138 type-mismatch @35:25
// expect: E138 type-mismatch @37:25
// expect: E138 type-mismatch @39:25
// expect: E138 type-mismatch @41:23
// expect: E138 type-mismatch @43:23
// expect: E138 type-mismatch @45:23
// expect: E138 type-mismatch @47:23
// expect: E138 type-mismatch @49:24
// expect: E138 type-mismatch @51:25
// expect: E138 type-mismatch @53:25
// expect: E138 type-mismatch @55:25
// expect: E138 type-mismatch @57:25
// expect: E138 type-mismatch @59:26
// expect: E138 type-mismatch @61:26
// expect: E138 type-mismatch @63:26
// expect: E138 type-mismatch @65:26
// expect: E138 type-mismatch @67:26
// expect: E138 type-mismatch @69:27
// expect: E138 type-mismatch @71:27
// expect: E138 type-mismatch @73:27
// expect: E138 type-mismatch @75:27
// expect: E138 type-mismatch @77:27
// expect: E138 type-mismatch @79:27
// expect: E138 type-mismatch @81:32
// expect: E138 type-mismatch @83:32
// expect: E138 type-mismatch @85:32
// expect: E138 type-mismatch @87:32
// expect: E138 type-mismatch @89:32
// expect: E138 type-mismatch @91:32
// Generated from the table of SPEC.md §4.1, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test operators
export const use1 = 1.5 + 2n

export const use2 = 1.5 + "a"

export const use3 = 1.5 + true

export const use4 = 1.5 + undefined

export const use5 = 1 + 2n

export const use6 = 1 + "a"

export const use7 = 1 + true

export const use8 = 1 + undefined

export const use9 = 2n + 1.5

export const use10 = 2n + 1

export const use11 = 2n + "a"

export const use12 = 2n + true

export const use13 = 2n + undefined

export const use14 = "a" + 1.5

export const use15 = "a" + 1

export const use16 = "a" + 2n

export const use17 = "a" + true

export const use18 = "a" + undefined

export const use19 = true + 1.5

export const use20 = true + 1

export const use21 = true + 2n

export const use22 = true + "a"

export const use23 = true + true

export const use24 = true + undefined

export const use25 = undefined + 1.5

export const use26 = undefined + 1

export const use27 = undefined + 2n

export const use28 = undefined + "a"

export const use29 = undefined + true

export const use30 = undefined + undefined
