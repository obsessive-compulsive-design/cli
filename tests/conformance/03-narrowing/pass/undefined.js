// Generated, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test narrowing
const isIt = (value, wanted) => value === wanted

export function inIf(flag) {
  const subject = flag ? "a" : undefined
  if (subject === undefined) {
    return subject ?? "b"
  }
  return "b"
}

export function inIfNegated(flag) {
  const subject = flag ? "a" : undefined
  if (subject !== undefined) {
    return subject.length
  }
  return 0
}

export function inElse(flag) {
  const subject = flag ? "a" : undefined
  if (subject === undefined) {
    return 0
  } else {
    return subject.length
  }
}

export function inElseNegated(flag) {
  const subject = flag ? "a" : undefined
  if (subject !== undefined) {
    return "b"
  } else {
    return subject ?? "b"
  }
}

export function afterReturn(flag) {
  const subject = flag ? "a" : undefined
  if (subject === undefined) {
    return 0
  }
  return subject.length
}

export function afterReturnNegated(flag) {
  const subject = flag ? "a" : undefined
  if (subject !== undefined) {
    return "b"
  }
  return subject ?? "b"
}

export function inThen(flag) {
  const subject = flag ? "a" : undefined
  return subject === undefined ? subject ?? "b" : "b"
}

export function inThenNegated(flag) {
  const subject = flag ? "a" : undefined
  return subject !== undefined ? subject.length : 0
}

export function inOtherwise(flag) {
  const subject = flag ? "a" : undefined
  return subject === undefined ? 0 : subject.length
}

export function inOtherwiseNegated(flag) {
  const subject = flag ? "a" : undefined
  return subject !== undefined ? "b" : subject ?? "b"
}

export function afterAnd(flag) {
  const subject = flag ? "a" : undefined
  return subject === undefined && isIt(subject ?? "b", "b")
}

export function afterAndNegated(flag) {
  const subject = flag ? "a" : undefined
  return subject !== undefined && isIt(subject.length, 0)
}

export function afterOr(flag) {
  const subject = flag ? "a" : undefined
  return subject === undefined || isIt(subject.length, 0)
}

export function afterOrNegated(flag) {
  const subject = flag ? "a" : undefined
  return subject !== undefined || isIt(subject ?? "b", "b")
}

export function inWhile(flag) {
  const subject = flag ? "a" : undefined
  while (subject === undefined) {
    return subject ?? "b"
  }
  return "b"
}

export function inWhileNegated(flag) {
  const subject = flag ? "a" : undefined
  while (subject !== undefined) {
    return subject.length
  }
  return 0
}
