// expect: E138 type-mismatch @35:25
// expect: E138 type-mismatch @37:25
// expect: E138 type-mismatch @39:25
// expect: E138 type-mismatch @41:25
// expect: E138 type-mismatch @43:23
// expect: E138 type-mismatch @45:23
// expect: E138 type-mismatch @47:23
// expect: E138 type-mismatch @49:23
// expect: E138 type-mismatch @51:24
// expect: E138 type-mismatch @53:25
// expect: E138 type-mismatch @55:25
// expect: E138 type-mismatch @57:25
// expect: E138 type-mismatch @59:25
// expect: E138 type-mismatch @61:25
// expect: E138 type-mismatch @63:26
// expect: E138 type-mismatch @65:26
// expect: E138 type-mismatch @67:26
// expect: E138 type-mismatch @69:26
// expect: E138 type-mismatch @71:26
// expect: E138 type-mismatch @73:26
// expect: E138 type-mismatch @75:27
// expect: E138 type-mismatch @77:27
// expect: E138 type-mismatch @79:27
// expect: E138 type-mismatch @81:27
// expect: E138 type-mismatch @83:27
// expect: E138 type-mismatch @85:27
// expect: E138 type-mismatch @87:32
// expect: E138 type-mismatch @89:32
// expect: E138 type-mismatch @91:32
// expect: E138 type-mismatch @93:32
// expect: E138 type-mismatch @95:32
// expect: E138 type-mismatch @97:32
// Generated from the table of SPEC.md §4.1, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test operators
export const use1 = 1.5 >>> 2n

export const use2 = 1.5 >>> "a"

export const use3 = 1.5 >>> true

export const use4 = 1.5 >>> undefined

export const use5 = 1 >>> 2n

export const use6 = 1 >>> "a"

export const use7 = 1 >>> true

export const use8 = 1 >>> undefined

export const use9 = 2n >>> 1.5

export const use10 = 2n >>> 1

export const use11 = 2n >>> 2n

export const use12 = 2n >>> "a"

export const use13 = 2n >>> true

export const use14 = 2n >>> undefined

export const use15 = "a" >>> 1.5

export const use16 = "a" >>> 1

export const use17 = "a" >>> 2n

export const use18 = "a" >>> "a"

export const use19 = "a" >>> true

export const use20 = "a" >>> undefined

export const use21 = true >>> 1.5

export const use22 = true >>> 1

export const use23 = true >>> 2n

export const use24 = true >>> "a"

export const use25 = true >>> true

export const use26 = true >>> undefined

export const use27 = undefined >>> 1.5

export const use28 = undefined >>> 1

export const use29 = undefined >>> 2n

export const use30 = undefined >>> "a"

export const use31 = undefined >>> true

export const use32 = undefined >>> undefined
