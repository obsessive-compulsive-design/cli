// expect: E121 constant-condition @5:7
// expect: E121 constant-condition @9:7
// A condition written `true`, and a `const` that is `false`: each is always the same.
export function run(log) {
  if (true) {
    log("always")
  }
  const verbose = false
  if (verbose) {
    log("never")
  }
}
