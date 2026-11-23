// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const response = new Response("body")

export const use1 = new Response()

export const use2 = Response.error()

export const use3 = Response.redirect("a")

export const use4 = Response.json(1)

export const use5 = response.type

export const use6 = response.url

export const use7 = response.redirected

export const use8 = response.status

export const use9 = response.ok

export const use10 = response.statusText

export const use11 = response.headers

export const use12 = response.clone()

export const use13 = response.body

export const use14 = response.bodyUsed

export const use15 = response.arrayBuffer()

export const use16 = response.blob()

export const use17 = response.bytes()

export const use18 = response.formData()

export const use19 = response.json()

export const use20 = response.text()
