// Generated, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test narrowing
const isIt = (value, wanted) => value === wanted

export function inIf(flag) {
  const subject = flag ? { kind: "circle", radius: 1 } : { kind: "square", side: 2 }
  if (subject.kind === "circle") {
    return subject.radius
  }
  return 1
}

export function inIfNegated(flag) {
  const subject = flag ? { kind: "circle", radius: 1 } : { kind: "square", side: 2 }
  if (subject.kind !== "circle") {
    return subject.side
  }
  return 2
}

export function inElse(flag) {
  const subject = flag ? { kind: "circle", radius: 1 } : { kind: "square", side: 2 }
  if (subject.kind === "circle") {
    return 2
  } else {
    return subject.side
  }
}

export function inElseNegated(flag) {
  const subject = flag ? { kind: "circle", radius: 1 } : { kind: "square", side: 2 }
  if (subject.kind !== "circle") {
    return 1
  } else {
    return subject.radius
  }
}

export function afterReturn(flag) {
  const subject = flag ? { kind: "circle", radius: 1 } : { kind: "square", side: 2 }
  if (subject.kind === "circle") {
    return 2
  }
  return subject.side
}

export function afterReturnNegated(flag) {
  const subject = flag ? { kind: "circle", radius: 1 } : { kind: "square", side: 2 }
  if (subject.kind !== "circle") {
    return 1
  }
  return subject.radius
}

export function inThen(flag) {
  const subject = flag ? { kind: "circle", radius: 1 } : { kind: "square", side: 2 }
  return subject.kind === "circle" ? subject.radius : 1
}

export function inThenNegated(flag) {
  const subject = flag ? { kind: "circle", radius: 1 } : { kind: "square", side: 2 }
  return subject.kind !== "circle" ? subject.side : 2
}

export function inOtherwise(flag) {
  const subject = flag ? { kind: "circle", radius: 1 } : { kind: "square", side: 2 }
  return subject.kind === "circle" ? 2 : subject.side
}

export function inOtherwiseNegated(flag) {
  const subject = flag ? { kind: "circle", radius: 1 } : { kind: "square", side: 2 }
  return subject.kind !== "circle" ? 1 : subject.radius
}

export function afterAnd(flag) {
  const subject = flag ? { kind: "circle", radius: 1 } : { kind: "square", side: 2 }
  return subject.kind === "circle" && isIt(subject.radius, 1)
}

export function afterAndNegated(flag) {
  const subject = flag ? { kind: "circle", radius: 1 } : { kind: "square", side: 2 }
  return subject.kind !== "circle" && isIt(subject.side, 2)
}

export function afterOr(flag) {
  const subject = flag ? { kind: "circle", radius: 1 } : { kind: "square", side: 2 }
  return subject.kind === "circle" || isIt(subject.side, 2)
}

export function afterOrNegated(flag) {
  const subject = flag ? { kind: "circle", radius: 1 } : { kind: "square", side: 2 }
  return subject.kind !== "circle" || isIt(subject.radius, 1)
}

export function inWhile(flag) {
  const subject = flag ? { kind: "circle", radius: 1 } : { kind: "square", side: 2 }
  while (subject.kind === "circle") {
    return subject.radius
  }
  return 1
}

export function inWhileNegated(flag) {
  const subject = flag ? { kind: "circle", radius: 1 } : { kind: "square", side: 2 }
  while (subject.kind !== "circle") {
    return subject.side
  }
  return 2
}
