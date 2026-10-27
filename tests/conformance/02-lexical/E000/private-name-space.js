// expect: E000 syntax @5:3
// A private name is # directly followed by its identifier.
class A {
  #a = 1
  # b = 2
}
