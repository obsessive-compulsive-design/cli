// expect: E138 type-mismatch @24:20
// expect: E137 missing-property @32:20
// expect: E137 missing-property @42:20
// expect: E138 type-mismatch @51:20
// expect: E137 missing-property @60:18
// expect: E138 type-mismatch @68:18
// expect: E138 type-mismatch @73:48
// expect: E137 missing-property @78:48
// expect: E137 missing-property @83:52
// expect: E138 type-mismatch @88:52
// expect: E138 type-mismatch @93:54
// expect: E137 missing-property @98:54
// expect: E137 missing-property @103:54
// expect: E138 type-mismatch @108:54
// expect: E138 type-mismatch @114:20
// expect: E137 missing-property @122:20
// Generated, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test narrowing
const isIt = (value, wanted) => value === wanted

export function inIf(flag) {
  const subject = flag ? "a" : 1
  if (typeof subject === "string") {
    return subject + 1
  }
  return 0
}

export function inIfNegated(flag) {
  const subject = flag ? "a" : 1
  if (typeof subject !== "string") {
    return subject.length
  }
  return 0
}

export function inElse(flag) {
  const subject = flag ? "a" : 1
  if (typeof subject === "string") {
    return 0
  } else {
    return subject.length
  }
}

export function inElseNegated(flag) {
  const subject = flag ? "a" : 1
  if (typeof subject !== "string") {
    return 0
  } else {
    return subject + 1
  }
}

export function afterReturn(flag) {
  const subject = flag ? "a" : 1
  if (typeof subject === "string") {
    return 0
  }
  return subject.length
}

export function afterReturnNegated(flag) {
  const subject = flag ? "a" : 1
  if (typeof subject !== "string") {
    return 0
  }
  return subject + 1
}

export function inThen(flag) {
  const subject = flag ? "a" : 1
  return typeof subject === "string" ? subject + 1 : 0
}

export function inThenNegated(flag) {
  const subject = flag ? "a" : 1
  return typeof subject !== "string" ? subject.length : 0
}

export function inOtherwise(flag) {
  const subject = flag ? "a" : 1
  return typeof subject === "string" ? 0 : subject.length
}

export function inOtherwiseNegated(flag) {
  const subject = flag ? "a" : 1
  return typeof subject !== "string" ? 0 : subject + 1
}

export function afterAnd(flag) {
  const subject = flag ? "a" : 1
  return typeof subject === "string" && isIt(subject + 1, 0)
}

export function afterAndNegated(flag) {
  const subject = flag ? "a" : 1
  return typeof subject !== "string" && isIt(subject.length, 0)
}

export function afterOr(flag) {
  const subject = flag ? "a" : 1
  return typeof subject === "string" || isIt(subject.length, 0)
}

export function afterOrNegated(flag) {
  const subject = flag ? "a" : 1
  return typeof subject !== "string" || isIt(subject + 1, 0)
}

export function inWhile(flag) {
  const subject = flag ? "a" : 1
  while (typeof subject === "string") {
    return subject + 1
  }
  return 0
}

export function inWhileNegated(flag) {
  const subject = flag ? "a" : 1
  while (typeof subject !== "string") {
    return subject.length
  }
  return 0
}
