// expect: E107 block-shadow @6:9
// In the block `limit` would mean something else than around it.
const limit = 1

{
  const limit = 2
  limit
}

export const kept = limit
