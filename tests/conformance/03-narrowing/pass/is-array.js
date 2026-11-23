// Generated, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test narrowing
const isIt = (value, wanted) => value === wanted

export function inIf(flag) {
  const subject = flag ? [1] : "a"
  if (Array.isArray(subject)) {
    return subject.join("")
  }
  return ""
}

export function inIfNegated(flag) {
  const subject = flag ? [1] : "a"
  if (!Array.isArray(subject)) {
    return subject.toUpperCase()
  }
  return ""
}

export function inElse(flag) {
  const subject = flag ? [1] : "a"
  if (Array.isArray(subject)) {
    return ""
  } else {
    return subject.toUpperCase()
  }
}

export function inElseNegated(flag) {
  const subject = flag ? [1] : "a"
  if (!Array.isArray(subject)) {
    return ""
  } else {
    return subject.join("")
  }
}

export function afterReturn(flag) {
  const subject = flag ? [1] : "a"
  if (Array.isArray(subject)) {
    return ""
  }
  return subject.toUpperCase()
}

export function afterReturnNegated(flag) {
  const subject = flag ? [1] : "a"
  if (!Array.isArray(subject)) {
    return ""
  }
  return subject.join("")
}

export function inThen(flag) {
  const subject = flag ? [1] : "a"
  return Array.isArray(subject) ? subject.join("") : ""
}

export function inThenNegated(flag) {
  const subject = flag ? [1] : "a"
  return !Array.isArray(subject) ? subject.toUpperCase() : ""
}

export function inOtherwise(flag) {
  const subject = flag ? [1] : "a"
  return Array.isArray(subject) ? "" : subject.toUpperCase()
}

export function inOtherwiseNegated(flag) {
  const subject = flag ? [1] : "a"
  return !Array.isArray(subject) ? "" : subject.join("")
}

export function afterAnd(flag) {
  const subject = flag ? [1] : "a"
  return Array.isArray(subject) && isIt(subject.join(""), "")
}

export function afterAndNegated(flag) {
  const subject = flag ? [1] : "a"
  return !Array.isArray(subject) && isIt(subject.toUpperCase(), "")
}

export function afterOr(flag) {
  const subject = flag ? [1] : "a"
  return Array.isArray(subject) || isIt(subject.toUpperCase(), "")
}

export function afterOrNegated(flag) {
  const subject = flag ? [1] : "a"
  return !Array.isArray(subject) || isIt(subject.join(""), "")
}

export function inWhile(flag) {
  const subject = flag ? [1] : "a"
  while (Array.isArray(subject)) {
    return subject.join("")
  }
  return ""
}

export function inWhileNegated(flag) {
  const subject = flag ? [1] : "a"
  while (!Array.isArray(subject)) {
    return subject.toUpperCase()
  }
  return ""
}
