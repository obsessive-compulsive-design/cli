// Generated, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test narrowing
const isIt = (value, wanted) => value === wanted

export function inIf(flag) {
  const subject = flag ? Math.random() : "a"
  if (Number.isNaN(subject)) {
    return subject + 1
  }
  return 0
}

export function inElseNegated(flag) {
  const subject = flag ? Math.random() : "a"
  if (!Number.isNaN(subject)) {
    return 0
  } else {
    return subject + 1
  }
}

export function afterReturnNegated(flag) {
  const subject = flag ? Math.random() : "a"
  if (!Number.isNaN(subject)) {
    return 0
  }
  return subject + 1
}

export function inThen(flag) {
  const subject = flag ? Math.random() : "a"
  return Number.isNaN(subject) ? subject + 1 : 0
}

export function inOtherwiseNegated(flag) {
  const subject = flag ? Math.random() : "a"
  return !Number.isNaN(subject) ? 0 : subject + 1
}

export function afterAnd(flag) {
  const subject = flag ? Math.random() : "a"
  return Number.isNaN(subject) && isIt(subject + 1, 0)
}

export function afterOrNegated(flag) {
  const subject = flag ? Math.random() : "a"
  return !Number.isNaN(subject) || isIt(subject + 1, 0)
}

export function inWhile(flag) {
  const subject = flag ? Math.random() : "a"
  while (Number.isNaN(subject)) {
    return subject + 1
  }
  return 0
}
