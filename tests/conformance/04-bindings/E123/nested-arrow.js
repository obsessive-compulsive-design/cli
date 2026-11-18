// expect: E123 modified-loop-variable @6:7
// An arrow inside the body changes the loop's variable.
const run = (n) => {
  for (let i = 0; i < n; i += 1) {
    const skip = () => {
      i += 1
    }
    skip()
  }
}

export { run }
