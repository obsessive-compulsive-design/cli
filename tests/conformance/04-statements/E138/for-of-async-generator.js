// expect: E138 type-mismatch @9:20
// An async generator is iterated with `for await`.
async function* ticks() {
  yield 1
}

const seen = [0]

for (const tick of ticks()) {
  seen.push(tick)
}

export { seen }
