// What is told of a `const` holds past a call and inside an arrow; a loop's condition
// narrows its body; each `case` of a `switch` on `typeof` is its kind: no diagnostic.
export function shout(flag, notify) {
  const word = flag ? "a" : undefined
  if (word === undefined) {
    return [""]
  }
  notify()
  return [1, 2].map(() => word.toUpperCase())
}

export function countdown(flag) {
  let left = flag ? 3 : undefined
  let rounds = 0
  while (left !== undefined) {
    rounds += left
    left = left > 1 ? left - 1 : undefined
  }
  return rounds
}

export function size(flag, other) {
  const value = flag ? "a" : other ? 2 : undefined
  switch (typeof value) {
    case "string": {
      return value.length
    }
    case "number": {
      return value + 1
    }
    default: {
      return value ?? 0
    }
  }
}
