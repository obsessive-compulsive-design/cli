// expect: E103 non-exhaustive @5:11
// A string has more values than any list of `case`s names: it needs a `default`.
export function weight(flag) {
  const name = `${flag}`
  switch (name) {
    case "true": {
      return 1
    }
    case "false": {
      return 0
    }
  }
  return 2
}
