// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const request = new Request("https://example.com/")

export const use1 = new Request("a")

export const use2 = request.method

export const use3 = request.url

export const use4 = request.headers

export const use5 = request.destination

export const use6 = request.referrer

export const use7 = request.referrerPolicy

export const use8 = request.mode

export const use9 = request.credentials

export const use10 = request.cache

export const use11 = request.redirect

export const use12 = request.integrity

export const use13 = request.keepalive

export const use14 = request.signal

export const use15 = request.duplex

export const use16 = request.clone()

export const use17 = request.body

export const use18 = request.bodyUsed

export const use19 = request.arrayBuffer()

export const use20 = request.blob()

export const use21 = request.bytes()

export const use22 = request.formData()

export const use23 = request.json()

export const use24 = request.text()
