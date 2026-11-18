// expect: E123 modified-loop-variable @5:5
// `i += 1` in the header is the loop's own; the one in the body is not.
const run = (n) => {
  for (let i = 0; i < n; i += 1) {
    i += 1
    console.log(i)
  }
}

export { run }
