// In a test file the test globals resolve: no diagnostic.
const adds = () => {
  expect(1 + 1).toBe(2)
}

const suite = () => {
  beforeEach(adds)
  it("adds", adds)
  it.skip("subtracts", adds)
}

describe("sum", suite)
