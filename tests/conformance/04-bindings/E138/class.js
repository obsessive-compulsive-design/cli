// expect: E138 type-mismatch @5:1
// A class declaration is a constant binding.
class Queue {}

Queue = undefined

export { Queue }
