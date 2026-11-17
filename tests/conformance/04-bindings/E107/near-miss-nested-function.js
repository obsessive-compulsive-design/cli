// A nested function may reuse an outer name: as a parameter, in its body, in a method.
// A global such as `fetch` is no binding, and blocks side by side share nothing.
const size = 1

export const scale = (size) => {
  const kept = size
  return (kept) => kept
}

export class Box {
  grow(size) {
    const scale = size
    return scale
  }
}

{
  const fetch = size
  fetch
}

{
  const fetch = 2
  fetch
}
