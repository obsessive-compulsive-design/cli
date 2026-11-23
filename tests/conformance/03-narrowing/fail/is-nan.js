// expect: E138 type-mismatch @16:20
// expect: E138 type-mismatch @26:20
// expect: E138 type-mismatch @35:18
// expect: E138 type-mismatch @40:43
// expect: E138 type-mismatch @45:46
// expect: E138 type-mismatch @50:49
// expect: E138 type-mismatch @55:48
// expect: E138 type-mismatch @61:20
// Generated, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test narrowing
const isIt = (value, wanted) => value === wanted

export function inIfNegated(flag) {
  const subject = flag ? Math.random() : "a"
  if (!Number.isNaN(subject)) {
    return subject + 1
  }
  return 0
}

export function inElse(flag) {
  const subject = flag ? Math.random() : "a"
  if (Number.isNaN(subject)) {
    return 0
  } else {
    return subject + 1
  }
}

export function afterReturn(flag) {
  const subject = flag ? Math.random() : "a"
  if (Number.isNaN(subject)) {
    return 0
  }
  return subject + 1
}

export function inThenNegated(flag) {
  const subject = flag ? Math.random() : "a"
  return !Number.isNaN(subject) ? subject + 1 : 0
}

export function inOtherwise(flag) {
  const subject = flag ? Math.random() : "a"
  return Number.isNaN(subject) ? 0 : subject + 1
}

export function afterAndNegated(flag) {
  const subject = flag ? Math.random() : "a"
  return !Number.isNaN(subject) && isIt(subject + 1, 0)
}

export function afterOr(flag) {
  const subject = flag ? Math.random() : "a"
  return Number.isNaN(subject) || isIt(subject + 1, 0)
}

export function inWhileNegated(flag) {
  const subject = flag ? Math.random() : "a"
  while (!Number.isNaN(subject)) {
    return subject + 1
  }
  return 0
}
