// expect: E109 finally-return @8:5
// The `return` of the `finally` replaces what the `try` returned, and swallows what
// it threw.
export function read(source) {
  try {
    return source()
  } finally {
    return 0
  }
}
