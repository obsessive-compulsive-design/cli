// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const uRLPattern = new URLPattern({ pathname: "/:id" })

export const use1 = new URLPattern()

export const use2 = uRLPattern.test()

export const use3 = uRLPattern.exec()

export const use4 = uRLPattern.protocol

export const use5 = uRLPattern.username

export const use6 = uRLPattern.password

export const use7 = uRLPattern.hostname

export const use8 = uRLPattern.port

export const use9 = uRLPattern.pathname

export const use10 = uRLPattern.search

export const use11 = uRLPattern.hash

export const use12 = uRLPattern.hasRegExpGroups
