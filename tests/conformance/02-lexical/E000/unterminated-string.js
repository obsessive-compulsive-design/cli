// expect: E000 syntax @3:17
// The string never closes before the line ends; lexing resumes on the next line.
const a = "never
const b = 1
