// expect: E137 missing-property @24:20
// expect: E137 missing-property @32:20
// expect: E137 missing-property @42:20
// expect: E137 missing-property @51:20
// expect: E137 missing-property @60:18
// expect: E137 missing-property @68:18
// expect: E137 missing-property @73:43
// expect: E137 missing-property @78:44
// expect: E137 missing-property @83:48
// expect: E137 missing-property @88:49
// expect: E137 missing-property @93:49
// expect: E137 missing-property @98:50
// expect: E137 missing-property @103:49
// expect: E137 missing-property @108:50
// expect: E137 missing-property @114:20
// expect: E137 missing-property @122:20
// Generated, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test narrowing
const isIt = (value, wanted) => value === wanted

export function inIf(flag) {
  const subject = flag ? [1] : "a"
  if (Array.isArray(subject)) {
    return subject.toUpperCase()
  }
  return ""
}

export function inIfNegated(flag) {
  const subject = flag ? [1] : "a"
  if (!Array.isArray(subject)) {
    return subject.join("")
  }
  return ""
}

export function inElse(flag) {
  const subject = flag ? [1] : "a"
  if (Array.isArray(subject)) {
    return ""
  } else {
    return subject.join("")
  }
}

export function inElseNegated(flag) {
  const subject = flag ? [1] : "a"
  if (!Array.isArray(subject)) {
    return ""
  } else {
    return subject.toUpperCase()
  }
}

export function afterReturn(flag) {
  const subject = flag ? [1] : "a"
  if (Array.isArray(subject)) {
    return ""
  }
  return subject.join("")
}

export function afterReturnNegated(flag) {
  const subject = flag ? [1] : "a"
  if (!Array.isArray(subject)) {
    return ""
  }
  return subject.toUpperCase()
}

export function inThen(flag) {
  const subject = flag ? [1] : "a"
  return Array.isArray(subject) ? subject.toUpperCase() : ""
}

export function inThenNegated(flag) {
  const subject = flag ? [1] : "a"
  return !Array.isArray(subject) ? subject.join("") : ""
}

export function inOtherwise(flag) {
  const subject = flag ? [1] : "a"
  return Array.isArray(subject) ? "" : subject.join("")
}

export function inOtherwiseNegated(flag) {
  const subject = flag ? [1] : "a"
  return !Array.isArray(subject) ? "" : subject.toUpperCase()
}

export function afterAnd(flag) {
  const subject = flag ? [1] : "a"
  return Array.isArray(subject) && isIt(subject.toUpperCase(), "")
}

export function afterAndNegated(flag) {
  const subject = flag ? [1] : "a"
  return !Array.isArray(subject) && isIt(subject.join(""), "")
}

export function afterOr(flag) {
  const subject = flag ? [1] : "a"
  return Array.isArray(subject) || isIt(subject.join(""), "")
}

export function afterOrNegated(flag) {
  const subject = flag ? [1] : "a"
  return !Array.isArray(subject) || isIt(subject.toUpperCase(), "")
}

export function inWhile(flag) {
  const subject = flag ? [1] : "a"
  while (Array.isArray(subject)) {
    return subject.toUpperCase()
  }
  return ""
}

export function inWhileNegated(flag) {
  const subject = flag ? [1] : "a"
  while (!Array.isArray(subject)) {
    return subject.join("")
  }
  return ""
}
