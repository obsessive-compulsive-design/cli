// Every member named: by a `switch` on a union of literals with stacked labels, on a
// boolean, on a discriminant two shapes share, and by a chain of `if`/`else if` whose
// branches return or throw; a string with a `default`: no diagnostic.
export function weight(flag, other) {
  const level = flag ? "low" : other ? "mid" : "high"
  switch (level) {
    case "low":
    case "mid": {
      return 1
    }
    case "high": {
      return 2
    }
  }
}

export function label(flag, other) {
  const on = flag && other
  switch (on) {
    case true: {
      return "on"
    }
    case false: {
      return "off"
    }
  }
}

export function area(flag, other) {
  const round = flag ? { kind: "circle", r: 1 } : { kind: "circle", d: 2 }
  const shape = other ? round : { kind: "square", side: 3 }
  switch (shape.kind) {
    case "circle": {
      return 1
    }
    case "square": {
      return shape.side
    }
  }
}

export function sign(flag, other) {
  const level = flag ? "low" : other ? "mid" : "high"
  if (level === "low") {
    return -1
  } else if (level === "mid") {
    return 0
  } else if (level === "high") {
    throw new Error(level)
  }
}

export function known(flag) {
  switch (`${flag}`) {
    case "true": {
      return 1
    }
    default: {
      return 0
    }
  }
}
