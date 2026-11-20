// expect: E105 no-overlap @33:25
// expect: E105 no-overlap @35:25
// expect: E105 no-overlap @37:25
// expect: E105 no-overlap @39:25
// expect: E105 no-overlap @41:25
// expect: E105 no-overlap @43:23
// expect: E105 no-overlap @45:23
// expect: E105 no-overlap @47:23
// expect: E105 no-overlap @49:23
// expect: E105 no-overlap @51:24
// expect: E105 no-overlap @53:25
// expect: E105 no-overlap @55:25
// expect: E105 no-overlap @57:25
// expect: E105 no-overlap @59:25
// expect: E105 no-overlap @61:25
// expect: E105 no-overlap @63:26
// expect: E105 no-overlap @65:26
// expect: E105 no-overlap @67:26
// expect: E105 no-overlap @69:26
// expect: E105 no-overlap @71:26
// expect: E105 no-overlap @73:27
// expect: E105 no-overlap @75:27
// expect: E105 no-overlap @77:27
// expect: E105 no-overlap @79:27
// expect: E105 no-overlap @81:27
// expect: E105 no-overlap @83:32
// expect: E105 no-overlap @85:32
// expect: E105 no-overlap @87:32
// expect: E105 no-overlap @89:32
// expect: E105 no-overlap @91:32
// Generated from the table of SPEC.md §4.1, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test operators
export const use1 = 1.5 === 1

export const use2 = 1.5 === 2n

export const use3 = 1.5 === "a"

export const use4 = 1.5 === true

export const use5 = 1.5 === undefined

export const use6 = 1 === 1.5

export const use7 = 1 === 2n

export const use8 = 1 === "a"

export const use9 = 1 === true

export const use10 = 1 === undefined

export const use11 = 2n === 1.5

export const use12 = 2n === 1

export const use13 = 2n === "a"

export const use14 = 2n === true

export const use15 = 2n === undefined

export const use16 = "a" === 1.5

export const use17 = "a" === 1

export const use18 = "a" === 2n

export const use19 = "a" === true

export const use20 = "a" === undefined

export const use21 = true === 1.5

export const use22 = true === 1

export const use23 = true === 2n

export const use24 = true === "a"

export const use25 = true === undefined

export const use26 = undefined === 1.5

export const use27 = undefined === 1

export const use28 = undefined === 2n

export const use29 = undefined === "a"

export const use30 = undefined === true
