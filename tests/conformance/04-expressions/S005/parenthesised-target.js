// expect: S005 redundant-parentheses @5:3 "(a)" -> "a"
let a = 0

function set() {
  (a) = 1
}
