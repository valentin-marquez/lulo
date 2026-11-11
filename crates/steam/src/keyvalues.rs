//! Valve KeyValues in text form (`"key" "value"` / `"key" { ... }`), as appinfo uses it.
//! Keys compare case-insensitively, like Valve's own parser.

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Str(String),
    Map(Map),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Map(Vec<(String, Value)>);

impl Map {
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v)
    }

    pub fn map(&self, key: &str) -> Option<&Map> {
        match self.get(key)? {
            Value::Map(map) => Some(map),
            Value::Str(_) => None,
        }
    }

    pub fn str(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            Value::Str(s) => Some(s),
            Value::Map(_) => None,
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.0.iter().map(|(k, v)| (k.as_str(), v))
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ParseError {
    #[error("unexpected end of input")]
    Eof,
    #[error("unexpected '{0}'")]
    Unexpected(char),
}

#[derive(Debug, PartialEq)]
enum Token {
    Str(String),
    Open,
    Close,
}

fn tokens(text: &str) -> Result<Vec<Token>, ParseError> {
    let mut out = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => out.push(Token::Open),
            '}' => out.push(Token::Close),
            '"' => {
                let mut s = String::new();
                loop {
                    match chars.next().ok_or(ParseError::Eof)? {
                        '"' => break,
                        '\\' => match chars.next().ok_or(ParseError::Eof)? {
                            'n' => s.push('\n'),
                            't' => s.push('\t'),
                            other => s.push(other),
                        },
                        other => s.push(other),
                    }
                }
                out.push(Token::Str(s));
            }
            '/' if chars.peek() == Some(&'/') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            c if c.is_whitespace() || c == '\0' => {}
            other => return Err(ParseError::Unexpected(other)),
        }
    }
    Ok(out)
}

pub fn parse(text: &str) -> Result<Map, ParseError> {
    let tokens = tokens(text)?;
    let mut at = 0;
    let map = block(&tokens, &mut at, false)?;
    Ok(map)
}

fn block(tokens: &[Token], at: &mut usize, nested: bool) -> Result<Map, ParseError> {
    let mut entries = Vec::new();
    loop {
        let key = match tokens.get(*at) {
            None if !nested => return Ok(Map(entries)),
            None => return Err(ParseError::Eof),
            Some(Token::Close) if nested => {
                *at += 1;
                return Ok(Map(entries));
            }
            Some(Token::Str(key)) => key.clone(),
            Some(Token::Open) => return Err(ParseError::Unexpected('{')),
            Some(Token::Close) => return Err(ParseError::Unexpected('}')),
        };
        *at += 1;
        let value = match tokens.get(*at).ok_or(ParseError::Eof)? {
            Token::Str(s) => {
                *at += 1;
                Value::Str(s.clone())
            }
            Token::Open => {
                *at += 1;
                Value::Map(block(tokens, at, true)?)
            }
            Token::Close => return Err(ParseError::Unexpected('}')),
        };
        entries.push((key, value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_blocks_and_escapes() {
        let text = r#"
            "appinfo"
            {
                "appid"  "108600" // trailing comment
                "common" { "name" "Project \"Zomboid\"" }
                "Depots" { "branches" { "public" { "buildid" "26486631" } } }
            }
        "#;
        let root = parse(text).unwrap();
        let info = root.map("appinfo").unwrap();
        assert_eq!(info.str("appid"), Some("108600"));
        assert_eq!(
            info.map("common").unwrap().str("name"),
            Some("Project \"Zomboid\"")
        );
        // Keys are case-insensitive.
        let public = info
            .map("depots")
            .unwrap()
            .map("branches")
            .unwrap()
            .map("PUBLIC")
            .unwrap();
        assert_eq!(public.str("buildid"), Some("26486631"));
    }

    #[test]
    fn rejects_broken_input() {
        assert_eq!(parse(r#""a" { "b" "c""#), Err(ParseError::Eof));
        assert_eq!(parse(r#""a" }"#), Err(ParseError::Unexpected('}')));
        assert_eq!(parse("a"), Err(ParseError::Unexpected('a')));
    }
}
