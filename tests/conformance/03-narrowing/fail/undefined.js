// expect: E137 missing-property @24:20
// expect: E121 constant-condition @32:12
// expect: E121 constant-condition @42:12
// expect: E137 missing-property @51:20
// expect: E121 constant-condition @60:10
// expect: E137 missing-property @68:18
// expect: E137 missing-property @73:42
// expect: E121 constant-condition @78:34
// expect: E121 constant-condition @83:40
// expect: E137 missing-property @88:46
// expect: E137 missing-property @93:48
// expect: E121 constant-condition @98:40
// expect: E121 constant-condition @103:40
// expect: E137 missing-property @108:48
// expect: E137 missing-property @114:20
// expect: E121 constant-condition @122:12
// Generated, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test narrowing
const isIt = (value, wanted) => value === wanted

export function inIf(flag) {
  const subject = flag ? "a" : undefined
  if (subject === undefined) {
    return subject.length
  }
  return 0
}

export function inIfNegated(flag) {
  const subject = flag ? "a" : undefined
  if (subject !== undefined) {
    return subject ?? "b"
  }
  return "b"
}

export function inElse(flag) {
  const subject = flag ? "a" : undefined
  if (subject === undefined) {
    return "b"
  } else {
    return subject ?? "b"
  }
}

export function inElseNegated(flag) {
  const subject = flag ? "a" : undefined
  if (subject !== undefined) {
    return 0
  } else {
    return subject.length
  }
}

export function afterReturn(flag) {
  const subject = flag ? "a" : undefined
  if (subject === undefined) {
    return "b"
  }
  return subject ?? "b"
}

export function afterReturnNegated(flag) {
  const subject = flag ? "a" : undefined
  if (subject !== undefined) {
    return 0
  }
  return subject.length
}

export function inThen(flag) {
  const subject = flag ? "a" : undefined
  return subject === undefined ? subject.length : 0
}

export function inThenNegated(flag) {
  const subject = flag ? "a" : undefined
  return subject !== undefined ? subject ?? "b" : "b"
}

export function inOtherwise(flag) {
  const subject = flag ? "a" : undefined
  return subject === undefined ? "b" : subject ?? "b"
}

export function inOtherwiseNegated(flag) {
  const subject = flag ? "a" : undefined
  return subject !== undefined ? 0 : subject.length
}

export function afterAnd(flag) {
  const subject = flag ? "a" : undefined
  return subject === undefined && isIt(subject.length, 0)
}

export function afterAndNegated(flag) {
  const subject = flag ? "a" : undefined
  return subject !== undefined && isIt(subject ?? "b", "b")
}

export function afterOr(flag) {
  const subject = flag ? "a" : undefined
  return subject === undefined || isIt(subject ?? "b", "b")
}

export function afterOrNegated(flag) {
  const subject = flag ? "a" : undefined
  return subject !== undefined || isIt(subject.length, 0)
}

export function inWhile(flag) {
  const subject = flag ? "a" : undefined
  while (subject === undefined) {
    return subject.length
  }
  return 0
}

export function inWhileNegated(flag) {
  const subject = flag ? "a" : undefined
  while (subject !== undefined) {
    return subject ?? "b"
  }
  return "b"
}
