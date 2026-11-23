// An `async` function that awaits, by `await` or by `for await`; top-level `await`
// needs no function at all: no diagnostic.
async function* ticks() {
  yield await Promise.resolve(1)
}

export const total = async () => {
  let sum = 0
  for await (const tick of ticks()) {
    sum += tick
  }
  return sum
}

export const first = await total()
