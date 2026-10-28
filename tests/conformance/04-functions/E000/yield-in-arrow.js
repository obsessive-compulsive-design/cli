// expect: E000 syntax @4:19
// An arrow has no generator context of its own.
function* g() {
  const f = () => yield 1
}
