// expect: E137 missing-property @24:20
// expect: E137 missing-property @32:20
// expect: E137 missing-property @42:20
// expect: E137 missing-property @51:20
// expect: E137 missing-property @60:18
// expect: E137 missing-property @68:18
// expect: E137 missing-property @73:46
// expect: E137 missing-property @78:46
// expect: E137 missing-property @83:50
// expect: E137 missing-property @88:50
// expect: E137 missing-property @93:52
// expect: E137 missing-property @98:52
// expect: E137 missing-property @103:52
// expect: E137 missing-property @108:52
// expect: E137 missing-property @114:20
// expect: E137 missing-property @122:20
// Generated, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test narrowing
const isIt = (value, wanted) => value === wanted

export function inIf(flag) {
  const subject = flag ? { kind: "circle", radius: 1 } : { kind: "square", side: 2 }
  if (subject.kind === "circle") {
    return subject.side
  }
  return 2
}

export function inIfNegated(flag) {
  const subject = flag ? { kind: "circle", radius: 1 } : { kind: "square", side: 2 }
  if (subject.kind !== "circle") {
    return subject.radius
  }
  return 1
}

export function inElse(flag) {
  const subject = flag ? { kind: "circle", radius: 1 } : { kind: "square", side: 2 }
  if (subject.kind === "circle") {
    return 1
  } else {
    return subject.radius
  }
}

export function inElseNegated(flag) {
  const subject = flag ? { kind: "circle", radius: 1 } : { kind: "square", side: 2 }
  if (subject.kind !== "circle") {
    return 2
  } else {
    return subject.side
  }
}

export function afterReturn(flag) {
  const subject = flag ? { kind: "circle", radius: 1 } : { kind: "square", side: 2 }
  if (subject.kind === "circle") {
    return 1
  }
  return subject.radius
}

export function afterReturnNegated(flag) {
  const subject = flag ? { kind: "circle", radius: 1 } : { kind: "square", side: 2 }
  if (subject.kind !== "circle") {
    return 2
  }
  return subject.side
}

export function inThen(flag) {
  const subject = flag ? { kind: "circle", radius: 1 } : { kind: "square", side: 2 }
  return subject.kind === "circle" ? subject.side : 2
}

export function inThenNegated(flag) {
  const subject = flag ? { kind: "circle", radius: 1 } : { kind: "square", side: 2 }
  return subject.kind !== "circle" ? subject.radius : 1
}

export function inOtherwise(flag) {
  const subject = flag ? { kind: "circle", radius: 1 } : { kind: "square", side: 2 }
  return subject.kind === "circle" ? 1 : subject.radius
}

export function inOtherwiseNegated(flag) {
  const subject = flag ? { kind: "circle", radius: 1 } : { kind: "square", side: 2 }
  return subject.kind !== "circle" ? 2 : subject.side
}

export function afterAnd(flag) {
  const subject = flag ? { kind: "circle", radius: 1 } : { kind: "square", side: 2 }
  return subject.kind === "circle" && isIt(subject.side, 2)
}

export function afterAndNegated(flag) {
  const subject = flag ? { kind: "circle", radius: 1 } : { kind: "square", side: 2 }
  return subject.kind !== "circle" && isIt(subject.radius, 1)
}

export function afterOr(flag) {
  const subject = flag ? { kind: "circle", radius: 1 } : { kind: "square", side: 2 }
  return subject.kind === "circle" || isIt(subject.radius, 1)
}

export function afterOrNegated(flag) {
  const subject = flag ? { kind: "circle", radius: 1 } : { kind: "square", side: 2 }
  return subject.kind !== "circle" || isIt(subject.side, 2)
}

export function inWhile(flag) {
  const subject = flag ? { kind: "circle", radius: 1 } : { kind: "square", side: 2 }
  while (subject.kind === "circle") {
    return subject.side
  }
  return 2
}

export function inWhileNegated(flag) {
  const subject = flag ? { kind: "circle", radius: 1 } : { kind: "square", side: 2 }
  while (subject.kind !== "circle") {
    return subject.radius
  }
  return 1
}
