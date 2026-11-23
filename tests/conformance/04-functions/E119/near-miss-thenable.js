// A promise of a promise is awaited through, and an instance of a class with `then`
// is awaited as a promise is: no diagnostic.
class Ready {
  then(resolve) {
    resolve("ready")
  }
}

const inner = async () => await Promise.resolve(1)

const outer = async () => await inner()

export const values = [await outer(), await new Ready()]
