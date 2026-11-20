// Each is filled where its binding is in scope, from a nested arrow too: no diagnostic.
export const names = []

export const ages = new Map()

export const seen = new Set()

const slots = []

export const record = (name, age) => {
  names.push(name)
  ages.set(name, age)
  seen.add(age)
  slots[0] = name
  return slots
}
