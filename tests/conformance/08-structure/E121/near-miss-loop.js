// `while (true)` is the one loop without end, and each of these is left: by a `break`
// in a nested `if`, by a `return`, by a `throw` inside a `switch`: no diagnostic.
export function drain(next, sink) {
  while (true) {
    const item = next()
    if (item === undefined) {
      break
    }
    sink(item)
  }
  while (true) {
    const item = next()
    switch (item) {
      case 0: {
        throw new Error("nothing left")
      }
      default: {
        sink(item)
      }
    }
  }
}

export function firstOver(limit, next) {
  while (true) {
    const item = next()
    if (item > limit) {
      return item
    }
  }
}
