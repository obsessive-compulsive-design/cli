// expect: E137 missing-property @24:20
// expect: E121 constant-condition @32:12
// expect: E121 constant-condition @42:12
// expect: E137 missing-property @51:20
// expect: E121 constant-condition @60:10
// expect: E137 missing-property @68:18
// expect: E137 missing-property @73:37
// expect: E121 constant-condition @78:29
// expect: E121 constant-condition @83:33
// expect: E137 missing-property @88:41
// expect: E137 missing-property @93:43
// expect: E121 constant-condition @98:35
// expect: E121 constant-condition @103:35
// expect: E137 missing-property @108:43
// expect: E137 missing-property @114:20
// expect: E121 constant-condition @122:12
// Generated, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test narrowing
const isIt = (value, wanted) => value === wanted

export function inIf(flag) {
  const subject = /a/v.exec(flag ? "a" : "b")
  if (subject === null) {
    return subject.index
  }
  return 0
}

export function inIfNegated(flag) {
  const subject = /a/v.exec(flag ? "a" : "b")
  if (subject !== null) {
    return subject ?? 0
  }
  return 0
}

export function inElse(flag) {
  const subject = /a/v.exec(flag ? "a" : "b")
  if (subject === null) {
    return 0
  } else {
    return subject ?? 0
  }
}

export function inElseNegated(flag) {
  const subject = /a/v.exec(flag ? "a" : "b")
  if (subject !== null) {
    return 0
  } else {
    return subject.index
  }
}

export function afterReturn(flag) {
  const subject = /a/v.exec(flag ? "a" : "b")
  if (subject === null) {
    return 0
  }
  return subject ?? 0
}

export function afterReturnNegated(flag) {
  const subject = /a/v.exec(flag ? "a" : "b")
  if (subject !== null) {
    return 0
  }
  return subject.index
}

export function inThen(flag) {
  const subject = /a/v.exec(flag ? "a" : "b")
  return subject === null ? subject.index : 0
}

export function inThenNegated(flag) {
  const subject = /a/v.exec(flag ? "a" : "b")
  return subject !== null ? subject ?? 0 : 0
}

export function inOtherwise(flag) {
  const subject = /a/v.exec(flag ? "a" : "b")
  return subject === null ? 0 : subject ?? 0
}

export function inOtherwiseNegated(flag) {
  const subject = /a/v.exec(flag ? "a" : "b")
  return subject !== null ? 0 : subject.index
}

export function afterAnd(flag) {
  const subject = /a/v.exec(flag ? "a" : "b")
  return subject === null && isIt(subject.index, 0)
}

export function afterAndNegated(flag) {
  const subject = /a/v.exec(flag ? "a" : "b")
  return subject !== null && isIt(subject ?? 0, 0)
}

export function afterOr(flag) {
  const subject = /a/v.exec(flag ? "a" : "b")
  return subject === null || isIt(subject ?? 0, 0)
}

export function afterOrNegated(flag) {
  const subject = /a/v.exec(flag ? "a" : "b")
  return subject !== null || isIt(subject.index, 0)
}

export function inWhile(flag) {
  const subject = /a/v.exec(flag ? "a" : "b")
  while (subject === null) {
    return subject.index
  }
  return 0
}

export function inWhileNegated(flag) {
  const subject = /a/v.exec(flag ? "a" : "b")
  while (subject !== null) {
    return subject ?? 0
  }
  return 0
}
