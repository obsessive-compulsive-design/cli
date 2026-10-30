//! STORIES.md US-041: a snapshot per operator family, each line a source expression
//! and, under it, its canonical form.

mod common;

use common::formatted;

fn family(name: &str, inputs: &[&str]) {
    let mut text = String::new();
    for input in inputs {
        text.push_str(input);
        text.push_str("\n  => ");
        text.push_str(&formatted(input));
        text.push('\n');
    }
    sjs_testkit::snapshot!(&format!("expressions-{name}"), &text);
}

#[test]
fn arithmetic() {
    family(
        "arithmetic",
        &[
            "x=a+b-c",
            "x = a*b/c%d",
            "x = a ** b ** c",
            "x = (a + b) * c",
            "x = (a * b) + c",
            "x = a - (b - c)",
            "x = (a - b) - c",
            "x = (-a) ** 2",
            "x = -(a ** 2)",
            "x = a+-b",
            "x = a - -b",
            "x = a*(b+c)*d",
        ],
    );
}

#[test]
fn bitwise_and_shift() {
    family(
        "bitwise-and-shift",
        &[
            "x = a&b|c^d",
            "x = (a | b) & c",
            "x = a<<1>>2>>>3",
            "x = (a << 1) + 2",
            "x = a << (1 + 2)",
            "x = ~a&~b",
        ],
    );
}

#[test]
fn comparison_and_logic() {
    family(
        "comparison-and-logic",
        &[
            "x = a<b===c>d",
            "x = a <= b !== c >= d",
            "x = a instanceof B",
            "x = (a === b) === c",
            "x = a&&b||c",
            "x = (a || b) && c",
            "x = a ?? b",
            "x = (a ?? b) || c",
            "x = a || (b ?? c)",
            "x = (a ?? b) ?? c",
            "x = !a && !!b",
            "x = !(a && b)",
        ],
    );
}

#[test]
fn assignment_and_conditional() {
    family(
        "assignment-and-conditional",
        &[
            "a=b=c",
            "a+=1",
            "a -= b *= c",
            "a **= 2",
            "a ??= b",
            "a.b[c] = d",
            "(a) = 1",
            "x = a?b:c",
            "x = a ? b : c ? d : e",
            "x = (a ? b : c) ? d : e",
            "x = a ? (b, c) : d",
            "x = a ? b = 1 : c",
        ],
    );
}

#[test]
fn unary_and_update() {
    family(
        "unary-and-update",
        &[
            "x = - a",
            "x = + a",
            "x = ~ a",
            "x = ! a",
            "x = typeof  a",
            "x = typeof (a + b)",
            "x = typeof typeof a",
            "x = await  p",
            "x = (await p).then",
            "x = -(-a)",
            "x = - -a",
            "i ++",
            "-- i",
            "x = a.b ++",
        ],
    );
}

#[test]
fn members_calls_and_new() {
    family(
        "members-calls-and-new",
        &[
            "x = a . b . c",
            "x = a [ b ] [ 0 ]",
            "x = a ?. b ?. [ c ] ?. ( d )",
            "x = (a?.b).c",
            "x = f ( a , b )",
            "x = f ( )",
            "x = f ( ... xs , y )",
            "x = (a.b)()",
            "x = (() => 1)()",
            "x = new  A ( )",
            "x = new ns . A ( 1 , 2 )",
            "x = new A ( ) . b",
            "x = import ( './a.js' )",
            "x = this . a",
        ],
    );
}

#[test]
fn literals_arrays_and_objects() {
    family(
        "literals-arrays-and-objects",
        &[
            "x = 'single'",
            "x = 'it\\'s \"quoted\"'",
            "x = \"kept\"",
            "x = '\"'",
            "x = `a ${ b } c`",
            "x = `line\nbreak`",
            "x = String.raw`\\n`",
            "x = /re/v",
            "x = 1_000n",
            "x = [ 1 , 2 , 3 ]",
            "x = [ ]",
            "x = [ 1 , , 2 ]",
            "x = { a , b : 1 , 'c' : 2 , [ d ] : 3 , ... e }",
            "x = { }",
            "x = { a: { b: [ 1 ] } }",
        ],
    );
}

#[test]
fn arrows_and_patterns() {
    family(
        "arrows-and-patterns",
        &[
            "x = ( a ) => a",
            "x = async ( a , b = 1 , ... c ) => a",
            "x = ( ) => ( { a } )",
            "x = ( ) => ( a + b )",
            "x = ( ) => { return 1 }",
            "[ a , b = 1 , ... c ] = d",
            "( { a , b : c , ... d } = e )",
            "x = ( a ) => ( b ) => c",
        ],
    );
}
