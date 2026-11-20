// expect: E133 removed-global-or-member @15:28
// expect: E133 removed-global-or-member @17:28
// expect: E133 removed-global-or-member @19:28
// expect: E133 removed-global-or-member @21:28
// expect: E133 removed-global-or-member @23:28
// expect: E133 removed-global-or-member @25:28
// expect: E133 removed-global-or-member @27:28
// expect: E133 removed-global-or-member @29:28
// expect: E133 removed-global-or-member @31:28
// expect: E133 removed-global-or-member @33:29
// expect: E133 removed-global-or-member @35:29
// expect: E133 removed-global-or-member @37:29
// Generated from ambient/globals.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test ambient
export const use1 = Object.create

export const use2 = Object.defineProperties

export const use3 = Object.defineProperty

export const use4 = Object.getOwnPropertyDescriptor

export const use5 = Object.getOwnPropertyNames

export const use6 = Object.getOwnPropertySymbols

export const use7 = Object.getPrototypeOf

export const use8 = Object.is

export const use9 = Object.preventExtensions

export const use10 = Object.seal

export const use11 = Object.setPrototypeOf

export const use12 = Object.notKept
