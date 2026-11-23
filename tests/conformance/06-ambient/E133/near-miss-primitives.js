// The kept members of strings, numbers, `Math`, `BigInt` and `JSON`: no diagnostic.
const name = "strict"

const ratio = Math.max(1, 2) / Math.PI

export const text = [
  name.slice(1).toUpperCase(),
  ratio.toFixed(2),
  BigInt.asIntN(8, 10n).toString(2),
  JSON.stringify({ name }, undefined, 2),
]

export const count = Number.parseInt("08", 10) + (name.codePointAt(0) ?? 0)

export const copy = structuredClone({ name, parts: name.split("") })
