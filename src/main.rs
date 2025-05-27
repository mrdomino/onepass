mod randexp;

use nom::{IResult, Parser, branch::alt, character::complete::char, combinator::value};

fn main() {
    println!("Hello, world!");
}

#[derive(Clone, Copy)]
enum PrimitiveClass {
    Single(char),
    Range(char, char),
}

#[derive(Clone, Copy)]
enum BuiltinClass {
    Alnum,
    Alpha,
    Digit,
    Lower,
    Upper,
    Space,
    Special,
    Word,
}

#[derive(Clone, Copy)]
enum BuiltinClassEntry {
    Builtin(BuiltinClass),
    Negated(BuiltinClass),
}

#[derive(Clone, Copy)]
enum CharClassEntry {
    PrimitiveClass,
    BuiltinClassEntry,
}

enum CharClass {
    Positive(Vec<CharClassEntry>),
    Negative(Vec<CharClassEntry>),
}

#[derive(Clone, Copy)]
enum Count {
    Single(i16),
    Range(i16, i16),
}

enum Schema {
    Word,
    CharClass,
    Counted(Box<Schema>, Count),
    Sequence(Vec<Schema>),
}

fn primitive_class(input: &str) -> IResult<&str, PrimitiveClass> {
    todo!()
}

fn builtin_class(input: &str) -> IResult<&str, BuiltinClassEntry> {
    let (input, _) = char('\\')(input)?;
    let (input, res) = alt((
        value(BuiltinClassEntry::Builtin(BuiltinClass::Digit), char('d')),
        value(BuiltinClassEntry::Negated(BuiltinClass::Digit), char('D')),
    ))
    .parse(input)?;
    Ok((input, res))
}
