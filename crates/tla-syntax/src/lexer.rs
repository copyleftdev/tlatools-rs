use crate::error::{Error, Result};
use crate::token::{self, Kw, Op, Tok, Token};

pub fn lex(src: &str) -> Result<Vec<Token>> {
    Lexer::new(src).run()
}

struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: u32,
    col: u32,
    out: Vec<Token>,
}

/// Where the module actually begins.
///
/// A `.tla` file is a module surrounded by prose: an explanation above the
/// header, notes and shell transcripts below the terminator. That text is not
/// TLA+ and must not be lexed as it — plenty of it contains `$`, `&` and `/`,
/// which are not characters the language allows loose.
fn module_start(src: &str) -> (usize, u32) {
    let mut offset = 0;
    if src.starts_with('\u{feff}') {
        offset = '\u{feff}'.len_utf8();
    }
    for (index, text) in src[offset..].split_inclusive('\n').enumerate() {
        let trimmed = text.trim_start();
        if trimmed.starts_with("----")
            && trimmed
                .trim_start_matches('-')
                .trim_start()
                .starts_with("MODULE")
        {
            return (offset, u32::try_from(index).unwrap_or(u32::MAX) + 1);
        }
        offset += text.len();
    }
    (0, 1)
}

impl Lexer {
    fn new(src: &str) -> Self {
        let (offset, line) = module_start(src);
        Self {
            chars: src[offset..].chars().collect(),
            pos: 0,
            line,
            col: 1,
            out: Vec::new(),
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.chars.get(self.pos).copied()?;
        self.pos += 1;
        // `\r\n` is one line break, not two, so a carriage return only ends a
        // line when no newline follows it.
        let ends_line = c == '\n' || (c == '\r' && self.peek() != Some('\n'));
        if ends_line {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn advance(&mut self, n: usize) {
        for _ in 0..n {
            self.bump();
        }
    }

    fn starts_with(&self, s: &str) -> bool {
        s.chars().enumerate().all(|(i, c)| self.at(i) == Some(c))
    }

    fn run_of(&self, c: char) -> usize {
        let mut n = 0;
        while self.at(n) == Some(c) {
            n += 1;
        }
        n
    }

    fn err(&self, msg: impl Into<String>) -> Error {
        Error::lex(msg, self.line, self.col)
    }

    fn run(mut self) -> Result<Vec<Token>> {
        // Modules nest, so the file ends at the terminator that closes the
        // outermost one, not at the first one seen.
        let mut depth = 0usize;
        loop {
            self.skip_trivia()?;
            let (line, col) = (self.line, self.col);
            let Some(c) = self.peek() else { break };
            let tok = self.scan(c)?;
            match tok {
                Tok::Kw(Kw::Module) => depth += 1,
                Tok::ModuleEnd => depth = depth.saturating_sub(1),
                _ => {}
            }
            let closed = matches!(tok, Tok::ModuleEnd) && depth == 0;
            self.out.push(Token { tok, line, col });
            if closed {
                break;
            }
        }
        self.out.push(Token {
            tok: Tok::Eof,
            line: self.line,
            col: self.col,
        });
        Ok(self.out)
    }

    fn skip_trivia(&mut self) -> Result<()> {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => {
                    self.bump();
                }
                Some('\\') if self.at(1) == Some('*') => {
                    while !matches!(self.peek(), None | Some('\n')) {
                        self.bump();
                    }
                }
                Some('(') if self.at(1) == Some('*') => self.skip_block_comment()?,
                _ => return Ok(()),
            }
        }
    }

    fn skip_block_comment(&mut self) -> Result<()> {
        let (line, col) = (self.line, self.col);
        let mut depth = 0usize;
        loop {
            if self.peek().is_none() {
                return Err(Error::lex("unterminated (* comment", line, col));
            }
            if self.starts_with("(*") {
                depth += 1;
                self.advance(2);
            } else if self.starts_with("*)") {
                depth -= 1;
                self.advance(2);
                if depth == 0 {
                    return Ok(());
                }
            } else {
                self.bump();
            }
        }
    }

    fn scan(&mut self, c: char) -> Result<Tok> {
        if c.is_ascii_digit() {
            return self.scan_number();
        }
        // A lone `_` is the placeholder in `Op(_, _)`; followed by more, it is
        // an ordinary identifier, and `]_vars` is disambiguated by the parser.
        if c.is_ascii_alphabetic()
            || (c == '_'
                && self
                    .at(1)
                    .is_some_and(|n| n.is_ascii_alphanumeric() || n == '_'))
        {
            return Ok(self.scan_word());
        }
        match c {
            '"' => self.scan_string(),
            '=' => self.scan_equals(),
            '-' => Ok(self.scan_dashes()),
            '<' => Ok(self.scan_lt()),
            '>' => Ok(self.scan_gt()),
            '\\' => self.scan_backslash(),
            '/' => self.scan_slash(),
            '|' => {
                if self.starts_with("|->") {
                    self.advance(3);
                    return Ok(Tok::MapsTo);
                }
                self.user_symbol()
                    .map_or_else(|| Err(self.err("stray `|`")), Ok)
            }
            ':' => {
                if self.starts_with(":>") {
                    self.advance(2);
                    return Ok(Tok::Op(Op::OneTo));
                }
                if let Some(tok) = self.user_symbol() {
                    return Ok(tok);
                }
                if self.starts_with("::") {
                    self.advance(2);
                    return Ok(Tok::ColonColon);
                }
                self.bump();
                Ok(Tok::Colon)
            }
            '@' => {
                if self.starts_with("@@") {
                    self.advance(2);
                    Ok(Tok::Op(Op::AtAt))
                } else {
                    self.bump();
                    Ok(Tok::At)
                }
            }
            '~' => {
                if self.starts_with("~>") {
                    self.advance(2);
                    Ok(Tok::Op(Op::LeadsTo))
                } else {
                    self.bump();
                    Ok(Tok::Op(Op::Not))
                }
            }
            '&' | '$' | '?' | '%' | '#' | '!' | '^' | '(' | '+' | '*' => self
                .user_symbol()
                .map_or_else(|| self.scan_punctuation(c), Ok),
            '.' => {
                if self.starts_with("..") {
                    self.advance(2);
                    Ok(Tok::Op(Op::DotDot))
                } else {
                    self.bump();
                    Ok(Tok::Dot)
                }
            }
            '[' => {
                if self.starts_with("[]") {
                    self.advance(2);
                    Ok(Tok::Op(Op::Always))
                } else {
                    self.bump();
                    Ok(Tok::LBrack)
                }
            }
            _ => self.scan_punctuation(c),
        }
    }

    fn scan_punctuation(&mut self, c: char) -> Result<Tok> {
        let at = (self.line, self.col);
        self.bump();
        Ok(match c {
            '(' => Tok::LParen,
            ')' => Tok::RParen,
            ']' => Tok::RBrack,
            '{' => Tok::LBrace,
            '}' => Tok::RBrace,
            ',' => Tok::Comma,
            '!' => Tok::Bang,
            '\'' => Tok::Prime,
            '_' => Tok::Underscore,
            '#' => Tok::Op(Op::Neq),
            '+' => Tok::Op(Op::Plus),
            '*' => Tok::Op(Op::Times),
            '%' => Tok::Op(Op::Mod),
            '^' => Tok::Op(Op::Pow),
            _ => {
                return Err(Error::lex(
                    format!("unexpected character {c:?}"),
                    at.0,
                    at.1,
                ));
            }
        })
    }

    /// The longest symbol from the language's user-definable operator table
    /// that starts here. The named `\`-operators are matched elsewhere.
    fn user_symbol(&mut self) -> Option<Tok> {
        let mut best: Option<&'static str> = None;
        for (symbol, _) in token::USER_OPERATORS {
            if symbol.starts_with('\\') || !self.starts_with(symbol) {
                continue;
            }
            if best.is_none_or(|found| symbol.len() > found.len()) {
                best = Some(symbol);
            }
        }
        let symbol = best?;
        self.advance(symbol.chars().count());
        Some(Tok::Op(Op::User(symbol)))
    }

    fn scan_number(&mut self) -> Result<Tok> {
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.bump();
        }
        // `1aMessage` is a name, not a number followed by one: TLA+ only asks
        // that an identifier contain a letter, not that it begin with one.
        if self
            .peek()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        {
            self.pos = start;
            return Ok(self.scan_word());
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        text.parse()
            .map(Tok::Num)
            .map_err(|_| self.err(format!("integer literal out of range: {text}")))
    }

    fn scan_word(&mut self) -> Tok {
        let start = self.pos;
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            self.bump();
        }
        let word: String = self.chars[start..self.pos].iter().collect();

        // `WF_vars` is one word to the lexer but an operator plus a subscript.
        // `WF_vars` is one word; `WF_<<a, b>>` is the same operator with a
        // subscript the lexer cannot see, so the name is left empty.
        for (prefix, strong) in [("WF_", false), ("SF_", true)] {
            if let Some(rest) = word.strip_prefix(prefix) {
                return Tok::Fair {
                    strong,
                    subscript: rest.to_string(),
                };
            }
        }
        match Kw::lookup(&word) {
            Some(kw) => Tok::Kw(kw),
            None => Tok::Ident(word),
        }
    }

    fn scan_string(&mut self) -> Result<Tok> {
        let (line, col) = (self.line, self.col);
        self.bump();
        let mut s = String::new();
        loop {
            match self.bump() {
                None | Some('\n') => return Err(Error::lex("unterminated string", line, col)),
                Some('"') => return Ok(Tok::Str(s)),
                Some('\\') => {
                    let escaped = self
                        .bump()
                        .ok_or_else(|| Error::lex("unterminated string", line, col))?;
                    s.push(match escaped {
                        'n' => '\n',
                        't' => '\t',
                        other => other,
                    });
                }
                Some(c) => s.push(c),
            }
        }
    }

    fn scan_equals(&mut self) -> Result<Tok> {
        let run = self.run_of('=');
        if run >= 4 {
            self.advance(run);
            return Ok(Tok::ModuleEnd);
        }
        if run == 2 {
            self.advance(2);
            return Ok(Tok::DefEq);
        }
        if run == 1 {
            return Ok(match self.at(1) {
                Some('>') => {
                    self.advance(2);
                    Tok::Op(Op::Implies)
                }
                Some('<') => {
                    self.advance(2);
                    Tok::Op(Op::Le)
                }
                Some('|') => {
                    self.advance(2);
                    Tok::Op(Op::User("=|"))
                }
                _ => {
                    self.bump();
                    Tok::Op(Op::Eq)
                }
            });
        }
        Err(self.err("`===` is neither a definition nor a module terminator"))
    }

    /// A run of four or more dashes is a separator, so the operators spelled
    /// with dashes have to be recognised around it rather than before it.
    fn scan_dashes(&mut self) -> Tok {
        if self.starts_with("-+->") {
            self.advance(4);
            return Tok::Op(Op::User("-+->"));
        }
        let run = self.run_of('-');
        if run >= 4 {
            self.advance(run);
            return Tok::Separator;
        }
        for (text, tok) in [("->", Tok::Arrow), ("-|", Tok::Op(Op::User("-|")))] {
            if self.starts_with(text) {
                self.advance(2);
                return tok;
            }
        }
        self.bump();
        Tok::Op(Op::Minus)
    }

    fn scan_lt(&mut self) -> Tok {
        for (text, tok) in [
            ("<<", Tok::LTup),
            ("<=>", Tok::Op(Op::Equiv)),
            ("<=", Tok::Op(Op::Le)),
            ("<-", Tok::Gets),
            ("<>", Tok::Op(Op::Eventually)),
            ("<:", Tok::Op(Op::User("<:"))),
        ] {
            if self.starts_with(text) {
                self.advance(text.len());
                return tok;
            }
        }
        self.bump();
        Tok::Op(Op::Lt)
    }

    fn scan_gt(&mut self) -> Tok {
        for (text, tok) in [(">>", Tok::RTup), (">=", Tok::Op(Op::Ge))] {
            if self.starts_with(text) {
                self.advance(text.len());
                return tok;
            }
        }
        self.bump();
        Tok::Op(Op::Gt)
    }

    fn scan_slash(&mut self) -> Result<Tok> {
        if self.starts_with("/\\") {
            self.advance(2);
            return Ok(Tok::Op(Op::And));
        }
        if self.starts_with("/=") {
            self.advance(2);
            return Ok(Tok::Op(Op::Neq));
        }
        self.user_symbol()
            .map_or_else(|| Err(self.err("stray `/`")), Ok)
    }

    fn scan_backslash(&mut self) -> Result<Tok> {
        if self.starts_with("\\/") {
            self.advance(2);
            return Ok(Tok::Op(Op::Or));
        }
        let start = self.pos + 1;
        let mut end = start;
        while self.chars.get(end).is_some_and(char::is_ascii_alphabetic) {
            end += 1;
        }
        if end == start {
            self.bump();
            return Ok(Tok::Op(Op::SetMinus));
        }
        let name: String = self.chars[start..end].iter().collect();
        let op = match name.as_str() {
            "in" => Op::In,
            "notin" => Op::NotIn,
            "subseteq" => Op::Subseteq,
            "supseteq" => Op::Supseteq,
            "cup" | "union" => Op::Cup,
            "cap" | "intersect" => Op::Cap,
            "times" | "X" => Op::Cartesian,
            "div" => Op::Div,
            "o" | "circ" => Op::Concat,
            "equiv" => Op::Equiv,
            "lnot" | "neg" => Op::Not,
            "land" => Op::And,
            "lor" => Op::Or,
            "leq" => Op::Le,
            "geq" => Op::Ge,
            "neq" => Op::Neq,
            "A" | "forall" => Op::Forall,
            "E" | "exists" => Op::Exists,
            "AA" => Op::TemporalForall,
            "EE" => Op::TemporalExists,
            _ => match token::user_operator(&format!("\\{name}")) {
                Some(op) => op,
                None => return Err(self.err(format!("unknown operator `\\{name}`"))),
            },
        };
        self.advance(1 + name.len());
        Ok(Tok::Op(op))
    }
}
