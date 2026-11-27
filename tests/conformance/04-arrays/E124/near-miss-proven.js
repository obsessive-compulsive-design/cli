// A place shown to be in the array reads an element: the variable of a loop over the
// array, an index tested on both sides (the length first too), a number the length
// is more than, and a literal place of a tuple; a call that mutates nothing keeps
// the proof, and a read that is not proven is fine where `undefined` is asked for:
// no diagnostic.
export function sum(values, log) {
  let total = 0
  for (let at = 0; at < values.length; at += 1) {
    log(at)
    total += values[at]
  }
  return total
}

export function pick(values, at) {
  if (at >= 0 && values.length > at) {
    return values[at] * 2
  }
  return values[at] ?? 0
}

export function first(values) {
  if (values.length === 0) {
    return 0
  }
  return values[0] + 1
}

export function both() {
  const pair = [1.5, "a"]
  const [size, name] = pair
  return pair[0] + size + name.length
}

export const results = [sum([1, 2], (at) => at), pick([1, 2], 1), first([3]), both()]
