// expect: S005 redundant-parentheses @4:3 "(a)" -> "a"
let a = 0
function set() {
  (a) = 1
}
