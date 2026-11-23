// The methods of an array are typed from what it holds, callbacks included; `at`
// may find nothing; a dictionary is made by `Object.fromEntries`: no diagnostic.
const sizes = [10, 9, 1]

export const total = sizes
  .filter((size) => size > 1)
  .map((size, place) => size * place)
  .reduce((sum, size) => sum + size, 0)

export const lastOrNone = (sizes.at(-1) ?? 0) + 1

export const found = [sizes.indexOf(9), sizes.indexOf(9.5)]

export const flat = [[1], [2, 3]].flat().join(",")

const shape = { low: 1, high: 2 }

export const byKey = Object.fromEntries(Object.entries(shape))

export const keys = Object.keys(shape).map((key) => `${key}!`)
