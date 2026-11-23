// Generated, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test narrowing
const isIt = (value, wanted) => value === wanted

export function inIf(flag) {
  const subject = /a/v.exec(flag ? "a" : "b")
  if (subject === null) {
    return subject ?? 0
  }
  return 0
}

export function inIfNegated(flag) {
  const subject = /a/v.exec(flag ? "a" : "b")
  if (subject !== null) {
    return subject.index
  }
  return 0
}

export function inElse(flag) {
  const subject = /a/v.exec(flag ? "a" : "b")
  if (subject === null) {
    return 0
  } else {
    return subject.index
  }
}

export function inElseNegated(flag) {
  const subject = /a/v.exec(flag ? "a" : "b")
  if (subject !== null) {
    return 0
  } else {
    return subject ?? 0
  }
}

export function afterReturn(flag) {
  const subject = /a/v.exec(flag ? "a" : "b")
  if (subject === null) {
    return 0
  }
  return subject.index
}

export function afterReturnNegated(flag) {
  const subject = /a/v.exec(flag ? "a" : "b")
  if (subject !== null) {
    return 0
  }
  return subject ?? 0
}

export function inThen(flag) {
  const subject = /a/v.exec(flag ? "a" : "b")
  return subject === null ? subject ?? 0 : 0
}

export function inThenNegated(flag) {
  const subject = /a/v.exec(flag ? "a" : "b")
  return subject !== null ? subject.index : 0
}

export function inOtherwise(flag) {
  const subject = /a/v.exec(flag ? "a" : "b")
  return subject === null ? 0 : subject.index
}

export function inOtherwiseNegated(flag) {
  const subject = /a/v.exec(flag ? "a" : "b")
  return subject !== null ? 0 : subject ?? 0
}

export function afterAnd(flag) {
  const subject = /a/v.exec(flag ? "a" : "b")
  return subject === null && isIt(subject ?? 0, 0)
}

export function afterAndNegated(flag) {
  const subject = /a/v.exec(flag ? "a" : "b")
  return subject !== null && isIt(subject.index, 0)
}

export function afterOr(flag) {
  const subject = /a/v.exec(flag ? "a" : "b")
  return subject === null || isIt(subject.index, 0)
}

export function afterOrNegated(flag) {
  const subject = /a/v.exec(flag ? "a" : "b")
  return subject !== null || isIt(subject ?? 0, 0)
}

export function inWhile(flag) {
  const subject = /a/v.exec(flag ? "a" : "b")
  while (subject === null) {
    return subject ?? 0
  }
  return 0
}

export function inWhileNegated(flag) {
  const subject = /a/v.exec(flag ? "a" : "b")
  while (subject !== null) {
    return subject.index
  }
  return 0
}
