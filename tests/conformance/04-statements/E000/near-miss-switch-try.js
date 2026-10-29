// Every permitted `switch` and `try` form: no diagnostic.
switch (x) {}
switch (x) {
  case 1n:
  case -1: {
    break
  }
  case "a": {}
  default:
  case true: {
    f()
  }
}
try {
  f()
} catch {
  g()
}
try {
  f()
} catch (e) {
  g(e)
} finally {
  h()
}
