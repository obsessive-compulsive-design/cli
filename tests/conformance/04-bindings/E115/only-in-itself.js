// expect: E115 unused-binding @3:10
// `loop` is read only by itself.
function loop() {
  return loop
}

export const kept = 2
