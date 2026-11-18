// expect: E115 unused-binding @6:12
// Nothing reads `error`: write `catch {`.
export const attempt = (run) => {
  try {
    return run()
  } catch (error) {
    return 0
  }
}
