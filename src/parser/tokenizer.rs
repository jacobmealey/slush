pub mod tokenizer {
    enum ShellTokens {
        NewLine,
        WhiteSpace,
        EndOfFile,      // EOF
        BackSlash,      // \
        DollarSign,     // $
        BackTick,       // `
        DoubleQuote,    // "
        SingleQuote,    // '
        LeftParen,      // (
        RightParen,     // )
        LeftBracket,    // [
        RightBracket,   // ]
        DoubleLeftBracket,    // [[
        DoubleRightBracket,   // ]]
        LeftBrace,      // {
        RightBrace,     // }
        Bang,           // !
        AtSign,         // @
        Star,           // *
        Pound,          // #
        QuestionMark,   // ?
        Tilde,          // ~
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
        Time
    }
    pub fn tokens(st: String) -> Vec<String> {
        let mut lexemes = Vec::new(); 
        let mut current = String::new();
        for c in st.chars() {
            if c.is_whitespace() {
                lexemes.push(current);
                current = "".to_string();
                continue;
            }
            current.push(c);
        }
        if current.len() > 0 {
            lexemes.push(current);
        }
        lexemes
    }
}
