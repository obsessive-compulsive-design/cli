// CaseClause: stacked labels of every literal kind, each clause a block.
const x = 1
switch (x) {
  case 1:
  case -1:
  case 1n:
  case "s":
  case true: {
    f(x)
    break
  }
  case 2.5: {
    break
  }
  default: {
    f(0)
  }
}
