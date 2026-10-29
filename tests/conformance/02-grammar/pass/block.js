// Block: declarations and statements inside braces, nested.
{
  const a = 1
  {
    let b = a
    b += 1
    f(b)
  }
  function g() {}
  class C {}
  g(new C())
}
