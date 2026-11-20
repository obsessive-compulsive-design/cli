// expect: E133 removed-global-or-member @18:21
// expect: E133 removed-global-or-member @20:28
// expect: E133 removed-global-or-member @22:28
// expect: E133 removed-global-or-member @24:28
// expect: E133 removed-global-or-member @26:28
// expect: E133 removed-global-or-member @28:28
// expect: E133 removed-global-or-member @30:28
// expect: E133 removed-global-or-member @32:28
// expect: E133 removed-global-or-member @34:28
// expect: E133 removed-global-or-member @36:29
// expect: E133 removed-global-or-member @38:29
// expect: E133 removed-global-or-member @40:29
// expect: E133 removed-global-or-member @42:29
// expect: E133 removed-global-or-member @44:29
// expect: E133 removed-global-or-member @46:29
// Generated from ambient/globals.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test ambient
export const use1 = Symbol

export const use2 = Symbol.for

export const use3 = Symbol.hasInstance

export const use4 = Symbol.isConcatSpreadable

export const use5 = Symbol.keyFor

export const use6 = Symbol.match

export const use7 = Symbol.matchAll

export const use8 = Symbol.replace

export const use9 = Symbol.search

export const use10 = Symbol.species

export const use11 = Symbol.split

export const use12 = Symbol.toPrimitive

export const use13 = Symbol.toStringTag

export const use14 = Symbol.unscopables

export const use15 = Symbol.notKept
