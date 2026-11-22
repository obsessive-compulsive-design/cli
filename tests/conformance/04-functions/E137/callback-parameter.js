// expect: E137 missing-property @5:49
// `withPoint` passes its callback a point, which has no `z`.
const withPoint = (f) => f({ x: 1, y: 2 })

export const depth = withPoint((point) => point.z)
