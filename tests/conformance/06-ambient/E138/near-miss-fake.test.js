// A test file may assign a global, to put a fake in its place: no diagnostic.
const real = fetch

const fake = () => {
  fetch = undefined
}

const restore = () => {
  fetch = real
}

beforeEach(fake)

afterEach(restore)
