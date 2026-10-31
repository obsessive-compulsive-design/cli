// MemberKey: an identifier, a private name, and each of the four symbol keys.
class A {
  plain() {}
  [Symbol.iterator]() {}
  [Symbol.asyncIterator]() {}
  [Symbol.dispose]() {}
  [Symbol.asyncDispose]() {}
  #secret() {}
}
