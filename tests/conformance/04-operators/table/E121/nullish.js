// expect: E121 constant-condition @33:21
// expect: E121 constant-condition @35:21
// expect: E121 constant-condition @37:21
// expect: E121 constant-condition @39:21
// expect: E121 constant-condition @41:21
// expect: E121 constant-condition @43:21
// expect: E121 constant-condition @45:21
// expect: E121 constant-condition @47:21
// expect: E121 constant-condition @49:21
// expect: E121 constant-condition @51:22
// expect: E121 constant-condition @53:22
// expect: E121 constant-condition @55:22
// expect: E121 constant-condition @57:22
// expect: E121 constant-condition @59:22
// expect: E121 constant-condition @61:22
// expect: E121 constant-condition @63:22
// expect: E121 constant-condition @65:22
// expect: E121 constant-condition @67:22
// expect: E121 constant-condition @69:22
// expect: E121 constant-condition @71:22
// expect: E121 constant-condition @73:22
// expect: E121 constant-condition @75:22
// expect: E121 constant-condition @77:22
// expect: E121 constant-condition @79:22
// expect: E121 constant-condition @81:22
// expect: E121 constant-condition @83:22
// expect: E121 constant-condition @85:22
// expect: E121 constant-condition @87:22
// expect: E121 constant-condition @89:22
// expect: E121 constant-condition @91:22
// Generated from the table of SPEC.md §4.1, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test operators
export const use1 = 1.5 ?? 1.5

export const use2 = 1.5 ?? 1

export const use3 = 1.5 ?? 2n

export const use4 = 1.5 ?? "a"

export const use5 = 1.5 ?? true

export const use6 = 1.5 ?? undefined

export const use7 = 1 ?? 1.5

export const use8 = 1 ?? 1

export const use9 = 1 ?? 2n

export const use10 = 1 ?? "a"

export const use11 = 1 ?? true

export const use12 = 1 ?? undefined

export const use13 = 2n ?? 1.5

export const use14 = 2n ?? 1

export const use15 = 2n ?? 2n

export const use16 = 2n ?? "a"

export const use17 = 2n ?? true

export const use18 = 2n ?? undefined

export const use19 = "a" ?? 1.5

export const use20 = "a" ?? 1

export const use21 = "a" ?? 2n

export const use22 = "a" ?? "a"

export const use23 = "a" ?? true

export const use24 = "a" ?? undefined

export const use25 = true ?? 1.5

export const use26 = true ?? 1

export const use27 = true ?? 2n

export const use28 = true ?? "a"

export const use29 = true ?? true

export const use30 = true ?? undefined
