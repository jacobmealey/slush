use std::collections::{HashMap, HashSet};
use std::iter::Peekable;
use std::str::Chars;
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShTokenType {
    NewLine,
    WhiteSpace,
    EndOfFile,          // EOF
    BackSlash,          // \
    DollarSign,         // $
    LeftParen,          // (
    RightParen,         // )
    LeftBracket,        // [
    RightBracket,       // ]
    DoubleLeftBracket,  // [[
    DoubleRightBracket, // ]]
    LeftBrace,          // {
    RightBrace,         // }
    Bang,               // !
    AtSign,             // @
    Star,               // *
    Pound,              // #
    QuestionMark,       // ?
    Tilde,              // ~
    Pipe,               // |
    Control,            // &
    RedirectOut,        // >
    RedirectIn,         // <
    AppendOut,          // >>
    AndIf,              // &&
    OrIf,               // ||
    Equal,              // =
    SemiColon,          // ;
    Case,
    Do,
    Done,
    Elif,
    Else,
    Esac,
    Fi,
    For,
    If,
    In,
    Then,
    Until,
    While,
    Function,
    NameSpace,
    Select,
    Time,
    Name,
    DoubleQuoteStr,
    BackTickStr,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub lexeme: String,
    pub token_type: ShTokenType,
}

pub fn is_delemiter(c: char) -> bool {
    let delimeter_set = HashSet::from([
        ' ', '\t', '$', '\\', '\'', '(', ')', '{', '}', '[', ']', '!', '@', '*', '#', '?', '~',
        '|', '>', '<', '`', '"', '&', '=', '\n', ';',
    ]);
    delimeter_set.contains(&c)
}

pub fn tokens(st: &str) -> Result<Vec<Token>, String> {
    let mut tokens: Vec<Token> = Vec::new();
    let mut current;
    let token_map: HashMap<&str, ShTokenType> = HashMap::from([
        ("case", ShTokenType::Case),
        ("do", ShTokenType::Do),
        ("done", ShTokenType::Done),
        ("elif", ShTokenType::Elif),
        ("else", ShTokenType::Else),
        ("esac", ShTokenType::Esac),
        ("fi", ShTokenType::Fi),
        ("for", ShTokenType::For),
        ("if", ShTokenType::If),
        ("in", ShTokenType::In),
        ("then", ShTokenType::Then),
        ("until", ShTokenType::Until),
        ("while", ShTokenType::While),
        ("function", ShTokenType::Function),
        ("namespace", ShTokenType::NameSpace),
        ("select", ShTokenType::Select),
        ("time", ShTokenType::Time),
    ]);

    let match_token = |current: String| -> Token {
        match token_map.get(&current.as_str()) {
            Some(&toke_type) => Token {
                lexeme: current.clone(),
                token_type: toke_type,
            },
            None => Token {
                lexeme: current.clone(),
                token_type: ShTokenType::Name,
            },
        }
    };

    let scan_until = |token: char, it: &mut Peekable<Chars<'_>>| -> Result<String, String> {
        let mut ret = String::new();
        while it.peek() != Some(&token) && it.peek().is_some() {
            ret.push(it.next().unwrap());
        }
        if it.peek().is_none() {
            return Err(format!("Couldn't find second '{}'.", token));
        }
        it.next();
        Ok(ret)
    };
    let mut it = st.chars().peekable();
    while let Some(c) = it.next() {
        let token = match c {
            '\n' => Token {
                lexeme: String::from(c),
                token_type: ShTokenType::NewLine,
            },
            ' ' => Token {
                lexeme: String::from(c),
                token_type: ShTokenType::WhiteSpace,
            },
            '\\' => Token {
                lexeme: String::from(c),
                token_type: ShTokenType::BackSlash,
            },
            '$' => Token {
                lexeme: String::from(c),
                token_type: ShTokenType::DollarSign,
            },
            '`' => Token {
                lexeme: scan_until('`', &mut it)?,
                token_type: ShTokenType::BackTickStr,
            },
            '"' => Token {
                lexeme: scan_until('"', &mut it)?,
                token_type: ShTokenType::DoubleQuoteStr,
            },
            '\'' => Token {
                lexeme: scan_until('\'', &mut it)?,
                token_type: ShTokenType::Name,
            },
            '(' => Token {
                lexeme: String::from(c),
                token_type: ShTokenType::LeftParen,
            },
            ')' => Token {
                lexeme: String::from(c),
                token_type: ShTokenType::RightParen,
            },
            '{' => Token {
                lexeme: String::from(c),
                token_type: ShTokenType::LeftBrace,
            },
            '}' => Token {
                lexeme: String::from(c),
                token_type: ShTokenType::RightBrace,
            },
            '!' => Token {
                lexeme: String::from(c),
                token_type: ShTokenType::Bang,
            },
            '@' => Token {
                lexeme: String::from(c),
                token_type: ShTokenType::AtSign,
            },
            '*' => Token {
                lexeme: String::from(c),
                token_type: ShTokenType::Star,
            },
            '#' => Token {
                lexeme: String::from(c),
                token_type: ShTokenType::Pound,
            },
            '?' => Token {
                lexeme: String::from(c),
                token_type: ShTokenType::QuestionMark,
            },
            '~' => Token {
                lexeme: String::from(c),
                token_type: ShTokenType::Tilde,
            },
            '=' => Token {
                lexeme: String::from(c),
                token_type: ShTokenType::Equal,
            },
            ';' => Token {
                lexeme: String::from(c),
                token_type: ShTokenType::SemiColon,
            },
            '[' => {
                let tok: Token;
                if it.peek().is_some() && *it.peek().expect("No char?") == '[' {
                    tok = Token {
                        lexeme: String::from("[["),
                        token_type: ShTokenType::DoubleLeftBracket,
                    };
                    it.next();
                } else {
                    tok = Token {
                        lexeme: String::from(c),
                        token_type: ShTokenType::LeftBracket,
                    };
                }
                tok
            }
            ']' => {
                let tok: Token;
                if it.peek().is_some() && *it.peek().expect("No Char?") == ']' {
                    tok = Token {
                        lexeme: String::from("]]"),
                        token_type: ShTokenType::DoubleRightBracket,
                    };
                    it.next();
                } else {
                    tok = Token {
                        lexeme: String::from(c),
                        token_type: ShTokenType::RightBracket,
                    }
                }
                tok
            }
            '&' => {
                let tok: Token;
                if it.peek().is_some() && *it.peek().unwrap() == '&' {
                    tok = Token {
                        lexeme: String::from("&&"),
                        token_type: ShTokenType::AndIf,
                    };
                    it.next();
                } else {
                    tok = Token {
                        lexeme: String::from(c),
                        token_type: ShTokenType::Control,
                    }
                }
                tok
            }
            '|' => {
                let tok: Token;
                if it.peek().is_some() && *it.peek().unwrap() == '|' {
                    tok = Token {
                        lexeme: String::from("||"),
                        token_type: ShTokenType::OrIf,
                    };
                    it.next();
                } else {
                    tok = Token {
                        lexeme: String::from(c),
                        token_type: ShTokenType::Pipe,
                    }
                }
                tok
            }
            '>' => {
                let tok: Token;
                if it.peek().is_some() && *it.peek().unwrap() == '>' {
                    tok = Token {
                        lexeme: String::from(">>"),
                        token_type: ShTokenType::AppendOut,
                    };
                    it.next();
                } else {
                    tok = Token {
                        lexeme: String::from(c),
                        token_type: ShTokenType::RedirectOut,
                    }
                }
                tok
            }
            '<' => Token {
                lexeme: String::from(c),
                token_type: ShTokenType::RedirectIn,
            },
            _ => {
                current = String::from(c);
                while let Some(cc) = it.peek() {
                    if !cc.is_whitespace() && !is_delemiter(*cc) {
                        current.push(*cc);
                        it.next();
                    } else {
                        break;
                    }
                }
                match_token(current)
            }
        };

        // this works fines for single quoted strings, but i don't like the idea
        // of the 'tokenizer' having to call executable code and stuff....
        if tokens.last().is_some()
            && tokens.last().unwrap().token_type == ShTokenType::Name
            && token.token_type == ShTokenType::Name
        {
            let last = tokens.pop().unwrap();
            tokens.push(Token {
                lexeme: last.lexeme + token.lexeme.as_str(),
                token_type: ShTokenType::Name,
            });
        } else {
            tokens.push(token);
        }
    }
    Ok(tokens)
}

mod tests {
    #[allow(unused_imports)]
    use super::*;
    #[test]
    fn basic_tokens() {
        let reference_string = "| || > >> < [ [[ ] ]] &&&~${}@*";
        let toks = tokens(reference_string).unwrap();
        let good_graces = [
            Token {
                lexeme: String::from("|"),
                token_type: ShTokenType::Pipe,
            },
            Token {
                lexeme: String::from(" "),
                token_type: ShTokenType::WhiteSpace,
            },
            Token {
                lexeme: String::from("||"),
                token_type: ShTokenType::OrIf,
            },
            Token {
                lexeme: String::from(" "),
                token_type: ShTokenType::WhiteSpace,
            },
            Token {
                lexeme: String::from(">"),
                token_type: ShTokenType::RedirectOut,
            },
            Token {
                lexeme: String::from(" "),
                token_type: ShTokenType::WhiteSpace,
            },
            Token {
                lexeme: String::from(">>"),
                token_type: ShTokenType::AppendOut,
            },
            Token {
                lexeme: String::from(" "),
                token_type: ShTokenType::WhiteSpace,
            },
            Token {
                lexeme: String::from("<"),
                token_type: ShTokenType::RedirectIn,
            },
            Token {
                lexeme: String::from(" "),
                token_type: ShTokenType::WhiteSpace,
            },
            Token {
                lexeme: String::from("["),
                token_type: ShTokenType::LeftBracket,
            },
            Token {
                lexeme: String::from(" "),
                token_type: ShTokenType::WhiteSpace,
            },
            Token {
                lexeme: String::from("[["),
                token_type: ShTokenType::DoubleLeftBracket,
            },
            Token {
                lexeme: String::from(" "),
                token_type: ShTokenType::WhiteSpace,
            },
            Token {
                lexeme: String::from("]"),
                token_type: ShTokenType::RightBracket,
            },
            Token {
                lexeme: String::from(" "),
                token_type: ShTokenType::WhiteSpace,
            },
            Token {
                lexeme: String::from("]]"),
                token_type: ShTokenType::DoubleRightBracket,
            },
            Token {
                lexeme: String::from(" "),
                token_type: ShTokenType::WhiteSpace,
            },
            Token {
                lexeme: String::from("&&"),
                token_type: ShTokenType::AndIf,
            },
            Token {
                lexeme: String::from("&"),
                token_type: ShTokenType::Control,
            },
            Token {
                lexeme: String::from("~"),
                token_type: ShTokenType::Tilde,
            },
            Token {
                lexeme: String::from("$"),
                token_type: ShTokenType::DollarSign,
            },
            Token {
                lexeme: String::from("{"),
                token_type: ShTokenType::LeftBrace,
            },
            Token {
                lexeme: String::from("}"),
                token_type: ShTokenType::RightBrace,
            },
            Token {
                lexeme: String::from("@"),
                token_type: ShTokenType::AtSign,
            },
            Token {
                lexeme: String::from("*"),
                token_type: ShTokenType::Star,
            },
        ];
        assert!(good_graces.iter().eq(toks.iter()));
    }

    #[test]
    fn wordy_tokes() {
        let reference_string = "if elif else fi while";
        let toks = tokens(reference_string).unwrap();
        let good_graces = [
            Token {
                lexeme: String::from("if"),
                token_type: ShTokenType::If,
            },
            Token {
                lexeme: String::from(" "),
                token_type: ShTokenType::WhiteSpace,
            },
            Token {
                lexeme: String::from("elif"),
                token_type: ShTokenType::Elif,
            },
            Token {
                lexeme: String::from(" "),
                token_type: ShTokenType::WhiteSpace,
            },
            Token {
                lexeme: String::from("else"),
                token_type: ShTokenType::Else,
            },
            Token {
                lexeme: String::from(" "),
                token_type: ShTokenType::WhiteSpace,
            },
            Token {
                lexeme: String::from("fi"),
                token_type: ShTokenType::Fi,
            },
            Token {
                lexeme: String::from(" "),
                token_type: ShTokenType::WhiteSpace,
            },
            Token {
                lexeme: String::from("while"),
                token_type: ShTokenType::While,
            },
        ];
        assert!(good_graces.iter().eq(toks.iter()));
    }

    #[test]
    fn words_adjacent_to_singles() {
        let reference_string = "if|while{ elif";
        let toks = tokens(reference_string).unwrap();
        let good_graces = [
            Token {
                lexeme: String::from("if"),
                token_type: ShTokenType::If,
            },
            Token {
                lexeme: String::from("|"),
                token_type: ShTokenType::Pipe,
            },
            Token {
                lexeme: String::from("while"),
                token_type: ShTokenType::While,
            },
            Token {
                lexeme: String::from("{"),
                token_type: ShTokenType::LeftBrace,
            },
            Token {
                lexeme: String::from(" "),
                token_type: ShTokenType::WhiteSpace,
            },
            Token {
                lexeme: String::from("elif"),
                token_type: ShTokenType::Elif,
            },
        ];
        assert!(good_graces.iter().eq(toks.iter()));
    }
}
