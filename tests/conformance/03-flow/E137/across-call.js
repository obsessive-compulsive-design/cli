// expect: E137 missing-property @13:15
// `clear` assigns `word`, and `notify` may call it: past the call, nothing is known of
// `word` any more.
export function count(flag, notify) {
  let word = flag ? "a" : undefined
  const clear = () => {
    word = undefined
  }
  if (word === undefined) {
    return 0
  }
  notify(clear)
  return word.length
}
