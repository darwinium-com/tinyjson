use std::char;
use std::collections::HashMap;
use std::iter::Peekable;
use std::str::Chars;

use crate::JsonValue;

#[derive(Debug)]
pub struct JsonParseError {
    msg: String,
    line: usize,
    col: usize,
}

impl JsonParseError {
    fn new(msg: String, line: usize, col: usize) -> JsonParseError {
        JsonParseError {
            msg: msg,
            line: line,
            col: col,
        }
    }
}

pub type JsonParseResult = Result<JsonValue, JsonParseError>;

pub struct JsonParser<I>
where
    I: Iterator<Item = char>,
{
    chars: Peekable<I>,
    line: usize,
    col: usize,
}

impl<I> JsonParser<I>
where
    I: Iterator<Item = char>,
{
    fn err(&self, msg: String) -> JsonParseResult {
        Err(self.error(msg))
    }

    fn error(&self, msg: String) -> JsonParseError {
        JsonParseError::new(msg, self.line, self.col)
    }

    fn unexpected_eof(&self) -> Result<char, JsonParseError> {
        Err(JsonParseError::new(
            String::from("Unexpected EOF"),
            self.line,
            self.col,
        ))
    }

    fn peek(&mut self) -> Result<char, JsonParseError> {
        loop {
            match self.chars.peek() {
                Some(c) => {
                    if !c.is_whitespace() {
                        return Ok(*c);
                    }
                    if *c == '\n' {
                        self.col = 0;
                        self.line += 1;
                    } else {
                        self.col += 1;
                    }
                }
                None => break,
            }
            self.chars.next();
        }
        self.unexpected_eof()
    }

    fn next(&mut self) -> Result<char, JsonParseError> {
        while let Some(c) = self.chars.next() {
            self.col += 1;
            if !c.is_whitespace() {
                return Ok(c);
            }
            if c == '\n' {
                self.col = 0;
                self.line += 1;
            }
        }
        self.unexpected_eof()
    }

    fn next_wo_skip(&mut self) -> Result<char, JsonParseError> {
        match self.chars.next() {
            Some(c) => Ok(c),
            None => self.unexpected_eof(),
        }
    }

    pub fn parse_object(&mut self) -> JsonParseResult {
        if self.next()? != '{' {
            return self.err(String::from("Object must starts with '{'"));
        }

        let mut m = HashMap::new();
        loop {
            let c = self.peek()?;
            if c == '}' {
                let _ = self.next();
                break;
            }

            let key = match self.parse()? {
                JsonValue::String(s) => s,
                v => return self.err(format!("Key of object must be string but found {:?}", v)),
            };

            let c = self.next()?;
            if c != ':' {
                return self.err(format!(
                    "':' is expected after key of object but actually found '{}'",
                    c
                ));
            }

            m.insert(key, self.parse()?);

            match self.peek()? {
                ',' => {
                    let _ = self.next();
                }
                '}' => {}
                c => {
                    return self.err(format!(
                        "',' is expected for object but actually found '{}'",
                        c
                    ))
                }
            }
        }

        Ok(JsonValue::Object(m))
    }

    pub fn parse_array(&mut self) -> JsonParseResult {
        if self.next()? != '[' {
            return self.err(String::from("Array must starts with '['"));
        }

        let mut v = vec![];
        loop {
            let c = self.peek()?;
            if c == ']' {
                let _ = self.next();
                break;
            }

            v.push(self.parse()?);

            match self.peek()? {
                ',' => {
                    let _ = self.next();
                }
                ']' => {}
                c => {
                    return self.err(format!(
                        "',' is expected for array but actually found '{}'",
                        c
                    ))
                }
            }
        }

        Ok(JsonValue::Array(v))
    }

    fn parse_special_char(&mut self) -> Result<char, JsonParseError> {
        Ok(match self.next()? {
            '\\' => '\\',
            '"' => '"',
            'b' => '\u{0008}',
            'f' => '\u{000c}',
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            'u' => {
                let mut u = 0 as u32;
                for _ in 0..4 {
                    let c = self.next()?;
                    let h = match c.to_digit(16) {
                            Some(n) => n,
                            None => return Err(self.error(format!("Unicode character must be \\uXXXX (X is hex character) format but found '{}'", c))),
                        };
                    u = u * 0x10 + h;
                }
                match char::from_u32(u) {
                    Some(c) => c,
                    None => {
                        return Err(
                            self.error(format!("Cannot convert \\u{:x} in unicode character", u))
                        )
                    }
                }
            }
            c => c,
        })
    }

    pub fn parse_string(&mut self) -> JsonParseResult {
        if self.next()? != '"' {
            return self.err(String::from("String must starts with double quote"));
        }

        let mut s = String::new();
        loop {
            s.push(match self.next_wo_skip()? {
                '\\' => self.parse_special_char()?,
                '"' => break,
                c => c,
            });
        }

        Ok(JsonValue::String(s))
    }

    fn parse_name(&mut self, s: &'static str) -> Option<JsonParseError> {
        for c in s.chars() {
            match self.next_wo_skip() {
                Ok(x) if x != c => {
                    return Some(JsonParseError::new(
                        format!("err while parsing '{}', invalid character '{}' found", s, c),
                        self.line,
                        self.col,
                    ));
                }
                Ok(_) => {}
                Err(e) => return Some(e),
            }
        }
        None
    }

    pub fn parse_null(&mut self) -> JsonParseResult {
        match self.parse_name("null") {
            Some(err) => Err(err),
            None => Ok(JsonValue::Null),
        }
    }

    pub fn parse_true(&mut self) -> JsonParseResult {
        match self.parse_name("true") {
            Some(err) => Err(err),
            None => Ok(JsonValue::Boolean(true)),
        }
    }

    pub fn parse_false(&mut self) -> JsonParseResult {
        match self.parse_name("false") {
            Some(err) => Err(err),
            None => Ok(JsonValue::Boolean(false)),
        }
    }

    pub fn parse_number(&mut self) -> JsonParseResult {
        let mut c = self.next()?;
        let negative = match c {
            '-' => {
                c = self.next()?;
                true
            }
            _ => false,
        };

        let mut s = c.to_string();
        loop {
            let d = match self.chars.peek() {
                Some(x) => *x,
                None => break,
            };

            s.push(match d {
                '0'..='9' | '.' | 'e' | 'E' => d,
                _ => break,
            });
            self.chars.next();
        }

        let n: f64 = match s.parse() {
            Ok(num) => num,
            Err(_) => return self.err(format!("Invalid number: {}", s)),
        };

        Ok(JsonValue::Number(if negative { -n } else { n }))
    }

    pub fn parse(&mut self) -> JsonParseResult {
        match self.peek()? {
            '1'..='9' | '-' => self.parse_number(),
            '"' => self.parse_string(),
            '[' => self.parse_array(),
            '{' => self.parse_object(),
            't' => self.parse_true(),
            'f' => self.parse_false(),
            'n' => self.parse_null(),
            c => self.err(format!("Invalid character: {}", c)),
        }
    }
}

pub fn make_parser<I>(it: I) -> JsonParser<I>
where
    I: Iterator<Item = char>,
{
    JsonParser {
        chars: it.peekable(),
        line: 0,
        col: 0,
    }
}

pub fn make_str_parser(s: &str) -> JsonParser<Chars<'_>> {
    make_parser(s.chars())
}

pub fn make_string_parser(s: &String) -> JsonParser<Chars<'_>> {
    make_parser(s.chars())
}

pub trait ParsableAsJson {
    fn parse_as_json(&self) -> JsonParseResult;
}

impl<'a> ParsableAsJson for &'a String {
    fn parse_as_json(&self) -> JsonParseResult {
        let mut p = make_parser(self.chars());
        p.parse()
    }
}

impl<'a> ParsableAsJson for &'a str {
    fn parse_as_json(&self) -> JsonParseResult {
        let mut p = make_parser(self.chars());
        p.parse()
    }
}

pub fn parse<T: ParsableAsJson>(parsable: T) -> JsonParseResult {
    parsable.parse_as_json()
}

pub fn must_parse<T: ParsableAsJson>(parsable: T) -> JsonValue {
    match parse(parsable) {
        Ok(json) => json,
        Err(err) => panic!("tinyjson: Parse failed: {:?}", err),
    }
}
