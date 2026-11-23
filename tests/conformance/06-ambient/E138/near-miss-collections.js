// A `Map`, a `Set`, a `WeakMap` and a typed array used by what they hold: no
// diagnostic.
const ages = new Map([["ada", 36]])

ages.set("alan", 41)

const key = { id: 1 }

const seen = new WeakMap([[key, true]])

const bytes = new Uint8Array(4)

bytes[0] = 255

const sizes = new Set([1, 2]).union(new Set([3]))

export const report = [
  ages.get("ada") ?? 0,
  ages.size,
  seen.has(key),
  bytes[0] ?? 0,
  bytes.length,
  sizes.has(3),
]

export const view = new DataView(bytes.buffer).getUint8(0)
