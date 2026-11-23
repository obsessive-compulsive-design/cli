// expect: E137 missing-property @32:20
// expect: E137 missing-property @40:20
// expect: E137 missing-property @50:20
// expect: E137 missing-property @59:20
// expect: E137 missing-property @68:18
// expect: E137 missing-property @76:18
// expect: E137 missing-property @81:46
// expect: E137 missing-property @86:49
// expect: E137 missing-property @91:50
// expect: E137 missing-property @96:53
// expect: E137 missing-property @101:52
// expect: E137 missing-property @106:55
// expect: E137 missing-property @111:52
// expect: E137 missing-property @116:55
// expect: E137 missing-property @122:20
// expect: E137 missing-property @130:20
// Generated, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test narrowing
const isIt = (value, wanted) => value === wanted

class Circle {
  radius = 1
}

class Square {
  side = 2
}

export function inIf(flag) {
  const subject = flag ? new Circle() : new Square()
  if (subject instanceof Circle) {
    return subject.side
  }
  return 2
}

export function inIfNegated(flag) {
  const subject = flag ? new Circle() : new Square()
  if (!(subject instanceof Circle)) {
    return subject.radius
  }
  return 1
}

export function inElse(flag) {
  const subject = flag ? new Circle() : new Square()
  if (subject instanceof Circle) {
    return 1
  } else {
    return subject.radius
  }
}

export function inElseNegated(flag) {
  const subject = flag ? new Circle() : new Square()
  if (!(subject instanceof Circle)) {
    return 2
  } else {
    return subject.side
  }
}

export function afterReturn(flag) {
  const subject = flag ? new Circle() : new Square()
  if (subject instanceof Circle) {
    return 1
  }
  return subject.radius
}

export function afterReturnNegated(flag) {
  const subject = flag ? new Circle() : new Square()
  if (!(subject instanceof Circle)) {
    return 2
  }
  return subject.side
}

export function inThen(flag) {
  const subject = flag ? new Circle() : new Square()
  return subject instanceof Circle ? subject.side : 2
}

export function inThenNegated(flag) {
  const subject = flag ? new Circle() : new Square()
  return !(subject instanceof Circle) ? subject.radius : 1
}

export function inOtherwise(flag) {
  const subject = flag ? new Circle() : new Square()
  return subject instanceof Circle ? 1 : subject.radius
}

export function inOtherwiseNegated(flag) {
  const subject = flag ? new Circle() : new Square()
  return !(subject instanceof Circle) ? 2 : subject.side
}

export function afterAnd(flag) {
  const subject = flag ? new Circle() : new Square()
  return subject instanceof Circle && isIt(subject.side, 2)
}

export function afterAndNegated(flag) {
  const subject = flag ? new Circle() : new Square()
  return !(subject instanceof Circle) && isIt(subject.radius, 1)
}

export function afterOr(flag) {
  const subject = flag ? new Circle() : new Square()
  return subject instanceof Circle || isIt(subject.radius, 1)
}

export function afterOrNegated(flag) {
  const subject = flag ? new Circle() : new Square()
  return !(subject instanceof Circle) || isIt(subject.side, 2)
}

export function inWhile(flag) {
  const subject = flag ? new Circle() : new Square()
  while (subject instanceof Circle) {
    return subject.side
  }
  return 2
}

export function inWhileNegated(flag) {
  const subject = flag ? new Circle() : new Square()
  while (!(subject instanceof Circle)) {
    return subject.radius
  }
  return 1
}
