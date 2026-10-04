//! Just enough JSON to read `op`'s output, because the crate takes no dependencies.
//! `op item get -` prints one pretty-printed object per item, back to back, so the entry point reads a stream of values rather than one document.

#[derive(Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Num(String),
    Str(String),
    Arr(Vec<Value>),
    Obj(Vec<(String, Value)>),
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Obj(members) => members.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_arr(&self) -> &[Value] {
        match self {
            Value::Arr(items) => items,
            _ => &[],
        }
    }
}

/// Every top-level value in `text`, in order.
pub fn parse_stream(text: &str) -> Result<Vec<Value>, String> {
    let mut p = Parser {
        s: text.as_bytes(),
        i: 0,
    };
    let mut out = Vec::new();
    loop {
        p.ws();
        if p.i == p.s.len() {
            return Ok(out);
        }
        out.push(p.value()?);
    }
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.s.get(self.i).is_some_and(u8::is_ascii_whitespace) {
            self.i += 1;
        }
    }

    fn err<T>(&self, what: &str) -> Result<T, String> {
        Err(format!("json: {what} at byte {}", self.i))
    }

    fn eat(&mut self, b: u8) -> Result<(), String> {
        self.ws();
        if self.s.get(self.i) == Some(&b) {
            self.i += 1;
            Ok(())
        } else {
            self.err(&format!("expected '{}'", b as char))
        }
    }

    fn value(&mut self) -> Result<Value, String> {
        self.ws();
        match self.s.get(self.i) {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => self.string().map(Value::Str),
            Some(b't') => self.word("true", Value::Bool(true)),
            Some(b'f') => self.word("false", Value::Bool(false)),
            Some(b'n') => self.word("null", Value::Null),
            Some(b'-' | b'0'..=b'9') => Ok(self.number()),
            _ => self.err("unexpected input"),
        }
    }

    fn word(&mut self, w: &str, v: Value) -> Result<Value, String> {
        if self.s[self.i..].starts_with(w.as_bytes()) {
            self.i += w.len();
            Ok(v)
        } else {
            self.err("bad literal")
        }
    }

    fn number(&mut self) -> Value {
        let start = self.i;
        while self
            .s
            .get(self.i)
            .is_some_and(|b| matches!(b, b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9'))
        {
            self.i += 1;
        }
        Value::Num(String::from_utf8_lossy(&self.s[start..self.i]).into_owned())
    }

    fn object(&mut self) -> Result<Value, String> {
        self.eat(b'{')?;
        let mut members = Vec::new();
        self.ws();
        if self.s.get(self.i) == Some(&b'}') {
            self.i += 1;
            return Ok(Value::Obj(members));
        }
        loop {
            self.ws();
            let key = self.string()?;
            self.eat(b':')?;
            members.push((key, self.value()?));
            self.ws();
            match self.s.get(self.i) {
                Some(b',') => self.i += 1,
                Some(b'}') => {
                    self.i += 1;
                    return Ok(Value::Obj(members));
                }
                _ => return self.err("expected ',' or '}'"),
            }
        }
    }

    fn array(&mut self) -> Result<Value, String> {
        self.eat(b'[')?;
        let mut items = Vec::new();
        self.ws();
        if self.s.get(self.i) == Some(&b']') {
            self.i += 1;
            return Ok(Value::Arr(items));
        }
        loop {
            items.push(self.value()?);
            self.ws();
            match self.s.get(self.i) {
                Some(b',') => self.i += 1,
                Some(b']') => {
                    self.i += 1;
                    return Ok(Value::Arr(items));
                }
                _ => return self.err("expected ',' or ']'"),
            }
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.eat(b'"')?;
        let mut out = Vec::new();
        loop {
            let Some(&b) = self.s.get(self.i) else {
                return self.err("unterminated string");
            };
            self.i += 1;
            match b {
                b'"' => return String::from_utf8(out).or_else(|_| self.err("invalid utf-8")),
                b'\\' => {
                    let Some(&e) = self.s.get(self.i) else {
                        return self.err("unterminated escape");
                    };
                    self.i += 1;
                    match e {
                        b'n' => out.push(b'\n'),
                        b't' => out.push(b'\t'),
                        b'r' => out.push(b'\r'),
                        b'b' => out.push(0x08),
                        b'f' => out.push(0x0c),
                        b'u' => {
                            let c = self.unicode()?;
                            out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
                        }
                        other => out.push(other),
                    }
                }
                _ => out.push(b),
            }
        }
    }

    /// The four hex digits after `\u`, joining a surrogate pair when one follows.
    fn unicode(&mut self) -> Result<char, String> {
        let hi = self.hex4()?;
        let code = if (0xD800..0xDC00).contains(&hi) && self.s[self.i..].starts_with(b"\\u") {
            self.i += 2;
            let lo = self.hex4()?;
            0x10000 + ((hi - 0xD800) << 10) + (lo.wrapping_sub(0xDC00) & 0x3FF)
        } else {
            hi
        };
        Ok(char::from_u32(code).unwrap_or('\u{FFFD}'))
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let digits = self
            .s
            .get(self.i..self.i + 4)
            .and_then(|d| std::str::from_utf8(d).ok());
        let Some(n) = digits.and_then(|d| u32::from_str_radix(d, 16).ok()) else {
            return self.err("bad \\u escape");
        };
        self.i += 4;
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::{Value, parse_stream};

    #[test]
    fn back_to_back_objects_are_separate_values() {
        let v = parse_stream("{\"a\": 1}\n{\"a\": [true, null]}\n").unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].get("a"), Some(&Value::Num("1".into())));
        assert_eq!(v[1].get("a").unwrap().as_arr().len(), 2);
    }

    #[test]
    fn escapes_decode() {
        let v = parse_stream(r#""a\"b\\c\né😀""#).unwrap();
        assert_eq!(v[0].as_str(), Some("a\"b\\c\né😀"));
    }

    #[test]
    fn empty_input_is_no_values() {
        assert_eq!(parse_stream("  \n").unwrap(), Vec::<Value>::new());
    }

    #[test]
    fn garbage_is_an_error() {
        assert!(parse_stream("{\"a\" 1}").is_err());
        assert!(parse_stream("\"open").is_err());
    }
}
