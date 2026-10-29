// expect: E000 syntax @3:7
function f(xs) {
  for await (const x of xs) {}
}
