// expect: E000 syntax @3:1
// Decorators are removed (SPEC.md §12); @ is not a token of the subset.
@sealed
class A {}
