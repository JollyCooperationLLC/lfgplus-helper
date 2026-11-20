//! Reads and writes the small part of Lua that World of Warcraft uses for SavedVariables files: assignments of
//! tables, strings, numbers, booleans and nil to global names. No Lua runs; anything else is an error.

use std::fmt;

/// Deeper nesting than this is refused, so a hostile file can't exhaust the stack.
const MAX_DEPTH: usize = 64;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Nil,
    Bool(bool),
    Num(f64),
    Str(String),
    Table(Table),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Key {
    Bool(bool),
    Num(f64),
    Str(String),
}

/// A Lua table as its entries, in file order. Positional values get the next integer key, as in Lua.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Table {
    pub entries: Vec<(Key, Value)>,
}

impl Table {
    /// The value stored under a string key.
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.entries.iter().rev().find_map(|(k, v)| match k {
            Key::Str(s) if s == name => Some(v),
            _ => None,
        })
    }

    /// The array part: the values at keys 1, 2, 3… up to the first missing one.
    pub fn list(&self) -> Vec<&Value> {
        let mut out = Vec::new();
        loop {
            let want = (out.len() + 1) as f64;
            let found = self.entries.iter().rev().find_map(|(k, v)| match k {
                Key::Num(n) if *n == want => Some(v),
                _ => None,
            });
            match found {
                Some(Value::Nil) | None => return out,
                Some(v) => out.push(v),
            }
        }
    }
}

impl Value {
    pub fn as_table(&self) -> Option<&Table> {
        match self {
            Value::Table(t) => Some(t),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    /// A whole, non-negative number that fits a u64.
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Value::Num(n) if n.is_finite() && *n >= 0.0 && n.fract() == 0.0 && *n < 1.8e19 => Some(*n as u64),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub offset: usize,
    pub message: &'static str,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.message, self.offset)
    }
}

impl std::error::Error for ParseError {}

/// Parses a SavedVariables file into its global assignments, in order.
pub fn parse(source: &str) -> Result<Vec<(String, Value)>, ParseError> {
    let mut p = Parser { src: source.as_bytes(), pos: 0 };
    let mut globals = Vec::new();
    loop {
        p.skip_space()?;
        if p.pos >= p.src.len() {
            return Ok(globals);
        }
        let name = p.name().ok_or_else(|| p.error("expected a name"))?;
        p.skip_space()?;
        p.expect(b'=')?;
        let value = p.value(0)?;
        globals.push((name, value));
        p.skip_space()?;
        if p.peek() == Some(b';') {
            p.pos += 1;
        }
    }
}

/// The value assigned to a global, if the file assigns it.
pub fn global<'a>(globals: &'a [(String, Value)], name: &str) -> Option<&'a Value> {
    globals.iter().rev().find(|(n, _)| n == name).map(|(_, v)| v)
}

struct Parser<'a> {
    src: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn error(&self, message: &'static str) -> ParseError {
        ParseError { offset: self.pos, message }
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn peek_at(&self, ahead: usize) -> Option<u8> {
        self.src.get(self.pos + ahead).copied()
    }

    fn expect(&mut self, byte: u8) -> Result<(), ParseError> {
        if self.peek() == Some(byte) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.error("unexpected character"))
        }
    }

    /// Skips whitespace and comments (`-- line` and `--[[ block ]]`, with any level of `=`).
    fn skip_space(&mut self) -> Result<(), ParseError> {
        loop {
            match self.peek() {
                Some(b' ' | b'\t' | b'\r' | b'\n' | 0x0b | 0x0c) => self.pos += 1,
                Some(b'-') if self.peek_at(1) == Some(b'-') => {
                    self.pos += 2;
                    if self.peek() == Some(b'[') && self.long_bracket_level().is_some() {
                        self.long_string()?;
                    } else {
                        while let Some(c) = self.peek() {
                            if c == b'\n' {
                                break;
                            }
                            self.pos += 1;
                        }
                    }
                }
                Some(0xef) if self.pos == 0 && self.src.starts_with(&[0xef, 0xbb, 0xbf]) => self.pos += 3,
                _ => return Ok(()),
            }
        }
    }

    fn name(&mut self) -> Option<String> {
        let start = self.pos;
        match self.peek() {
            Some(c) if c.is_ascii_alphabetic() || c == b'_' => self.pos += 1,
            _ => return None,
        }
        while matches!(self.peek(), Some(c) if c.is_ascii_alphanumeric() || c == b'_') {
            self.pos += 1;
        }
        Some(String::from_utf8_lossy(&self.src[start..self.pos]).into_owned())
    }

    fn value(&mut self, depth: usize) -> Result<Value, ParseError> {
        self.skip_space()?;
        match self.peek() {
            Some(b'{') => {
                if depth >= MAX_DEPTH {
                    return Err(self.error("tables nested too deeply"));
                }
                Ok(Value::Table(self.table(depth + 1)?))
            }
            Some(b'"' | b'\'') => Ok(Value::Str(self.quoted()?)),
            Some(b'[') if self.long_bracket_level().is_some() => Ok(Value::Str(self.long_string()?)),
            Some(b'-') => {
                self.pos += 1;
                match self.value(depth)? {
                    Value::Num(n) => Ok(Value::Num(-n)),
                    _ => Err(self.error("minus before something that isn't a number")),
                }
            }
            Some(c) if c.is_ascii_digit() || c == b'.' => Ok(Value::Num(self.number()?)),
            Some(c) if c.is_ascii_alphabetic() || c == b'_' => {
                let start = self.pos;
                match self.name().as_deref() {
                    Some("true") => Ok(Value::Bool(true)),
                    Some("false") => Ok(Value::Bool(false)),
                    Some("nil") => Ok(Value::Nil),
                    // The client writes infinities and NaN this way.
                    Some("inf") => Ok(Value::Num(f64::INFINITY)),
                    Some("nan") => Ok(Value::Num(f64::NAN)),
                    _ => {
                        self.pos = start;
                        Err(self.error("expected a value"))
                    }
                }
            }
            _ => Err(self.error("expected a value")),
        }
    }

    fn table(&mut self, depth: usize) -> Result<Table, ParseError> {
        self.expect(b'{')?;
        let mut table = Table::default();
        let mut next_index = 1.0;
        loop {
            self.skip_space()?;
            match self.peek() {
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(table);
                }
                None => return Err(self.error("unfinished table")),
                _ => {}
            }
            let key = if self.peek() == Some(b'[') && self.long_bracket_level().is_none() {
                self.pos += 1;
                let key = match self.value(depth)? {
                    Value::Str(s) => Key::Str(s),
                    Value::Num(n) if !n.is_nan() => Key::Num(n),
                    Value::Bool(b) => Key::Bool(b),
                    _ => return Err(self.error("a key can't be nil or NaN")),
                };
                self.skip_space()?;
                self.expect(b']')?;
                self.skip_space()?;
                self.expect(b'=')?;
                Some(key)
            } else if matches!(self.peek(), Some(c) if c.is_ascii_alphabetic() || c == b'_') {
                // `name = value`, or a bare true/false/nil in the array part.
                let start = self.pos;
                let name = self.name().unwrap_or_default();
                self.skip_space()?;
                if self.peek() == Some(b'=') {
                    self.pos += 1;
                    Some(Key::Str(name))
                } else {
                    self.pos = start;
                    None
                }
            } else {
                None
            };
            let value = self.value(depth)?;
            let key = key.unwrap_or_else(|| {
                let k = Key::Num(next_index);
                next_index += 1.0;
                k
            });
            table.entries.push((key, value));
            self.skip_space()?;
            match self.peek() {
                Some(b',' | b';') => self.pos += 1,
                Some(b'}') => {}
                _ => return Err(self.error("expected , or }")),
            }
        }
    }

    fn number(&mut self) -> Result<f64, ParseError> {
        let start = self.pos;
        if self.peek() == Some(b'0') && matches!(self.peek_at(1), Some(b'x' | b'X')) {
            self.pos += 2;
            let digits = self.pos;
            while matches!(self.peek(), Some(c) if c.is_ascii_hexdigit()) {
                self.pos += 1;
            }
            let hex = std::str::from_utf8(&self.src[digits..self.pos]).unwrap_or("");
            return u64::from_str_radix(hex, 16).map(|n| n as f64).map_err(|_| self.error("bad hex number"));
        }
        while let Some(c) = self.peek() {
            let exponent_sign =
                matches!(c, b'+' | b'-') && matches!(self.src.get(self.pos.wrapping_sub(1)), Some(b'e' | b'E'));
            if c.is_ascii_digit() || c == b'.' || c == b'e' || c == b'E' || exponent_sign {
                self.pos += 1;
            } else {
                break;
            }
        }
        std::str::from_utf8(&self.src[start..self.pos])
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .ok_or(ParseError { offset: start, message: "bad number" })
    }

    fn quoted(&mut self) -> Result<String, ParseError> {
        let quote = self.peek().unwrap_or(b'"');
        self.pos += 1;
        let mut out: Vec<u8> = Vec::new();
        loop {
            let c = self.peek().ok_or_else(|| self.error("unfinished string"))?;
            self.pos += 1;
            match c {
                b'\n' => return Err(self.error("line break inside a string")),
                b'\\' => {
                    let e = self.peek().ok_or_else(|| self.error("unfinished string"))?;
                    self.pos += 1;
                    match e {
                        b'n' | b'\n' => out.push(b'\n'),
                        b't' => out.push(b'\t'),
                        b'r' => out.push(b'\r'),
                        b'a' => out.push(0x07),
                        b'b' => out.push(0x08),
                        b'f' => out.push(0x0c),
                        b'v' => out.push(0x0b),
                        b'\\' | b'"' | b'\'' => out.push(e),
                        b'0'..=b'9' => {
                            let mut code = u32::from(e - b'0');
                            for _ in 0..2 {
                                match self.peek() {
                                    Some(d @ b'0'..=b'9') => {
                                        code = code * 10 + u32::from(d - b'0');
                                        self.pos += 1;
                                    }
                                    _ => break,
                                }
                            }
                            let byte = u8::try_from(code).map_err(|_| self.error("escape past 255"))?;
                            out.push(byte);
                        }
                        _ => return Err(self.error("unknown escape")),
                    }
                }
                c if c == quote => return Ok(String::from_utf8_lossy(&out).into_owned()),
                c => out.push(c),
            }
        }
    }

    /// At `[`: the number of `=` in an opening long bracket (`[[`, `[==[`), or None if this isn't one.
    fn long_bracket_level(&self) -> Option<usize> {
        let mut i = 1;
        while self.peek_at(i) == Some(b'=') {
            i += 1;
        }
        (self.peek_at(i) == Some(b'[')).then_some(i - 1)
    }

    fn long_string(&mut self) -> Result<String, ParseError> {
        let level = self.long_bracket_level().ok_or_else(|| self.error("expected a long bracket"))?;
        self.pos += level + 2;
        // A line break right after the opening bracket isn't part of the string.
        if self.peek() == Some(b'\r') {
            self.pos += 1;
        }
        if self.peek() == Some(b'\n') {
            self.pos += 1;
        }
        let start = self.pos;
        let mut close = Vec::with_capacity(level + 2);
        close.push(b']');
        close.extend(std::iter::repeat_n(b'=', level));
        close.push(b']');
        while self.pos + close.len() <= self.src.len() {
            if self.src[self.pos..].starts_with(&close) {
                let text = String::from_utf8_lossy(&self.src[start..self.pos]).into_owned();
                self.pos += close.len();
                return Ok(text);
            }
            self.pos += 1;
        }
        self.pos = self.src.len();
        Err(self.error("unfinished long string"))
    }
}

/// A Lua string literal for `text`, safe for any bytes.
pub fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => out.push_str(&format!("\\{}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(src: &str) -> Value {
        let globals = parse(src).expect("parses");
        assert_eq!(globals.len(), 1);
        globals.into_iter().next().map(|(_, v)| v).unwrap_or(Value::Nil)
    }

    #[test]
    fn reads_a_file_as_the_client_writes_it() {
        let src = "\u{feff}\nLFGPlusDB = {\n[\"records\"] = {\n{\n[\"text\"] = \"LFGR1:abc\",\n[\"seq\"] = 3,\n[\"proven\"] = true,\n}, -- [1]\n},\n[\"lastSeq\"] = 12,\n[\"name\"] = \"Th\\195\\169a \\\"Q\\\"\",\n}\nLFGPlusLog = {\n}\n";
        let globals = parse(src).expect("parses");
        let db = global(&globals, "LFGPlusDB").and_then(Value::as_table).expect("table");
        assert_eq!(db.get("lastSeq").and_then(Value::as_u64), Some(12));
        assert_eq!(db.get("name").and_then(Value::as_str), Some("Théa \"Q\""));
        let records = db.get("records").and_then(Value::as_table).expect("records").list();
        assert_eq!(records.len(), 1);
        let first = records[0].as_table().expect("record");
        assert_eq!(first.get("text").and_then(Value::as_str), Some("LFGR1:abc"));
        assert_eq!(first.get("proven"), Some(&Value::Bool(true)));
        assert!(global(&globals, "LFGPlusLog").is_some());
        assert!(global(&globals, "Missing").is_none());
    }

    #[test]
    fn reads_every_kind_of_value() {
        assert_eq!(one("x = nil"), Value::Nil);
        assert_eq!(one("x = false"), Value::Bool(false));
        assert_eq!(one("x = -2.5e3"), Value::Num(-2500.0));
        assert_eq!(one("x = 1E+2"), Value::Num(100.0));
        assert_eq!(one("x = 0x1F"), Value::Num(31.0));
        assert_eq!(one("x = .5"), Value::Num(0.5));
        assert_eq!(one("x = inf"), Value::Num(f64::INFINITY));
        assert!(matches!(one("x = nan"), Value::Num(n) if n.is_nan()));
        assert_eq!(one("x = 'a\\tb\\r\\a\\b\\f\\v\\\\\\'\\65\\\n'"), Value::Str("a\tb\r\x07\x08\x0c\x0b\\'A\n".into()));
        assert_eq!(one("x = [[\nlong ]] "), Value::Str("long ".into()));
        assert_eq!(one("x = [==[\r\na]]b]==]"), Value::Str("a]]b".into()));
        assert_eq!(one("x = 1;"), Value::Num(1.0));
    }

    #[test]
    fn reads_every_kind_of_key() {
        let t = one("x = { a = 1, [true] = 2, [2.5] = 3, 'p', true, nil, ['s'] = 4; [1] = 'q' }");
        let t = t.as_table().expect("table");
        assert_eq!(t.get("a"), Some(&Value::Num(1.0)));
        assert_eq!(t.get("s"), Some(&Value::Num(4.0)));
        assert!(t.entries.contains(&(Key::Bool(true), Value::Num(2.0))));
        assert!(t.entries.contains(&(Key::Num(2.5), Value::Num(3.0))));
        // The later [1] wins, as in Lua; the array stops at the nil.
        assert_eq!(t.list(), vec![&Value::Str("q".into()), &Value::Bool(true)]);
    }

    #[test]
    fn skips_comments() {
        assert_eq!(one("-- a\n--[[ b\n]] x --[==[ c ]==] = -- d\n 7 -- e"), Value::Num(7.0));
    }

    #[test]
    fn non_values_read_as_none() {
        assert_eq!(Value::Nil.as_table(), None);
        assert_eq!(Value::Nil.as_str(), None);
        assert_eq!(Value::Num(-1.0).as_u64(), None);
        assert_eq!(Value::Num(1.5).as_u64(), None);
        assert_eq!(Value::Num(f64::INFINITY).as_u64(), None);
        assert_eq!(Value::Str("1".into()).as_u64(), None);
    }

    #[test]
    fn refuses_what_isnt_saved_variables() {
        for bad in [
            "x",
            "x =",
            "= 1",
            "1 = 2",
            "x = y",
            "x = {",
            "x = { 1 2 }",
            "x = \"a",
            "x = \"a\nb\"",
            "x = '\\q'",
            "x = '\\300'",
            "x = '\\",
            "x = -'a'",
            "x = 0xZ",
            "x = 1e",
            "x = [[a",
            "x = { [nil] = 1 }",
            "x = { [nan] = 1 }",
            "x = { [1 = 2 }",
            "x = { [1] 2 }",
            "x = $",
            "x = 'a",
        ] {
            assert!(parse(bad).is_err(), "{bad:?} should fail");
        }
        let deep = format!("x = {}{}", "{".repeat(MAX_DEPTH + 1), "}".repeat(MAX_DEPTH + 1));
        assert_eq!(parse(&deep).map_err(|e| e.message), Err("tables nested too deeply"));
        let err = parse("x = $").expect_err("fails");
        assert_eq!(err.to_string(), "expected a value at byte 4");
    }

    #[test]
    fn quote_round_trips() {
        let text = "a\"b\\c\nd\re\u{1}f\u{7f}é";
        assert_eq!(one(&format!("x = {}", quote(text))), Value::Str(text.into()));
    }

    mod properties {
        use super::super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn never_panics_on_any_input(src in ".{0,400}") {
                let _ = parse(&src);
            }

            #[test]
            fn never_panics_on_lua_shaped_input(src in "[{}\\[\\]=,;\"'\\\\a-z0-9 .\\-\n]{0,200}") {
                let _ = parse(&src);
            }

            #[test]
            fn quoted_strings_round_trip(text in any::<String>()) {
                let globals = parse(&format!("x = {}", quote(&text))).expect("parses");
                prop_assert_eq!(global(&globals, "x"), Some(&Value::Str(text)));
            }

            #[test]
            fn whole_numbers_round_trip(n in 0u64..(1u64 << 53)) {
                let globals = parse(&format!("x = {n}")).expect("parses");
                prop_assert_eq!(global(&globals, "x").and_then(Value::as_u64), Some(n));
            }
        }
    }
}
