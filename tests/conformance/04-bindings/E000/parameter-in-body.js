// expect: E000 syntax @4:9
// The body of a function is where its parameters live.
export const scale = (factor) => {
  const factor = 2
  return factor
}
