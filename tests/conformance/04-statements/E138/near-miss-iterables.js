// An array, a string, a `Map`, a generator and a class with `[Symbol.iterator]` are
// iterable, and `for await` takes an async generator: no diagnostic.
class Bag {
  *[Symbol.iterator]() {
    yield 1
    yield* [2, 3]
  }
}

function* letters() {
  yield "a"
  yield* "bc"
}

async function* ticks() {
  yield await Promise.resolve(1)
}

const pairs = new Map()

pairs.set("a", 1)

const seen = [""]

for (const letter of letters()) {
  seen.push(letter)
}

for (const [key, count] of pairs) {
  seen.push(`${key}${count}`)
}

for (const item of new Bag()) {
  seen.push(`${item}`)
}

export const watch = async () => {
  for await (const tick of ticks()) {
    seen.push(`${tick}`)
  }
}

export const doubled = letters().map((letter) => `${letter}${letter}`).toArray()

export const spread = [...seen, ..."de"]
