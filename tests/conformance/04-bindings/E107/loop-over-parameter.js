// expect: E107 block-shadow @4:14
// The loop is in the function whose parameter is `item`.
export const each = (item, items) => {
  for (const item of items) {
    item
  }
  return item
}
