use crate::ast::{Bound, Decl, Def, ExceptPath, Expr, LetInstance, Module, Param, QuantKind, Unit};
use crate::error::{Error, Result};
use crate::lexer::lex;
use crate::token::{Kw, Op, Tok, Token};

pub fn parse_module(src: &str) -> Result<Module> {
    parse_module_bounded(src, DEFAULT_NESTING_LIMIT)
}

/// Parse with a nesting limit of your own, for a caller whose stack is smaller
/// than [`DEFAULT_NESTING_LIMIT`] assumes.
pub fn parse_module_bounded(src: &str, nesting_limit: usize) -> Result<Module> {
    Parser::new(lex(src)?, nesting_limit).module()
}

/// Parse a bare expression, with no surrounding module. Task definitions carry
/// predicates as strings, and they have nowhere else to live.
pub fn parse_expression(src: &str) -> Result<Expr> {
    parse_expression_bounded(src, DEFAULT_NESTING_LIMIT)
}

/// As [`parse_expression`], with a nesting limit of your own.
pub fn parse_expression_bounded(src: &str, nesting_limit: usize) -> Result<Expr> {
    let mut p = Parser::new(lex(src)?, nesting_limit);
    let e = p.expr(0)?;
    if matches!(p.peek(), Tok::Eof) {
        Ok(e)
    } else {
        Err(p.err(format!("unexpected {:?} after the expression", p.peek())))
    }
}

/// How deeply expressions may nest before the parser gives up on them.
///
/// The parser descends recursively, so without a bound a file of enough open
/// brackets exhausts the stack — and a parser that aborts the process on bad
/// input is worse than one that rejects it. SANY, the reference parser, has no
/// such bound and dies with a `StackOverflowError`.
///
/// The figure is measured, by `examples/depth.rs`, not chosen. Across the 432
/// modules of the public corpus the deepest expression nests 24, so this is
/// ten times anything real. Reaching it costs about 512 KiB of stack in an
/// optimised build and 5 MiB in an unoptimised one, which fits the 8 MiB a
/// main thread is given but not the 2 MiB a spawned thread gets in a debug
/// build. A caller in that position should use [`parse_module_bounded`] with
/// a limit of its own; the cost is close to linear, at roughly 2 KiB per level
/// optimised and 20 KiB unoptimised.
pub const DEFAULT_NESTING_LIMIT: usize = 256;

struct Parser {
    toks: Vec<Token>,
    pos: usize,
    depth: usize,
    nesting_limit: usize,
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
    fn new(toks: Vec<Token>, nesting_limit: usize) -> Self {
        Self {
            toks,
            pos: 0,
            depth: 0,
            nesting_limit,
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
        self.module_body()
    }

    /// Everything from `MODULE` to the terminator. Modules nest, so this is
    /// reached both at the top of a file and part-way through one.
    fn module_body(&mut self) -> Result<Module> {
        self.expect(&Tok::Kw(Kw::Module))?;
        let name = self.expect_ident()?;
        self.expect(&Tok::Separator)?;

        let mut extends = Vec::new();
        let mut units = Vec::new();
        loop {
            while self.eat(&Tok::Separator) {}
            match self.peek() {
                Tok::ModuleEnd | Tok::Eof => break,
                Tok::Kw(Kw::Module) => {
                    let inner = self.module_body()?;
                    self.expect(&Tok::ModuleEnd)?;
                    units.push(Unit::Inner(Box::new(inner)));
                }
                Tok::Kw(Kw::Extends) => {
                    self.advance();
                    extends.push(self.expect_ident()?);
                    while self.eat(&Tok::Comma) {
                        extends.push(self.expect_ident()?);
                    }
                }
                _ => {
                    // A unit that reads nothing would leave this loop spinning
                    // on the same token for ever. Nothing should do that, and
                    // if something does, saying so beats hanging.
                    let before = self.pos;
                    let unit = self.unit_body()?;
                    if self.pos == before {
                        return Err(self.err(format!(
                            "cannot make sense of {:?}, and cannot get past it",
                            self.peek()
                        )));
                    }
                    units.push(unit);
                }
            }
        }
        Ok(Module {
            name,
            extends,
            units,
        })
    }

    fn unit_body(&mut self) -> Result<Unit> {
        if self.at_proof() {
            self.skip_proof();
            return Ok(Unit::Opaque);
        }
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
                self.skip_label();
                match self.recover(|p| p.expr(0)) {
                    Some(e) => Ok(Unit::Assume(e)),
                    None => Ok(Unit::Opaque),
                }
            }
            Tok::Kw(Kw::Theorem) => {
                self.advance();
                self.skip_label();
                let statement = self.recover(|p| p.expr(0));
                self.skip_proof();
                match statement {
                    Some(e) => Ok(Unit::Theorem(e)),
                    None => Ok(Unit::Opaque),
                }
            }
            Tok::Kw(Kw::Proof) => {
                self.skip_proof();
                Ok(Unit::Opaque)
            }
            Tok::Kw(Kw::Instance) => {
                let (module, subs) = self.instance_tail()?;
                Ok(Unit::Instance {
                    name: None,
                    module,
                    subs,
                })
            }
            // `op a == e`, and the `-.` spelling that distinguishes prefix
            // minus from the infix one.
            Tok::Op(op) => {
                self.advance();
                self.eat(&Tok::Dot);
                let operand = self.param()?;
                self.expect(&Tok::DefEq)?;
                Ok(Unit::Def(Def {
                    name: op.symbol().to_string(),
                    params: vec![operand],
                    body: self.expr(0)?,
                    local,
                }))
            }
            Tok::Ident(name) => self.named_definition(name, local),
            // A specification may define something spelled like a keyword.
            // SANY reads it and complains afterwards; so do we.
            Tok::Kw(kw) if matches!(self.peek_at(1), Tok::DefEq) && kw.text().is_some() => {
                let name = kw.text().expect("checked").to_string();
                self.named_definition(name, local)
            }
            other => Err(self.err(format!("expected a declaration, found {other:?}"))),
        }
    }

    /// Everything that can follow a name at the head of a definition: an
    /// ordinary or operator definition, a function definition, an instance, or
    /// a definition of an infix or postfix operator whose left operand this is.
    fn named_definition(&mut self, name: String, local: bool) -> Result<Unit> {
        self.advance();

        if let Tok::Op(op) = self.peek().clone() {
            self.advance();
            let left = Param::value(name);
            let params = if self.eat(&Tok::DefEq) {
                vec![left]
            } else {
                let right = self.param()?;
                self.expect(&Tok::DefEq)?;
                vec![left, right]
            };
            return Ok(Unit::Def(Def {
                name: op.symbol().to_string(),
                params,
                body: self.expr(0)?,
                local,
            }));
        }

        // `f[x \in S] == e` defines a function, and may refer to `f` inside.
        if matches!(self.peek(), Tok::LBrack) {
            self.advance();
            let bounds = self.bracketed(|p| p.bounds(&Tok::RBrack))?;
            self.expect(&Tok::RBrack)?;
            self.expect(&Tok::DefEq)?;
            let body = self.expr(0)?;
            return Ok(Unit::Def(Def {
                name,
                params: Vec::new(),
                body: Expr::FnDef {
                    bounds,
                    body: Box::new(body),
                },
                local,
            }));
        }

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
        Ok(Unit::Def(Def {
            name,
            params,
            body: self.expr(0)?,
            local,
        }))
    }

    /// `THEOREM Name == ...` names the theorem; the name carries no meaning
    /// for evaluation, so it is read and dropped.
    fn skip_label(&mut self) {
        if matches!(self.peek(), Tok::Ident(_)) && matches!(self.peek_at(1), Tok::DefEq) {
            self.advance();
            self.advance();
        }
    }

    /// Try something, and rewind rather than fail if it does not work.
    fn recover<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T>) -> Option<T> {
        let mark = self.pos;
        let fences = self.fences.len();
        let depth = self.depth;
        let Ok(value) = f(self) else {
            self.pos = mark;
            self.fences.truncate(fences);
            self.depth = depth;
            self.skip_to_unit();
            return None;
        };
        Some(value)
    }

    /// Skip a TLAPS proof.
    ///
    /// Proofs are checked by a prover, not by an evaluator, so this crate
    /// recognises them in order to get past them. A proof runs until the next
    /// token that can only begin a new module unit.
    fn skip_proof(&mut self) {
        if !self.at_proof() {
            return;
        }
        // Always consume the token that began the proof. A proof step can
        // itself look like the start of a unit -- `<1> DEFINE Op == ...` --
        // and returning without moving would leave the caller where it was.
        self.advance();
        while !matches!(self.peek(), Tok::Eof | Tok::ModuleEnd | Tok::Separator) {
            if self.at_unit_start() {
                return;
            }
            self.advance();
        }
    }

    /// Advance to whatever comes after something that could not be read.
    fn skip_to_unit(&mut self) {
        self.advance();
        while !matches!(self.peek(), Tok::Eof | Tok::ModuleEnd | Tok::Separator) {
            if self.at_unit_start() {
                return;
            }
            self.advance();
        }
    }

    /// Would a new unit begin `ahead` tokens from here? Used to tell an
    /// operator substitution from the start of an expression.
    fn at_unit_start_after(&self, ahead: usize) -> bool {
        self.toks.get(self.pos + ahead).is_some_and(|t| t.col == 1)
    }

    fn at_proof(&self) -> bool {
        if matches!(self.peek(), Tok::Kw(Kw::Proof)) {
            return true;
        }
        // A proof step is written `<1>2.`, which reaches us as `<`, a number
        // or name, then `>`. It always begins a line; without that, the `<`
        // and `>` of an ordinary comparison would look the same.
        self.begins_line()
            && matches!(self.peek(), Tok::Op(Op::Lt))
            && matches!(self.peek_at(2), Tok::Op(Op::Gt))
    }

    fn begins_line(&self) -> bool {
        self.pos == 0 || self.toks[self.pos - 1].line < self.toks[self.pos].line
    }

    /// Does a new module unit begin here? Only tokens in the first column can,
    /// which is what keeps a proof's own keywords from ending it early.
    fn at_unit_start(&self) -> bool {
        if self.col() != 1 {
            return false;
        }
        match self.peek() {
            Tok::Kw(
                Kw::Variable
                | Kw::Constant
                | Kw::Assume
                | Kw::Theorem
                | Kw::Local
                | Kw::Instance
                | Kw::Recursive
                | Kw::Extends,
            ) => true,
            Tok::Ident(_) | Tok::Op(_) => self.at_definition(),
            _ => false,
        }
    }

    /// Does a definition's left-hand side start here?
    ///
    /// Matching the shape matters rather than merely finding a `==` further
    /// on: a generated specification wraps expressions into the first column,
    /// and the next definition's `==` is only a few tokens away.
    /// Does a definition's left-hand side start here?
    ///
    /// The two callers want different things, and the looser answer suits
    /// both. Ending an expression only ever turns on an operator, because a
    /// name cannot continue one — the expression stops at a name whether or
    /// not anything calls it a definition. Ending a *proof* turns on a name,
    /// and there a rough answer is enough: proof steps are indented, so
    /// anything in the first column that looks like a definition is one.
    fn at_definition(&self) -> bool {
        let defeq = |offset: usize| matches!(self.peek_at(offset), Tok::DefEq);
        if matches!(self.peek(), Tok::Ident(_)) {
            return match self.peek_at(1) {
                // `Name ==`, and `Name(..) ==` / `Name[..] ==` without walking
                // past brackets that may hold anything.
                Tok::DefEq | Tok::LParen | Tok::LBrack => true,
                // `b ^+ ==` defines a postfix operator and `a ++ b ==` an
                // infix one. Both begin with what looks like an ordinary name.
                Tok::Op(_) => defeq(2) || (matches!(self.peek_at(2), Tok::Ident(_)) && defeq(3)),
                _ => false,
            };
        }
        // `-. a ==`, `- a ==`, `- _ ==`. An operator *can* continue an
        // expression, so here the whole shape has to be right.
        let after = usize::from(matches!(self.peek_at(1), Tok::Dot)) + 1;
        matches!(self.peek_at(after), Tok::Ident(_) | Tok::Underscore) && defeq(after + 1)
    }

    fn decl_list(&mut self) -> Result<Vec<Decl>> {
        let mut out = vec![self.decl()?];
        while self.eat(&Tok::Comma) {
            out.push(self.decl()?);
        }
        Ok(out)
    }

    fn decl(&mut self) -> Result<Decl> {
        if let Some(param) = self.fixity_declaration() {
            return Ok(Decl {
                name: param.name,
                arity: param.arity,
            });
        }
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

    fn opt_params(&mut self) -> Result<Vec<Param>> {
        let mut params = Vec::new();
        if self.eat(&Tok::LParen) {
            loop {
                params.push(self.param()?);
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
            self.expect(&Tok::RParen)?;
        }
        Ok(params)
    }

    /// An operator declared by the shape it is written in rather than by a
    /// name: `_+_` takes two operands around it, `-._` one after it, `_^#` one
    /// before it. The underscores are where the operands go.
    fn fixity_declaration(&mut self) -> Option<Param> {
        let mark = self.pos;
        // `_ op _` and `_ op`
        if matches!(self.peek(), Tok::Underscore)
            && let Tok::Op(op) = *self.peek_at(1)
        {
            self.advance();
            self.advance();
            let arity = if self.eat(&Tok::Underscore) { 2 } else { 1 };
            return Some(Param {
                name: op.symbol().to_string(),
                arity,
            });
        }
        // `op _`, and the `-._` spelling that marks a prefix minus.
        if let Tok::Op(op) = *self.peek() {
            self.advance();
            self.eat(&Tok::Dot);
            if self.eat(&Tok::Underscore) {
                return Some(Param {
                    name: op.symbol().to_string(),
                    arity: 1,
                });
            }
            self.pos = mark;
        }
        None
    }

    /// A formal parameter: a name, `f(_, _)` for one that is itself an
    /// operator, or an operator written by its fixity.
    fn param(&mut self) -> Result<Param> {
        if let Some(operator) = self.fixity_declaration() {
            return Ok(operator);
        }
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
        Ok(Param { name, arity })
    }

    fn instance_tail(&mut self) -> Result<(String, Vec<(String, Expr)>)> {
        self.expect(&Tok::Kw(Kw::Instance))?;
        let module = self.expect_ident()?;
        let mut subs = Vec::new();
        if self.eat(&Tok::Kw(Kw::With)) {
            loop {
                let name = self.expect_ident()?;
                self.expect(&Tok::Gets)?;
                let replacement = if matches!(self.peek_at(1), Tok::Comma | Tok::Eof)
                    || self.at_unit_start_after(1)
                {
                    self.operator_by_symbol()
                } else {
                    None
                };
                match replacement {
                    Some(operator) => subs.push((name, operator)),
                    None => subs.push((name, self.expr(0)?)),
                }
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
        }
        Ok((module, subs))
    }

    // ------------------------------------------------------------ expression

    fn expr(&mut self, min_prec: u8) -> Result<Expr> {
        if self.depth >= self.nesting_limit {
            let limit = self.nesting_limit;
            return Err(self.err(format!("expressions nest more than {limit} deep")));
        }
        self.depth += 1;
        let parsed = self.expr_inner(min_prec);
        self.depth -= 1;
        parsed
    }

    fn expr_inner(&mut self, min_prec: u8) -> Result<Expr> {
        // A bulleted list is an operand, not a whole expression: after
        //
        //     /\ TypeOK
        //     /\ OneVote
        //     => chosen = {}
        //
        // the `=>` ends the list and then applies to it.
        let mut lhs = match *self.peek() {
            Tok::Op(op @ (Op::And | Op::Or)) => self.junction(op)?,
            _ => self.prefix()?,
        };
        loop {
            if self.fenced() || self.at_unit_start() || self.at_proof() {
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
        if let Some(labelled) = self.skip_expression_label() {
            return labelled;
        }
        let op = match *self.peek() {
            Tok::Op(o @ (Op::Forall | Op::Exists | Op::TemporalForall | Op::TemporalExists)) => {
                self.advance();
                let kind = match o {
                    Op::Forall => QuantKind::Forall,
                    Op::Exists => QuantKind::Exists,
                    Op::TemporalForall => QuantKind::TemporalForall,
                    _ => QuantKind::TemporalExists,
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

    /// A label names a subexpression so a proof can refer to it. It has no
    /// bearing on the expression's value, so it is dropped.
    fn skip_expression_label(&mut self) -> Option<Result<Expr>> {
        if !matches!(self.peek(), Tok::Ident(_)) {
            return None;
        }
        let mut ahead = 1;
        if matches!(self.peek_at(1), Tok::LParen) {
            let mut depth = 0usize;
            loop {
                match self.peek_at(ahead) {
                    Tok::LParen => depth += 1,
                    Tok::RParen => {
                        depth -= 1;
                        if depth == 0 {
                            ahead += 1;
                            break;
                        }
                    }
                    Tok::Eof => return None,
                    _ => {}
                }
                ahead += 1;
            }
        }
        if !matches!(self.peek_at(ahead), Tok::ColonColon) {
            return None;
        }
        for _ in 0..=ahead {
            self.advance();
        }
        Some(self.expr(0))
    }

    fn postfix(&mut self) -> Result<Expr> {
        let e = self.primary()?;
        self.continue_postfix(e)
    }

    fn continue_postfix(&mut self, head: Expr) -> Result<Expr> {
        let mut e = head;
        loop {
            if self.fenced() || self.at_unit_start() {
                break;
            }
            match self.peek() {
                Tok::Prime => {
                    self.advance();
                    e = Expr::Prime(Box::new(e));
                }
                Tok::Op(op) if op.is_postfix() => {
                    let op = *op;
                    self.advance();
                    e = Expr::Unary(op, Box::new(e));
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
                    let instance = match &e {
                        Expr::Ident(name) => name.clone(),
                        Expr::Qualified { instance, name, .. } => format!("{instance}!{name}"),
                        Expr::Apply(head, _) => head.to_string(),
                        _ => return Err(self.err("`!` must follow an instance name")),
                    };
                    self.advance();
                    // `Inv!2` picks the second conjunct of `Inv`, `Inv!:` its
                    // whole body, `Inv!<<` and `Inv!>>` the sides of a tuple,
                    // and `Inv!@` the subject of an EXCEPT. All are proof
                    // notation for pointing inside a definition, not values.
                    let name = match self.peek().clone() {
                        Tok::Num(n) => {
                            self.advance();
                            n.to_string()
                        }
                        Tok::Op(op) => {
                            self.advance();
                            op.symbol().to_string()
                        }
                        Tok::At => {
                            self.advance();
                            "@".to_string()
                        }
                        Tok::LTup => {
                            self.advance();
                            "<<".to_string()
                        }
                        Tok::RTup => {
                            self.advance();
                            ">>".to_string()
                        }
                        Tok::Colon => {
                            self.advance();
                            String::new()
                        }
                        _ => self.expect_ident()?,
                    };
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
            // `FoldSet(+, 0, S)` passes the operator itself, not an
            // application of it. So does `apply(', v')`, where the operator is
            // the prime.
            if (matches!(self.peek_at(1), Tok::Comma) || self.peek_at(1) == close)
                && let Some(operator) = self.operator_by_symbol()
            {
                out.push(operator);
                if !self.eat(&Tok::Comma) {
                    return Ok(out);
                }
                continue;
            }
            out.push(self.expr(0)?);
            if !self.eat(&Tok::Comma) {
                return Ok(out);
            }
        }
    }

    /// An operator standing where a value would, named by its own symbol.
    fn operator_by_symbol(&mut self) -> Option<Expr> {
        let symbol = match self.peek() {
            Tok::Op(op) => op.symbol(),
            Tok::Prime => "'",
            Tok::Kw(Kw::Enabled) => "ENABLED",
            Tok::Kw(Kw::Unchanged) => "UNCHANGED",
            Tok::Kw(Kw::Domain) => "DOMAIN",
            Tok::Kw(Kw::Subset) => "SUBSET",
            Tok::Kw(Kw::Union) => "UNION",
            _ => return None,
        };
        self.advance();
        Some(Expr::Ident(symbol.to_string()))
    }

    fn primary(&mut self) -> Result<Expr> {
        let at = self.pos;
        match self.advance() {
            Tok::Num(n) => Ok(Expr::Num(n)),
            Tok::Decimal(text) => Ok(Expr::Decimal(text)),
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
            Tok::Kw(Kw::Let) => self.let_form(),
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
            Tok::Kw(Kw::Lambda) => {
                let mut params = vec![self.param()?];
                while self.eat(&Tok::Comma) {
                    params.push(self.param()?);
                }
                self.expect(&Tok::Colon)?;
                Ok(Expr::Lambda {
                    params,
                    body: Box::new(self.expr(0)?),
                })
            }
            // `WF_vars` arrives as one word, but `WF_<<a, b>>` leaves the
            // subscript for the parser to read.
            Tok::Fair { strong, subscript } => {
                let subscript = if subscript.is_empty() {
                    self.postfix()?
                } else {
                    Expr::Ident(subscript)
                };
                self.expect(&Tok::LParen)?;
                let action = self.bracketed(|p| p.expr(0))?;
                self.expect(&Tok::RParen)?;
                Ok(Expr::Fairness {
                    strong,
                    subscript: Box::new(subscript),
                    action: Box::new(action),
                })
            }
            other => Err(self.err_at(at, format!("expected an expression, found {other:?}"))),
        }
    }

    fn let_form(&mut self) -> Result<Expr> {
        let mut defs = Vec::new();
        let mut instances = Vec::new();
        while !matches!(self.peek(), Tok::Kw(Kw::In)) {
            if self.eat(&Tok::Kw(Kw::Recursive)) {
                self.decl_list()?;
                continue;
            }
            if matches!(self.peek(), Tok::Kw(Kw::Instance)) {
                let (module, subs) = self.instance_tail()?;
                instances.push(LetInstance {
                    name: None,
                    module,
                    subs,
                });
                continue;
            }
            let Tok::Ident(name) = self.peek().clone() else {
                return Err(self.err("expected a definition inside LET"));
            };
            match self.named_definition(name, false)? {
                Unit::Def(def) => defs.push(def),
                Unit::Instance { name, module, subs } => {
                    instances.push(LetInstance { name, module, subs });
                }
                _ => return Err(self.err("only definitions may appear inside LET")),
            }
        }
        self.advance();
        let body = self.expr(0)?;
        Ok(Expr::Let {
            defs,
            instances,
            body: Box::new(body),
        })
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
            if other.is_some() || !self.eat(&Tok::Op(Op::Always)) {
                return Ok(Expr::Case { arms, other });
            }
        }
    }

    fn tuple_or_action(&mut self) -> Result<Expr> {
        let items = self.bracketed(|p| p.expr_list(&Tok::RTup))?;
        self.expect(&Tok::RTup)?;
        if self.at_subscript() {
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

    /// Is a `_v` subscript coming? Identifiers may begin with an underscore,
    /// so `[A]_vars` reaches the parser as one token, not two.
    fn at_subscript(&self) -> bool {
        match self.peek() {
            Tok::Underscore => true,
            Tok::Ident(name) => name.starts_with('_'),
            _ => false,
        }
    }

    /// The `v` of `[A]_v`, parsed tightly so it cannot swallow what follows.
    fn subscript(&mut self) -> Result<Expr> {
        if let Tok::Ident(name) = self.peek()
            && let Some(rest) = name.strip_prefix('_')
            && !rest.is_empty()
        {
            let head = Expr::Ident(rest.to_string());
            self.advance();
            // The name may carry on -- `[A]_Inst!vars` -- so whatever follows
            // it still belongs to the subscript.
            return self.continue_postfix(head);
        }
        self.expect(&Tok::Underscore)?;
        self.postfix()
    }

    /// `{a, b}`, `{x \in S : P}` and `{e : x \in S}` are told apart by what
    /// follows their first expression rather than by scanning ahead: a
    /// `CHOOSE` inside the braces has a `:` of its own, and a lookahead
    /// cannot tell whose it is.
    fn brace_form(&mut self) -> Result<Expr> {
        if self.eat(&Tok::RBrace) {
            return Ok(Expr::SetEnum(Vec::new()));
        }
        self.bracketed(|p| {
            let first = p.expr(0)?;
            if p.eat(&Tok::Colon) {
                if let Some(bound) = as_bound(&first) {
                    let pred = p.expr(0)?;
                    p.expect(&Tok::RBrace)?;
                    return Ok(Expr::SetFilter {
                        bound: Box::new(bound),
                        pred: Box::new(pred),
                    });
                }
                let bounds = p.bounds(&Tok::RBrace)?;
                p.expect(&Tok::RBrace)?;
                return Ok(Expr::SetMap {
                    expr: Box::new(first),
                    bounds,
                });
            }
            let mut items = vec![first];
            while p.eat(&Tok::Comma) {
                items.push(p.expr(0)?);
            }
            p.expect(&Tok::RBrace)?;
            Ok(Expr::SetEnum(items))
        })
    }

    fn bracket_form(&mut self) -> Result<Expr> {
        let shape = if self.subscript_follows_bracket() {
            Shape::Closed
        } else {
            self.shape()
        };
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
                    let indices = self.bracketed(|p| p.expr_list(&Tok::RBrack))?;
                    self.expect(&Tok::RBrack)?;
                    path.push(ExceptPath::Index(if indices.len() == 1 {
                        indices.into_iter().next().expect("length checked")
                    } else {
                        Expr::Tuple(indices)
                    }));
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

    /// Is this `[A]_v`? A quantifier inside the brackets puts a `:` where a
    /// record set would have one, so the only reliable sign is the subscript
    /// after the closing bracket.
    fn subscript_follows_bracket(&self) -> bool {
        let mut depth = 0usize;
        for (offset, t) in self.toks[self.pos..].iter().enumerate() {
            match &t.tok {
                Tok::LParen | Tok::LBrack | Tok::LBrace | Tok::LTup => depth += 1,
                Tok::RBrack if depth == 0 => {
                    return match &self.peek_at(offset + 1) {
                        Tok::Underscore => true,
                        Tok::Ident(name) => name.starts_with('_'),
                        _ => false,
                    };
                }
                Tok::RParen | Tok::RBrack | Tok::RBrace | Tok::RTup => {
                    if depth == 0 {
                        return false;
                    }
                    depth -= 1;
                }
                Tok::Eof => return false,
                _ => {}
            }
        }
        false
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

/// Read `x \in S` back as the bound it is, so `{x \in S : P}` can be told from
/// `{e : x \in S}` once the first expression has already been parsed.
fn as_bound(e: &Expr) -> Option<Bound> {
    let Expr::Binary(Op::In, lhs, domain) = e else {
        return None;
    };
    let (names, destructure) = match &**lhs {
        Expr::Ident(name) => (vec![name.clone()], false),
        Expr::Tuple(items) => {
            let mut names = Vec::with_capacity(items.len());
            for item in items {
                let Expr::Ident(name) = item else {
                    return None;
                };
                names.push(name.clone());
            }
            (names, true)
        }
        _ => return None,
    };
    Some(Bound {
        names,
        domain: Some((**domain).clone()),
        destructure,
    })
}
