// `then` on an instance of a class that declares it is an ordinary method call: no
// diagnostic.
class Step {
  then(resolve) {
    resolve(1)
  }
}

const seen = [0]

new Step().then((value) => seen.push(value))

export { seen }
