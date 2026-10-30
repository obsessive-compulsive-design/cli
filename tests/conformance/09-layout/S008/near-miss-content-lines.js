// Whitespace inside a template literal is content: no diagnostic.
const t = `a  

  b  
`
f(t)
