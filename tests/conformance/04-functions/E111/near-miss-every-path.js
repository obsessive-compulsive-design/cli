// A value on every path, by `else`, by `switch` with a `default`, or by throwing:
// no diagnostic.
export function label(found) {
  if (found) {
    return "found"
  } else {
    return "missing"
  }
}

export function name(kind) {
  switch (kind) {
    case 1: {
      return "one"
    }
    default: {
      throw new Error("unknown kind")
    }
  }
}

export function log(text) {
  if (text === "") {
    return
  }
  console.log(text)
}
