// The header updates the variable, and the body its own: no diagnostic.
const run = (n) => {
  let total = 0
  for (let i = 0; i < n; i += 2) {
    let step = i
    step += 1
    total += step
  }
  return total
}

export { run }
