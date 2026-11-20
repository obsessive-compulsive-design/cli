// A key every member has, a literal key, `?.` on a value that may be absent, and
// the keys of a shape: no diagnostic.
const box = { width: 1, height: 2, tag: "box" }

const key = "height"

let maybe = undefined

maybe = { width: 3 }

export const area = box.width * box[key]

export const width = maybe?.width

export const size = box.tag.length

export const keys = Object.keys(box)
