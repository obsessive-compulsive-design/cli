// A `finally` may throw, and a function written in one returns for itself: no
// diagnostic.
export function read(source, close) {
  try {
    return source()
  } finally {
    const report = (code) => {
      if (code === 0) {
        return "ok"
      }
      return "failed"
    }
    if (close(report) === 0) {
      throw new Error("not closed")
    }
  }
}
