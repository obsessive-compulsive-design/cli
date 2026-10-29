// expect: E001 ambiguous-line-start @4:1
// A line starting with a template continues the expression before it as a tag.
const raw = String.raw
`text`
