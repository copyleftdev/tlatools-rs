use crate::ast::{Bound, Decl, Def, ExceptPath, Expr, Module, QuantKind, Unit};
use crate::error::{Error, Result};
use crate::lexer::lex;
use crate::token::{Kw, Op, Tok, Token};

pub fn parse_module(src: &str) -> Result<Module> {
    Parser::new(lex(src)?).module()
}

/// Parse a bare expression, with no surrounding module. Task definitions carry
/// predicates as strings, and they have nowhere else to live.
pub fn parse_expression(src: &str) -> Result<Expr> {
    let mut p = Parser::new(lex(src)?);
    let e = p.expr(0)?;
    if matches!(p.peek(), Tok::Eof) {
        Ok(e)
    } else {
        Err(p.err(format!("unexpected {:?} after the expression", p.peek())))
    }
}

struct Parser {
    toks: Vec<Token>,
    pos: usize,
    /// Column fences from enclosing junction lists. A token at or left of the
    /// innermost fence ends the current conjunct; a bracketed context pushes 0
    /// to suspend the rule while inside it.
    fences: Vec<u32>,
}

/// What the tokens after a `[` or `{` reveal about which construct it opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    MapsTo { bounded: bool },
    Colon { bounded: bool },
    Arrow,
    Except,
    Closed,
}

impl Parser {
    fn new(toks: Vec<Token>) -> Self {
        Self {
            toks,
            pos: 0,
            fences: Vec::new(),
        }
    }

    fn peek(&self) -> &Tok {
        &self.toks[self.pos].tok
    }

    fn peek_at(&self, offset: usize) -> &Tok {
        let i = (self.pos + offset).min(self.toks.len() - 1);
        &self.toks[i].tok
    }

    fn col(&self) -> u32 {
        self.toks[self.pos].col
    }

    fn advance(&mut self) -> Tok {
        let t = self.toks[self.pos].tok.clone();
        if self.pos + 1 < self.toks.len() {
            self.pos += 1;
        }
        t
    }

    fn eat(&mut self, tok: &Tok) -> bool {
        if self.peek() == tok {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, tok: &Tok) -> Result<()> {
        if self.eat(tok) {
            Ok(())
        } else {
            Err(self.err(format!("expected {tok:?}, found {:?}", self.peek())))
        }
    }

    fn expect_ident(&mut self) -> Result<String> {
        let at = self.pos;
        match self.advance() {
            Tok::Ident(name) => Ok(name),
            other => Err(self.err_at(at, format!("expected an identifier, found {other:?}"))),
        }
    }

    fn err(&self, message: impl Into<String>) -> Error {
        self.err_at(self.pos, message)
    }

    /// Errors point at the token that caused them, which is not the current
    /// one once it has been consumed.
    fn err_at(&self, at: usize, message: impl Into<String>) -> Error {
        let t = &self.toks[at];
        Error::parse(message, t.line, t.col)
    }

    /// True when the upcoming token belongs to an enclosing junction list
    /// rather than to the expression being parsed.
    fn fenced(&self) -> bool {
        self.fences.last().is_some_and(|&f| self.col() <= f)
    }

    fn bracketed<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        self.fences.push(0);
        let out = f(self);
        self.fences.pop();
        out
    }

    // ---------------------------------------------------------------- module

    fn module(&mut self) -> Result<Module> {
        while !(matches!(self.peek(), Tok::Separator)
            && matches!(self.peek_at(1), Tok::Kw(Kw::Module)))
        {
            if matches!(self.peek(), Tok::Eof) {
                return Err(self.err("no `---- MODULE ----` header found"));
            }
            self.advance();
        }
        self.advance();
        self.advance();
        let name = self.expect_ident()?;
        self.expect(&Tok::Separator)?;

        let mut extends = Vec::new();
        let mut units = Vec::new();
        loop {
            while self.eat(&Tok::Separator) {}
            match self.peek() {
                Tok::ModuleEnd | Tok::Eof => break,
                Tok::Kw(Kw::Extends) => {
                    self.advance();
                    extends.push(self.expect_ident()?);
                    while self.eat(&Tok::Comma) {
                        extends.push(self.expect_ident()?);
                    }
                }
                _ => units.push(self.unit()?),
            }
        }
        Ok(Module {
            name,
            extends,
            units,
        })
    }

    fn unit(&mut self) -> Result<Unit> {
        let local = self.eat(&Tok::Kw(Kw::Local));
        match self.peek().clone() {
            Tok::Kw(Kw::Constant) => {
                self.advance();
                Ok(Unit::Constants(self.decl_list()?))
            }
            Tok::Kw(Kw::Recursive) => {
                self.advance();
                Ok(Unit::Recursive(self.decl_list()?))
            }
            Tok::Kw(Kw::Variable) => {
                self.advance();
                let mut names = vec![self.expect_ident()?];
                while self.eat(&Tok::Comma) {
                    names.push(self.expect_ident()?);
                }
                Ok(Unit::Variables(names))
            }
            Tok::Kw(Kw::Assume) => {
                self.advance();
                Ok(Unit::Assume(self.expr(0)?))
            }
            Tok::Kw(Kw::Theorem) => {
                self.advance();
                Ok(Unit::Theorem(self.expr(0)?))
            }
            Tok::Kw(Kw::Instance) => {
                let (module, subs) = self.instance_tail()?;
                Ok(Unit::Instance {
                    name: None,
                    module,
                    subs,
                })
            }
            Tok::Ident(name) => {
                self.advance();
                let params = self.opt_params()?;
                self.expect(&Tok::DefEq)?;
                if matches!(self.peek(), Tok::Kw(Kw::Instance)) {
                    let (module, subs) = self.instance_tail()?;
                    return Ok(Unit::Instance {
                        name: Some(name),
                        module,
                        subs,
                    });
                }
                let body = self.expr(0)?;
                Ok(Unit::Def(Def {
                    name,
                    params,
                    body,
                    local,
                }))
            }
            other => Err(self.err(format!("expected a declaration, found {other:?}"))),
        }
    }

    fn decl_list(&mut self) -> Result<Vec<Decl>> {
        let mut out = vec![self.decl()?];
        while self.eat(&Tok::Comma) {
            out.push(self.decl()?);
        }
        Ok(out)
    }

    fn decl(&mut self) -> Result<Decl> {
        let name = self.expect_ident()?;
        let mut arity = 0;
        if self.eat(&Tok::LParen) {
            loop {
                if !self.eat(&Tok::Underscore) {
                    self.expect_ident()?;
                }
                arity += 1;
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
            self.expect(&Tok::RParen)?;
        }
        Ok(Decl { name, arity })
    }

    fn opt_params(&mut self) -> Result<Vec<String>> {
        let mut params = Vec::new();
        if self.eat(&Tok::LParen) {
            loop {
                params.push(self.expect_ident()?);
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
            self.expect(&Tok::RParen)?;
        }
        Ok(params)
    }

    fn instance_tail(&mut self) -> Result<(String, Vec<(String, Expr)>)> {
        self.expect(&Tok::Kw(Kw::Instance))?;
        let module = self.expect_ident()?;
        let mut subs = Vec::new();
        if self.eat(&Tok::Kw(Kw::With)) {
            loop {
                let name = self.expect_ident()?;
                self.expect(&Tok::Gets)?;
                subs.push((name, self.expr(0)?));
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
        }
        Ok((module, subs))
    }

    // ------------------------------------------------------------ expression

    fn expr(&mut self, min_prec: u8) -> Result<Expr> {
        if let Tok::Op(op @ (Op::And | Op::Or)) = *self.peek() {
            return self.junction(op);
        }
        let mut lhs = self.prefix()?;
        loop {
            if self.fenced() {
                break;
            }
            let Tok::Op(op) = *self.peek() else { break };
            let Some(prec) = op.infix_prec() else { break };
            if prec < min_prec {
                break;
            }
            self.advance();
            let next_min = if op.is_right_assoc() { prec } else { prec + 1 };
            let rhs = self.expr(next_min)?;
            lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    /// A bulleted `/\` or `\/` list, scoped by the column of its bullets.
    fn junction(&mut self, op: Op) -> Result<Expr> {
        let col = self.col();
        self.fences.push(col);
        let mut items = Vec::new();
        while *self.peek() == Tok::Op(op) && self.col() == col {
            self.advance();
            items.push(self.expr(0)?);
        }
        self.fences.pop();
        Ok(if op == Op::And {
            Expr::conjunction(items)
        } else {
            Expr::disjunction(items)
        })
    }

    fn prefix(&mut self) -> Result<Expr> {
        let op = match *self.peek() {
            Tok::Op(o @ (Op::Forall | Op::Exists)) => {
                self.advance();
                let kind = if o == Op::Forall {
                    QuantKind::Forall
                } else {
                    QuantKind::Exists
                };
                let bounds = self.bounds(&Tok::Colon)?;
                self.expect(&Tok::Colon)?;
                let body = self.expr(0)?;
                return Ok(Expr::Quant {
                    kind,
                    bounds,
                    body: Box::new(body),
                });
            }
            Tok::Op(o @ (Op::Not | Op::Always | Op::Eventually | Op::Minus)) => o,
            Tok::Kw(Kw::Domain) => Op::Domain,
            Tok::Kw(Kw::Subset) => Op::Subset,
            Tok::Kw(Kw::Union) => Op::BigUnion,
            Tok::Kw(Kw::Enabled) => Op::Enabled,
            Tok::Kw(Kw::Unchanged) => Op::Unchanged,
            _ => return self.postfix(),
        };
        self.advance();
        // Every prefix operator here binds tighter than `/\`, so `[][A]_v /\ B`
        // is a conjunction of two formulas rather than `[]` over both.
        let operand_prec = match op {
            Op::Minus => 11,
            Op::Domain | Op::Subset | Op::BigUnion => 9,
            _ => 5,
        };
        let operand = self.expr(operand_prec)?;
        Ok(Expr::Unary(op, Box::new(operand)))
    }

    fn postfix(&mut self) -> Result<Expr> {
        let mut e = self.primary()?;
        loop {
            if self.fenced() {
                break;
            }
            match self.peek() {
                Tok::Prime => {
                    self.advance();
                    e = Expr::Prime(Box::new(e));
                }
                Tok::Dot => {
                    self.advance();
                    e = Expr::Field(Box::new(e), self.expect_ident()?);
                }
                Tok::LBrack => {
                    self.advance();
                    let args = self.bracketed(|p| p.expr_list(&Tok::RBrack))?;
                    self.expect(&Tok::RBrack)?;
                    e = Expr::FnApply(Box::new(e), args);
                }
                Tok::LParen if matches!(e, Expr::Ident(_)) => {
                    self.advance();
                    let args = self.bracketed(|p| p.expr_list(&Tok::RParen))?;
                    self.expect(&Tok::RParen)?;
                    e = Expr::Apply(Box::new(e), args);
                }
                Tok::Bang => {
                    let Expr::Ident(instance) = e else {
                        return Err(self.err("`!` must follow an instance name"));
                    };
                    self.advance();
                    let name = self.expect_ident()?;
                    let mut args = Vec::new();
                    if self.eat(&Tok::LParen) {
                        args = self.bracketed(|p| p.expr_list(&Tok::RParen))?;
                        self.expect(&Tok::RParen)?;
                    }
                    e = Expr::Qualified {
                        instance,
                        name,
                        args,
                    };
                }
                _ => break,
            }
        }
        Ok(e)
    }

    fn expr_list(&mut self, close: &Tok) -> Result<Vec<Expr>> {
        let mut out = Vec::new();
        if self.peek() == close {
            return Ok(out);
        }
        loop {
            out.push(self.expr(0)?);
            if !self.eat(&Tok::Comma) {
                return Ok(out);
            }
        }
    }

    fn primary(&mut self) -> Result<Expr> {
        let at = self.pos;
        match self.advance() {
            Tok::Num(n) => Ok(Expr::Num(n)),
            Tok::Str(s) => Ok(Expr::Str(s)),
            Tok::Kw(Kw::True) => Ok(Expr::Bool(true)),
            Tok::Kw(Kw::False) => Ok(Expr::Bool(false)),
            Tok::Ident(name) => Ok(Expr::Ident(name)),
            Tok::At => Ok(Expr::At),
            Tok::LParen => {
                let e = self.bracketed(|p| p.expr(0))?;
                self.expect(&Tok::RParen)?;
                Ok(e)
            }
            Tok::LTup => self.tuple_or_action(),
            Tok::LBrace => self.brace_form(),
            Tok::LBrack => self.bracket_form(),
            Tok::Kw(Kw::If) => {
                let cond = self.expr(0)?;
                self.expect(&Tok::Kw(Kw::Then))?;
                let then = self.expr(0)?;
                self.expect(&Tok::Kw(Kw::Else))?;
                let otherwise = self.expr(0)?;
                Ok(Expr::If {
                    cond: Box::new(cond),
                    then: Box::new(then),
                    otherwise: Box::new(otherwise),
                })
            }
            Tok::Kw(Kw::Let) => {
                let mut defs = Vec::new();
                while !matches!(self.peek(), Tok::Kw(Kw::In)) {
                    let name = self.expect_ident()?;
                    let params = self.opt_params()?;
                    self.expect(&Tok::DefEq)?;
                    let body = self.expr(0)?;
                    defs.push(Def {
                        name,
                        params,
                        body,
                        local: false,
                    });
                }
                self.advance();
                let body = self.expr(0)?;
                Ok(Expr::Let {
                    defs,
                    body: Box::new(body),
                })
            }
            Tok::Kw(Kw::Choose) => {
                let mut bounds = self.bounds(&Tok::Colon)?;
                self.expect(&Tok::Colon)?;
                let body = self.expr(0)?;
                if bounds.len() != 1 {
                    return Err(self.err("CHOOSE takes exactly one bound variable"));
                }
                Ok(Expr::Choose {
                    bound: Box::new(bounds.remove(0)),
                    body: Box::new(body),
                })
            }
            Tok::Kw(Kw::Case) => self.case_form(),
            Tok::Fair { strong, subscript } => {
                self.expect(&Tok::LParen)?;
                let action = self.bracketed(|p| p.expr(0))?;
                self.expect(&Tok::RParen)?;
                Ok(Expr::Fairness {
                    strong,
                    subscript: Box::new(Expr::Ident(subscript)),
                    action: Box::new(action),
                })
            }
            other => Err(self.err_at(at, format!("expected an expression, found {other:?}"))),
        }
    }

    fn case_form(&mut self) -> Result<Expr> {
        let mut arms = Vec::new();
        let mut other = None;
        loop {
            if self.eat(&Tok::Kw(Kw::Other)) {
                self.expect(&Tok::Arrow)?;
                other = Some(Box::new(self.expr(0)?));
            } else {
                let guard = self.expr(0)?;
                self.expect(&Tok::Arrow)?;
                arms.push((guard, self.expr(0)?));
            }
            if !(*self.peek() == Tok::Op(Op::Or) && other.is_none()) {
                break;
            }
            self.advance();
        }
        Ok(Expr::Case { arms, other })
    }

    fn tuple_or_action(&mut self) -> Result<Expr> {
        let items = self.bracketed(|p| p.expr_list(&Tok::RTup))?;
        self.expect(&Tok::RTup)?;
        if self.eat(&Tok::Underscore) {
            if items.len() != 1 {
                return Err(self.err("`<<A>>_v` takes a single action"));
            }
            let subscript = self.subscript()?;
            return Ok(Expr::ActionAngle {
                action: Box::new(items.into_iter().next().expect("length checked")),
                subscript: Box::new(subscript),
            });
        }
        Ok(Expr::Tuple(items))
    }

    /// The `v` of `[A]_v`, parsed tightly so it cannot swallow what follows.
    fn subscript(&mut self) -> Result<Expr> {
        self.postfix()
    }

    fn brace_form(&mut self) -> Result<Expr> {
        if self.eat(&Tok::RBrace) {
            return Ok(Expr::SetEnum(Vec::new()));
        }
        let shape = self.shape();
        self.bracketed(|p| match shape {
            Shape::Colon { bounded: true } => {
                let mut bounds = p.bounds(&Tok::Colon)?;
                p.expect(&Tok::Colon)?;
                let pred = p.expr(0)?;
                p.expect(&Tok::RBrace)?;
                if bounds.len() != 1 {
                    return Err(p.err("a set filter takes exactly one bound variable"));
                }
                Ok(Expr::SetFilter {
                    bound: Box::new(bounds.remove(0)),
                    pred: Box::new(pred),
                })
            }
            Shape::Colon { bounded: false } => {
                let expr = p.expr(0)?;
                p.expect(&Tok::Colon)?;
                let bounds = p.bounds(&Tok::RBrace)?;
                p.expect(&Tok::RBrace)?;
                Ok(Expr::SetMap {
                    expr: Box::new(expr),
                    bounds,
                })
            }
            _ => {
                let items = p.expr_list(&Tok::RBrace)?;
                p.expect(&Tok::RBrace)?;
                Ok(Expr::SetEnum(items))
            }
        })
    }

    fn bracket_form(&mut self) -> Result<Expr> {
        let shape = self.shape();
        let inner = self.bracketed(|p| match shape {
            Shape::MapsTo { bounded: true } => {
                let bounds = p.bounds(&Tok::MapsTo)?;
                p.expect(&Tok::MapsTo)?;
                let body = p.expr(0)?;
                p.expect(&Tok::RBrack)?;
                Ok(Expr::FnDef {
                    bounds,
                    body: Box::new(body),
                })
            }
            Shape::MapsTo { bounded: false } => {
                let fields = p.field_list(&Tok::MapsTo)?;
                p.expect(&Tok::RBrack)?;
                Ok(Expr::Record(fields))
            }
            Shape::Colon { .. } => {
                let fields = p.field_list(&Tok::Colon)?;
                p.expect(&Tok::RBrack)?;
                Ok(Expr::RecordSet(fields))
            }
            Shape::Arrow => {
                let domain = p.expr(0)?;
                p.expect(&Tok::Arrow)?;
                let range = p.expr(0)?;
                p.expect(&Tok::RBrack)?;
                Ok(Expr::FnSet {
                    domain: Box::new(domain),
                    range: Box::new(range),
                })
            }
            Shape::Except => {
                let base = p.expr(0)?;
                p.expect(&Tok::Kw(Kw::Except))?;
                let updates = p.except_updates()?;
                p.expect(&Tok::RBrack)?;
                Ok(Expr::Except {
                    base: Box::new(base),
                    updates,
                })
            }
            Shape::Closed => {
                let action = p.expr(0)?;
                p.expect(&Tok::RBrack)?;
                Ok(action)
            }
        })?;
        if shape != Shape::Closed {
            return Ok(inner);
        }
        self.expect(&Tok::Underscore)?;
        let subscript = self.subscript()?;
        Ok(Expr::ActionBox {
            action: Box::new(inner),
            subscript: Box::new(subscript),
        })
    }

    fn field_list(&mut self, sep: &Tok) -> Result<Vec<(String, Expr)>> {
        let mut out = Vec::new();
        loop {
            let name = self.expect_ident()?;
            self.expect(sep)?;
            out.push((name, self.expr(0)?));
            if !self.eat(&Tok::Comma) {
                return Ok(out);
            }
        }
    }

    fn except_updates(&mut self) -> Result<Vec<(Vec<ExceptPath>, Expr)>> {
        let mut out = Vec::new();
        loop {
            self.expect(&Tok::Bang)?;
            let mut path = Vec::new();
            loop {
                if self.eat(&Tok::LBrack) {
                    path.push(ExceptPath::Index(self.expr(0)?));
                    self.expect(&Tok::RBrack)?;
                } else if self.eat(&Tok::Dot) {
                    path.push(ExceptPath::Field(self.expect_ident()?));
                } else {
                    break;
                }
            }
            if path.is_empty() {
                return Err(self.err("EXCEPT update needs a `[...]` or `.field` path"));
            }
            self.expect(&Tok::Op(Op::Eq))?;
            out.push((path, self.expr(0)?));
            if !self.eat(&Tok::Comma) {
                return Ok(out);
            }
        }
    }

    fn bounds(&mut self, terminator: &Tok) -> Result<Vec<Bound>> {
        let mut out = Vec::new();
        loop {
            let mut destructure = false;
            let mut names = Vec::new();
            if self.eat(&Tok::LTup) {
                destructure = true;
                loop {
                    names.push(self.expect_ident()?);
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                self.expect(&Tok::RTup)?;
            } else {
                names.push(self.expect_ident()?);
                while *self.peek() == Tok::Comma && matches!(self.peek_at(1), Tok::Ident(_)) {
                    self.advance();
                    names.push(self.expect_ident()?);
                }
            }
            let domain = if self.eat(&Tok::Op(Op::In)) {
                Some(self.expr(0)?)
            } else {
                None
            };
            out.push(Bound {
                names,
                domain,
                destructure,
            });
            if self.peek() == terminator || !self.eat(&Tok::Comma) {
                return Ok(out);
            }
        }
    }

    /// Look ahead past a just-consumed `[` or `{` to the first delimiter that
    /// identifies the construct, ignoring anything nested inside brackets.
    fn shape(&self) -> Shape {
        let mut depth = 0usize;
        let mut saw_in = false;
        for t in &self.toks[self.pos..] {
            match &t.tok {
                Tok::LParen | Tok::LBrack | Tok::LBrace | Tok::LTup => depth += 1,
                Tok::RParen | Tok::RBrack | Tok::RBrace | Tok::RTup => {
                    if depth == 0 {
                        return Shape::Closed;
                    }
                    depth -= 1;
                }
                Tok::Eof => break,
                _ if depth > 0 => {}
                Tok::Op(Op::In) => saw_in = true,
                Tok::MapsTo => return Shape::MapsTo { bounded: saw_in },
                Tok::Colon => return Shape::Colon { bounded: saw_in },
                Tok::Arrow => return Shape::Arrow,
                Tok::Kw(Kw::Except) => return Shape::Except,
                _ => {}
            }
        }
        Shape::Closed
    }
}
