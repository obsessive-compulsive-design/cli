// Generated, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test narrowing
const isIt = (value, wanted) => value === wanted

export function inIf(flag) {
  const subject = flag ? "a" : 1
  if (typeof subject === "string") {
    return subject.length
  }
  return 0
}

export function inIfNegated(flag) {
  const subject = flag ? "a" : 1
  if (typeof subject !== "string") {
    return subject + 1
  }
  return 0
}

export function inElse(flag) {
  const subject = flag ? "a" : 1
  if (typeof subject === "string") {
    return 0
  } else {
    return subject + 1
  }
}

export function inElseNegated(flag) {
  const subject = flag ? "a" : 1
  if (typeof subject !== "string") {
    return 0
  } else {
    return subject.length
  }
}

export function afterReturn(flag) {
  const subject = flag ? "a" : 1
  if (typeof subject === "string") {
    return 0
  }
  return subject + 1
}

export function afterReturnNegated(flag) {
  const subject = flag ? "a" : 1
  if (typeof subject !== "string") {
    return 0
  }
  return subject.length
}

export function inThen(flag) {
  const subject = flag ? "a" : 1
  return typeof subject === "string" ? subject.length : 0
}

export function inThenNegated(flag) {
  const subject = flag ? "a" : 1
  return typeof subject !== "string" ? subject + 1 : 0
}

export function inOtherwise(flag) {
  const subject = flag ? "a" : 1
  return typeof subject === "string" ? 0 : subject + 1
}

export function inOtherwiseNegated(flag) {
  const subject = flag ? "a" : 1
  return typeof subject !== "string" ? 0 : subject.length
}

export function afterAnd(flag) {
  const subject = flag ? "a" : 1
  return typeof subject === "string" && isIt(subject.length, 0)
}

export function afterAndNegated(flag) {
  const subject = flag ? "a" : 1
  return typeof subject !== "string" && isIt(subject + 1, 0)
}

export function afterOr(flag) {
  const subject = flag ? "a" : 1
  return typeof subject === "string" || isIt(subject + 1, 0)
}

export function afterOrNegated(flag) {
  const subject = flag ? "a" : 1
  return typeof subject !== "string" || isIt(subject.length, 0)
}

export function inWhile(flag) {
  const subject = flag ? "a" : 1
  while (typeof subject === "string") {
    return subject.length
  }
  return 0
}

export function inWhileNegated(flag) {
  const subject = flag ? "a" : 1
  while (typeof subject !== "string") {
    return subject + 1
  }
  return 0
}
