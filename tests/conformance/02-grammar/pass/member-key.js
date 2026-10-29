// MemberKey: an identifier, a private name, and each of the four symbol keys.
class A {
  plain() {}
  #secret() {}
  [Symbol.iterator]() {}
  [Symbol.asyncIterator]() {}
  [Symbol.dispose]() {}
  [Symbol.asyncDispose]() {}
}
