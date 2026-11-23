// expect: E117 unreachable-code @9:3
// The `try` returns on every path, through its `finally`: nothing follows it.
export function read(source, close) {
  try {
    return source()
  } finally {
    close()
  }
  return close()
}
