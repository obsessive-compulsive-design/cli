// expect: E138 type-mismatch @5:1
// A function declaration is a constant binding.
function run() {}

run = undefined

export { run }
