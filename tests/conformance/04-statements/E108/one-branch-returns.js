// expect: E108 fallthrough @7:5
// Only one branch of the `if` leaves the `switch`: by the other, the end of the block
// is reached.
export function sign(kind, value) {
  let text = "+"
  switch (kind) {
    case "signed": {
      if (value < 0) {
        text = "-"
        break
      }
    }
    default: {
      text = `${text}?`
    }
  }
  return text
}
