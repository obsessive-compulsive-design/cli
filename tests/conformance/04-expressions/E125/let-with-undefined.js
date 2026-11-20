// expect: E125 template-non-string @10:25
// `last` is `undefined` until `note` runs, so it is `string | undefined`.
let last = undefined

export const note = (text) => {
  last = "seen"
  return text
}

export const shown = `${last}`
