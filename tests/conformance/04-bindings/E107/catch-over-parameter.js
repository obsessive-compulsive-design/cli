// expect: E107 block-shadow @6:12
// A `catch` binding is a name of a block too.
export const guard = (error, run) => {
  try {
    run()
  } catch (error) {
    return error
  }
  return error
}
