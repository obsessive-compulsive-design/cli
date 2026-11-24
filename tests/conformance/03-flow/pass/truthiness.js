// A shape or `undefined` is never false when it is there: asked for its truth, it is
// narrowed by the answer, on its own, negated and beside a boolean: no diagnostic.
export function area(flag, scale) {
  const shape = flag ? { side: 2 } : undefined
  if (!shape) {
    return 0
  }
  return shape.side * scale
}

export function side(flag, wanted) {
  const shape = flag ? { side: 2 } : undefined
  if (wanted && shape) {
    return shape.side
  }
  return shape ? shape.side + 1 : 0
}
