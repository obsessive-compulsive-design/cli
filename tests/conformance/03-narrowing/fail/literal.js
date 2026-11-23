// expect: E137 missing-property @28:18
// expect: E137 missing-property @36:18
// expect: E137 missing-property @46:18
// expect: E137 missing-property @55:18
// expect: E137 missing-property @64:16
// expect: E137 missing-property @72:16
// expect: E137 missing-property @77:34
// expect: E137 missing-property @82:34
// expect: E137 missing-property @87:38
// expect: E137 missing-property @92:38
// expect: E137 missing-property @97:40
// expect: E137 missing-property @102:40
// expect: E137 missing-property @107:40
// expect: E137 missing-property @112:40
// expect: E137 missing-property @118:18
// expect: E137 missing-property @126:18
// Generated, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test narrowing
const isIt = (value, wanted) => value === wanted

const onlyA = { a: 1 }

const onlyB = { b: 2 }

export function inIf(flag) {
  const subject = flag ? "a" : "b"
  if (subject === "a") {
    return onlyB[subject]
  }
  return 2
}

export function inIfNegated(flag) {
  const subject = flag ? "a" : "b"
  if (subject !== "a") {
    return onlyA[subject]
  }
  return 1
}

export function inElse(flag) {
  const subject = flag ? "a" : "b"
  if (subject === "a") {
    return 1
  } else {
    return onlyA[subject]
  }
}

export function inElseNegated(flag) {
  const subject = flag ? "a" : "b"
  if (subject !== "a") {
    return 2
  } else {
    return onlyB[subject]
  }
}

export function afterReturn(flag) {
  const subject = flag ? "a" : "b"
  if (subject === "a") {
    return 1
  }
  return onlyA[subject]
}

export function afterReturnNegated(flag) {
  const subject = flag ? "a" : "b"
  if (subject !== "a") {
    return 2
  }
  return onlyB[subject]
}

export function inThen(flag) {
  const subject = flag ? "a" : "b"
  return subject === "a" ? onlyB[subject] : 2
}

export function inThenNegated(flag) {
  const subject = flag ? "a" : "b"
  return subject !== "a" ? onlyA[subject] : 1
}

export function inOtherwise(flag) {
  const subject = flag ? "a" : "b"
  return subject === "a" ? 1 : onlyA[subject]
}

export function inOtherwiseNegated(flag) {
  const subject = flag ? "a" : "b"
  return subject !== "a" ? 2 : onlyB[subject]
}

export function afterAnd(flag) {
  const subject = flag ? "a" : "b"
  return subject === "a" && isIt(onlyB[subject], 2)
}

export function afterAndNegated(flag) {
  const subject = flag ? "a" : "b"
  return subject !== "a" && isIt(onlyA[subject], 1)
}

export function afterOr(flag) {
  const subject = flag ? "a" : "b"
  return subject === "a" || isIt(onlyA[subject], 1)
}

export function afterOrNegated(flag) {
  const subject = flag ? "a" : "b"
  return subject !== "a" || isIt(onlyB[subject], 2)
}

export function inWhile(flag) {
  const subject = flag ? "a" : "b"
  while (subject === "a") {
    return onlyB[subject]
  }
  return 2
}

export function inWhileNegated(flag) {
  const subject = flag ? "a" : "b"
  while (subject !== "a") {
    return onlyA[subject]
  }
  return 1
}
