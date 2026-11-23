// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const uRL = new URL("https://example.com/")

export const use1 = new URL("a")

export const use2 = URL.parse("a")

export const use3 = URL.canParse("a")

export const use4 = uRL.href

export const use5 = uRL.origin

export const use6 = uRL.protocol

export const use7 = uRL.username

export const use8 = uRL.password

export const use9 = uRL.host

export const use10 = uRL.hostname

export const use11 = uRL.port

export const use12 = uRL.pathname

export const use13 = uRL.search

export const use14 = uRL.searchParams

export const use15 = uRL.hash

export const use16 = uRL.toJSON()

export const use17 = uRL.toString()
