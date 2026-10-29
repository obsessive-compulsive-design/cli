// expect: E000 syntax @8:7
// expect: E000 syntax @9:4
// expect: E000 syntax @10:13 "==" -> "==="
// expect: E000 syntax @11:15
// expect: E000 syntax @12:11 "new A" -> "new A()"
// Five independent faults: each is reported once, and the parser recovers from each
// to find the next (STORIES.md US-032).
const = 1
if x {}
const b = a == c
const d = [1, , 2]
const e = new A
