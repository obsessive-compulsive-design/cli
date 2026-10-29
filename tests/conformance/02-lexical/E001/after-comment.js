// expect: E001 ambiguous-line-start @5:1
// The hazard looks past a comment-only line.
const a = b
// a comment between the lines
(c)
