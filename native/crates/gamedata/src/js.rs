//! A small JavaScript interpreter, enough for the scripts a WebTiles server generates for its
//! gamedata: AMD `define` modules of `var`s, object and array literals, assignments, `val++`,
//! functions with `if`, `for`, `for in`, `switch` and `return`. It is not a JavaScript engine:
//! no prototypes, `this`, closures over loops or regular expressions, and what it does not know
//! is an error, never a guess. Every run has a step budget, so a script cannot hang the load.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

// ---------------------------------------------------------------------------------------------
// Tokens

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Str(Rc<str>),
    Ident(Rc<str>),
    Punct(&'static str),
    Eof,
}

#[derive(Clone, Debug)]
struct Token {
    tok: Tok,
    line: u32,
    /// A line break comes before this token (for semicolon insertion).
    nl: bool,
}

const PUNCTS: &[&str] = &[
    ">>>=", "===", "!==", ">>>", "<<=", ">>=", "...", "==", "!=", "<=", ">=", "&&", "||", "??",
    "++", "--", "<<", ">>", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "=>", "{", "}", "(",
    ")", "[", "]", ";", ",", "<", ">", "+", "-", "*", "/", "%", "&", "|", "^", "!", "~", "?", ":",
    "=", ".",
];

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == '$' || (!c.is_ascii() && c.is_alphabetic())
}

fn is_ident_part(c: char) -> bool {
    is_ident_start(c) || c.is_ascii_digit()
}

fn tokenize(src: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = src.chars().collect();
    let n = chars.len();
    let mut i = 0;
    let mut line = 1u32;
    let mut nl = false;
    let mut out: Vec<Token> = Vec::with_capacity(src.len() / 4);
    let push = |out: &mut Vec<Token>, tok: Tok, line: u32, nl: &mut bool| {
        out.push(Token { tok, line, nl: *nl });
        *nl = false;
    };
    while i < n {
        let c = chars[i];
        match c {
            '\n' | '\u{2028}' | '\u{2029}' => {
                line += 1;
                nl = true;
                i += 1;
            }
            c if c.is_whitespace() || c == '\u{feff}' => i += 1,
            '/' if i + 1 < n && chars[i + 1] == '/' => {
                while i < n && chars[i] != '\n' {
                    i += 1;
                }
            }
            '/' if i + 1 < n && chars[i + 1] == '*' => {
                let start = line;
                i += 2;
                loop {
                    if i + 1 >= n {
                        return Err(format!("unterminated comment from line {start}"));
                    }
                    if chars[i] == '*' && chars[i + 1] == '/' {
                        i += 2;
                        break;
                    }
                    if chars[i] == '\n' {
                        line += 1;
                        nl = true;
                    }
                    i += 1;
                }
            }
            '"' | '\'' => {
                let quote = c;
                let start = line;
                i += 1;
                let mut s = String::new();
                loop {
                    if i >= n || chars[i] == '\n' {
                        return Err(format!("unterminated string on line {start}"));
                    }
                    let d = chars[i];
                    i += 1;
                    if d == quote {
                        break;
                    }
                    if d != '\\' {
                        s.push(d);
                        continue;
                    }
                    if i >= n {
                        return Err(format!("unterminated string on line {start}"));
                    }
                    let e = chars[i];
                    i += 1;
                    match e {
                        'n' => s.push('\n'),
                        't' => s.push('\t'),
                        'r' => s.push('\r'),
                        'b' => s.push('\u{8}'),
                        'f' => s.push('\u{c}'),
                        'v' => s.push('\u{b}'),
                        '0' => s.push('\0'),
                        '\n' => line += 1,
                        'x' | 'u' => {
                            let len = if e == 'x' { 2 } else { 4 };
                            let hex: String = chars
                                .get(i..i + len)
                                .map(|h| h.iter().collect())
                                .unwrap_or_default();
                            let code = u32::from_str_radix(&hex, 16)
                                .map_err(|_| format!("bad escape \\{e}{hex} on line {line}"))?;
                            s.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                            i += len;
                        }
                        other => s.push(other),
                    }
                }
                push(&mut out, Tok::Str(s.into()), line, &mut nl);
            }
            c if c.is_ascii_digit() || (c == '.' && i + 1 < n && chars[i + 1].is_ascii_digit()) => {
                let start = i;
                let radix = if c == '0' && i + 1 < n {
                    match chars[i + 1] {
                        'x' | 'X' => 16,
                        'o' | 'O' => 8,
                        'b' | 'B' => 2,
                        _ => 10,
                    }
                } else {
                    10
                };
                let value = if radix != 10 {
                    i += 2;
                    let ds = i;
                    while i < n && chars[i].is_digit(radix) {
                        i += 1;
                    }
                    let digits: String = chars[ds..i].iter().collect();
                    u64::from_str_radix(&digits, radix)
                        .map_err(|_| format!("bad number on line {line}"))?
                        as f64
                } else {
                    while i < n && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                    if i < n && chars[i] == '.' {
                        i += 1;
                        while i < n && chars[i].is_ascii_digit() {
                            i += 1;
                        }
                    }
                    if i < n && (chars[i] == 'e' || chars[i] == 'E') {
                        let save = i;
                        i += 1;
                        if i < n && (chars[i] == '+' || chars[i] == '-') {
                            i += 1;
                        }
                        if i < n && chars[i].is_ascii_digit() {
                            while i < n && chars[i].is_ascii_digit() {
                                i += 1;
                            }
                        } else {
                            i = save;
                        }
                    }
                    let text: String = chars[start..i].iter().collect();
                    text.parse::<f64>()
                        .map_err(|_| format!("bad number {text} on line {line}"))?
                };
                if i < n && is_ident_start(chars[i]) {
                    return Err(format!("bad number on line {line}"));
                }
                push(&mut out, Tok::Num(value), line, &mut nl);
            }
            c if is_ident_start(c) => {
                let start = i;
                while i < n && is_ident_part(chars[i]) {
                    i += 1;
                }
                let name: String = chars[start..i].iter().collect();
                push(&mut out, Tok::Ident(name.into()), line, &mut nl);
            }
            _ => {
                let p = PUNCTS
                    .iter()
                    .find(|p| {
                        p.chars()
                            .enumerate()
                            .all(|(k, pc)| chars.get(i + k) == Some(&pc))
                    })
                    .ok_or_else(|| format!("unexpected character {c:?} on line {line}"))?;
                i += p.len();
                push(&mut out, Tok::Punct(p), line, &mut nl);
            }
        }
    }
    out.push(Token {
        tok: Tok::Eof,
        line,
        nl: true,
    });
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// Syntax

#[derive(Debug)]
pub(crate) struct FuncDef {
    params: Vec<Rc<str>>,
    body: Vec<Stmt>,
}

#[derive(Debug)]
enum Expr {
    Num(f64),
    Str(Rc<str>),
    Ident(Rc<str>),
    Bool(bool),
    Null,
    This,
    Array(Vec<Expr>),
    Object(Vec<(Rc<str>, Expr)>),
    Func(Rc<FuncDef>),
    Member(Box<Expr>, Rc<str>),
    Index(Box<Expr>, Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),
    New(Box<Expr>, Vec<Expr>),
    Unary(&'static str, Box<Expr>),
    Update {
        op: &'static str,
        prefix: bool,
        target: Box<Expr>,
    },
    Binary(&'static str, Box<Expr>, Box<Expr>),
    Cond(Box<Expr>, Box<Expr>, Box<Expr>),
    Assign(&'static str, Box<Expr>, Box<Expr>),
    Seq(Vec<Expr>),
}

type VarDecls = Vec<(Rc<str>, Option<Expr>)>;

#[derive(Debug)]
struct Stmt {
    line: u32,
    kind: StmtKind,
}

#[derive(Debug)]
enum StmtKind {
    Var(VarDecls),
    Expr(Expr),
    Return(Option<Expr>),
    If(Expr, Box<Stmt>, Option<Box<Stmt>>),
    Block(Vec<Stmt>),
    For {
        init: Option<Box<Stmt>>,
        test: Option<Expr>,
        update: Option<Expr>,
        body: Box<Stmt>,
    },
    ForIn {
        name: Rc<str>,
        obj: Expr,
        body: Box<Stmt>,
    },
    While(Expr, Box<Stmt>),
    DoWhile(Box<Stmt>, Expr),
    Switch(Expr, Vec<(Option<Expr>, Vec<Stmt>)>),
    Break,
    Continue,
    Throw(Expr),
    Try {
        block: Vec<Stmt>,
        catch: Option<(Option<Rc<str>>, Vec<Stmt>)>,
        finally: Option<Vec<Stmt>>,
    },
    Func(Rc<str>, Rc<FuncDef>),
    Empty,
}

struct Parser {
    toks: Vec<Token>,
    pos: usize,
}

const MAX_NESTING: u32 = 400;

impl Parser {
    fn peek(&self) -> &Tok {
        &self.toks[self.pos].tok
    }

    fn peek_at(&self, k: usize) -> &Tok {
        &self.toks[(self.pos + k).min(self.toks.len() - 1)].tok
    }

    fn line(&self) -> u32 {
        self.toks[self.pos].line
    }

    fn next(&mut self) -> Tok {
        let t = self.toks[self.pos].tok.clone();
        if self.pos + 1 < self.toks.len() {
            self.pos += 1;
        }
        t
    }

    fn is_punct(&self, p: &str) -> bool {
        matches!(self.peek(), Tok::Punct(q) if *q == p)
    }

    fn is_word(&self, w: &str) -> bool {
        matches!(self.peek(), Tok::Ident(q) if &**q == w)
    }

    fn eat_punct(&mut self, p: &str) -> bool {
        if self.is_punct(p) {
            self.next();
            true
        } else {
            false
        }
    }

    fn eat_word(&mut self, w: &str) -> bool {
        if self.is_word(w) {
            self.next();
            true
        } else {
            false
        }
    }

    fn unexpected(&self) -> String {
        let what = match self.peek() {
            Tok::Num(n) => num_to_string(*n),
            Tok::Str(s) => format!("{s:?}"),
            Tok::Ident(s) => s.to_string(),
            Tok::Punct(p) => p.to_string(),
            Tok::Eof => "end of script".to_string(),
        };
        format!("unexpected {what} on line {}", self.line())
    }

    fn expect_punct(&mut self, p: &str) -> Result<(), String> {
        if self.eat_punct(p) {
            Ok(())
        } else {
            Err(format!("{} (expected {p})", self.unexpected()))
        }
    }

    fn ident(&mut self) -> Result<Rc<str>, String> {
        match self.peek().clone() {
            Tok::Ident(s) => {
                self.next();
                Ok(s)
            }
            _ => Err(format!("{} (expected a name)", self.unexpected())),
        }
    }

    /// The end of a statement: a `;`, or one inserted before `}`, the end or a line break.
    fn semicolon(&mut self) -> Result<(), String> {
        if self.eat_punct(";")
            || self.is_punct("}")
            || matches!(self.peek(), Tok::Eof)
            || self.toks[self.pos].nl
        {
            Ok(())
        } else {
            Err(format!("{} (expected ;)", self.unexpected()))
        }
    }

    fn program(&mut self) -> Result<Vec<Stmt>, String> {
        let mut out = Vec::new();
        while !matches!(self.peek(), Tok::Eof) {
            out.push(self.statement(0)?);
        }
        Ok(out)
    }

    fn block(&mut self, depth: u32) -> Result<Vec<Stmt>, String> {
        self.expect_punct("{")?;
        let mut out = Vec::new();
        while !self.eat_punct("}") {
            if matches!(self.peek(), Tok::Eof) {
                return Err(self.unexpected());
            }
            out.push(self.statement(depth + 1)?);
        }
        Ok(out)
    }

    fn statement(&mut self, depth: u32) -> Result<Stmt, String> {
        if depth > MAX_NESTING {
            return Err(format!(
                "statements nested too deep on line {}",
                self.line()
            ));
        }
        let line = self.line();
        let kind = self.statement_kind(depth)?;
        Ok(Stmt { line, kind })
    }

    fn var_list(&mut self, depth: u32) -> Result<VarDecls, String> {
        let mut decls = Vec::new();
        loop {
            let name = self.ident()?;
            let init = if self.eat_punct("=") {
                Some(self.assign(depth)?)
            } else {
                None
            };
            decls.push((name, init));
            if !self.eat_punct(",") {
                break;
            }
        }
        Ok(decls)
    }

    fn statement_kind(&mut self, depth: u32) -> Result<StmtKind, String> {
        if self.is_punct("{") {
            return Ok(StmtKind::Block(self.block(depth)?));
        }
        if self.eat_punct(";") {
            return Ok(StmtKind::Empty);
        }
        let word = match self.peek() {
            Tok::Ident(w) => w.clone(),
            _ => Rc::from(""),
        };
        match &*word {
            "var" | "let" | "const" => {
                self.next();
                let decls = self.var_list(depth)?;
                self.semicolon()?;
                Ok(StmtKind::Var(decls))
            }
            "function" => {
                self.next();
                let name = self.ident()?;
                let def = self.function_rest(depth)?;
                Ok(StmtKind::Func(name, def))
            }
            "return" => {
                self.next();
                let value = if self.is_punct(";")
                    || self.is_punct("}")
                    || matches!(self.peek(), Tok::Eof)
                    || self.toks[self.pos].nl
                {
                    None
                } else {
                    Some(self.expr(depth)?)
                };
                self.semicolon()?;
                Ok(StmtKind::Return(value))
            }
            "if" => {
                self.next();
                self.expect_punct("(")?;
                let test = self.expr(depth)?;
                self.expect_punct(")")?;
                let then = Box::new(self.statement(depth + 1)?);
                let otherwise = if self.eat_word("else") {
                    Some(Box::new(self.statement(depth + 1)?))
                } else {
                    None
                };
                Ok(StmtKind::If(test, then, otherwise))
            }
            "for" => {
                self.next();
                self.expect_punct("(")?;
                let decl =
                    matches!(self.peek(), Tok::Ident(w) if matches!(&**w, "var" | "let" | "const"));
                let k = if decl { 1 } else { 0 };
                if matches!(self.peek_at(k), Tok::Ident(_))
                    && matches!(self.peek_at(k + 1), Tok::Ident(w) if &**w == "in")
                {
                    if decl {
                        self.next();
                    }
                    let name = self.ident()?;
                    self.next(); // in
                    let obj = self.expr(depth)?;
                    self.expect_punct(")")?;
                    let body = Box::new(self.statement(depth + 1)?);
                    return Ok(StmtKind::ForIn { name, obj, body });
                }
                let line = self.line();
                let init = if self.is_punct(";") {
                    None
                } else if decl {
                    self.next();
                    Some(Box::new(Stmt {
                        line,
                        kind: StmtKind::Var(self.var_list(depth)?),
                    }))
                } else {
                    Some(Box::new(Stmt {
                        line,
                        kind: StmtKind::Expr(self.expr(depth)?),
                    }))
                };
                self.expect_punct(";")?;
                let test = if self.is_punct(";") {
                    None
                } else {
                    Some(self.expr(depth)?)
                };
                self.expect_punct(";")?;
                let update = if self.is_punct(")") {
                    None
                } else {
                    Some(self.expr(depth)?)
                };
                self.expect_punct(")")?;
                let body = Box::new(self.statement(depth + 1)?);
                Ok(StmtKind::For {
                    init,
                    test,
                    update,
                    body,
                })
            }
            "while" => {
                self.next();
                self.expect_punct("(")?;
                let test = self.expr(depth)?;
                self.expect_punct(")")?;
                Ok(StmtKind::While(test, Box::new(self.statement(depth + 1)?)))
            }
            "do" => {
                self.next();
                let body = Box::new(self.statement(depth + 1)?);
                if !self.eat_word("while") {
                    return Err(format!("{} (expected while)", self.unexpected()));
                }
                self.expect_punct("(")?;
                let test = self.expr(depth)?;
                self.expect_punct(")")?;
                self.eat_punct(";");
                Ok(StmtKind::DoWhile(body, test))
            }
            "switch" => {
                self.next();
                self.expect_punct("(")?;
                let disc = self.expr(depth)?;
                self.expect_punct(")")?;
                self.expect_punct("{")?;
                let mut cases: Vec<(Option<Expr>, Vec<Stmt>)> = Vec::new();
                while !self.eat_punct("}") {
                    let test = if self.eat_word("case") {
                        Some(self.expr(depth)?)
                    } else if self.eat_word("default") {
                        None
                    } else {
                        return Err(format!("{} (expected case)", self.unexpected()));
                    };
                    self.expect_punct(":")?;
                    let mut body = Vec::new();
                    while !self.is_word("case") && !self.is_word("default") && !self.is_punct("}") {
                        if matches!(self.peek(), Tok::Eof) {
                            return Err(self.unexpected());
                        }
                        body.push(self.statement(depth + 1)?);
                    }
                    cases.push((test, body));
                }
                Ok(StmtKind::Switch(disc, cases))
            }
            "break" | "continue" => {
                self.next();
                if matches!(self.peek(), Tok::Ident(_)) && !self.toks[self.pos].nl {
                    return Err(format!("labels are not supported (line {})", self.line()));
                }
                self.semicolon()?;
                Ok(if &*word == "break" {
                    StmtKind::Break
                } else {
                    StmtKind::Continue
                })
            }
            "throw" => {
                self.next();
                let value = self.expr(depth)?;
                self.semicolon()?;
                Ok(StmtKind::Throw(value))
            }
            "try" => {
                self.next();
                let block = self.block(depth)?;
                let catch = if self.eat_word("catch") {
                    let param = if self.eat_punct("(") {
                        let p = self.ident()?;
                        self.expect_punct(")")?;
                        Some(p)
                    } else {
                        None
                    };
                    Some((param, self.block(depth)?))
                } else {
                    None
                };
                let finally = if self.eat_word("finally") {
                    Some(self.block(depth)?)
                } else {
                    None
                };
                Ok(StmtKind::Try {
                    block,
                    catch,
                    finally,
                })
            }
            _ => {
                let e = self.expr(depth)?;
                self.semicolon()?;
                Ok(StmtKind::Expr(e))
            }
        }
    }

    fn function_rest(&mut self, depth: u32) -> Result<Rc<FuncDef>, String> {
        self.expect_punct("(")?;
        let mut params = Vec::new();
        while !self.eat_punct(")") {
            params.push(self.ident()?);
            if !self.eat_punct(",") {
                self.expect_punct(")")?;
                break;
            }
        }
        let body = self.block(depth)?;
        Ok(Rc::new(FuncDef { params, body }))
    }

    fn expr(&mut self, depth: u32) -> Result<Expr, String> {
        let first = self.assign(depth)?;
        if !self.is_punct(",") {
            return Ok(first);
        }
        let mut all = vec![first];
        while self.eat_punct(",") {
            all.push(self.assign(depth)?);
        }
        Ok(Expr::Seq(all))
    }

    fn assign(&mut self, depth: u32) -> Result<Expr, String> {
        if depth > MAX_NESTING {
            return Err(format!(
                "expression nested too deep on line {}",
                self.line()
            ));
        }
        let lhs = self.cond(depth)?;
        let op = match self.peek() {
            Tok::Punct(p)
                if matches!(
                    *p,
                    "=" | "+="
                        | "-="
                        | "*="
                        | "/="
                        | "%="
                        | "&="
                        | "|="
                        | "^="
                        | "<<="
                        | ">>="
                        | ">>>="
                ) =>
            {
                *p
            }
            _ => return Ok(lhs),
        };
        if !matches!(lhs, Expr::Ident(_) | Expr::Member(..) | Expr::Index(..)) {
            return Err(format!("invalid assignment on line {}", self.line()));
        }
        self.next();
        let rhs = self.assign(depth + 1)?;
        Ok(Expr::Assign(op, Box::new(lhs), Box::new(rhs)))
    }

    fn cond(&mut self, depth: u32) -> Result<Expr, String> {
        let test = self.binary(0, depth)?;
        if !self.eat_punct("?") {
            return Ok(test);
        }
        let a = self.assign(depth + 1)?;
        self.expect_punct(":")?;
        let b = self.assign(depth + 1)?;
        Ok(Expr::Cond(Box::new(test), Box::new(a), Box::new(b)))
    }

    fn binary_op(&self) -> Option<(&'static str, u8)> {
        let (op, prec) = match self.peek() {
            Tok::Punct(p) => (
                *p,
                match *p {
                    "||" | "??" => 1,
                    "&&" => 2,
                    "|" => 3,
                    "^" => 4,
                    "&" => 5,
                    "==" | "!=" | "===" | "!==" => 6,
                    "<" | ">" | "<=" | ">=" => 7,
                    "<<" | ">>" | ">>>" => 8,
                    "+" | "-" => 9,
                    "*" | "/" | "%" => 10,
                    _ => return None,
                },
            ),
            Tok::Ident(w) if &**w == "in" => ("in", 7),
            Tok::Ident(w) if &**w == "instanceof" => ("instanceof", 7),
            _ => return None,
        };
        Some((op, prec))
    }

    fn binary(&mut self, min: u8, depth: u32) -> Result<Expr, String> {
        let mut lhs = self.unary(depth)?;
        while let Some((op, prec)) = self.binary_op() {
            if prec <= min {
                break;
            }
            self.next();
            let rhs = self.binary(prec, depth + 1)?;
            lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn unary(&mut self, depth: u32) -> Result<Expr, String> {
        if depth > MAX_NESTING {
            return Err(format!(
                "expression nested too deep on line {}",
                self.line()
            ));
        }
        let op: Option<&'static str> = match self.peek() {
            Tok::Punct(p) if matches!(*p, "!" | "~" | "-" | "+") => Some(*p),
            Tok::Ident(w) if &**w == "typeof" => Some("typeof"),
            Tok::Ident(w) if &**w == "void" => Some("void"),
            _ => None,
        };
        if let Some(op) = op {
            self.next();
            return Ok(Expr::Unary(op, Box::new(self.unary(depth + 1)?)));
        }
        if self.is_punct("++") || self.is_punct("--") {
            let op = if self.eat_punct("++") {
                "++"
            } else {
                self.next();
                "--"
            };
            let target = self.unary(depth + 1)?;
            return Ok(Expr::Update {
                op,
                prefix: true,
                target: Box::new(target),
            });
        }
        let e = self.call_member(depth)?;
        if (self.is_punct("++") || self.is_punct("--")) && !self.toks[self.pos].nl {
            let op = if self.eat_punct("++") {
                "++"
            } else {
                self.next();
                "--"
            };
            return Ok(Expr::Update {
                op,
                prefix: false,
                target: Box::new(e),
            });
        }
        Ok(e)
    }

    fn args(&mut self, depth: u32) -> Result<Vec<Expr>, String> {
        let mut args = Vec::new();
        while !self.eat_punct(")") {
            args.push(self.assign(depth + 1)?);
            if !self.eat_punct(",") {
                self.expect_punct(")")?;
                break;
            }
        }
        Ok(args)
    }

    fn property_name(&mut self) -> Result<Rc<str>, String> {
        match self.next() {
            Tok::Ident(s) | Tok::Str(s) => Ok(s),
            Tok::Num(n) => Ok(num_to_string(n).into()),
            _ => {
                self.pos -= 1;
                Err(format!("{} (expected a property name)", self.unexpected()))
            }
        }
    }

    fn call_member(&mut self, depth: u32) -> Result<Expr, String> {
        let mut e = if self.eat_word("new") {
            let mut callee = self.primary(depth)?;
            loop {
                if self.eat_punct(".") {
                    let name = self.property_name()?;
                    callee = Expr::Member(Box::new(callee), name);
                } else if self.eat_punct("[") {
                    let key = self.expr(depth + 1)?;
                    self.expect_punct("]")?;
                    callee = Expr::Index(Box::new(callee), Box::new(key));
                } else {
                    break;
                }
            }
            let args = if self.eat_punct("(") {
                self.args(depth)?
            } else {
                Vec::new()
            };
            Expr::New(Box::new(callee), args)
        } else {
            self.primary(depth)?
        };
        loop {
            if self.eat_punct(".") {
                let name = self.property_name()?;
                e = Expr::Member(Box::new(e), name);
            } else if self.eat_punct("[") {
                let key = self.expr(depth + 1)?;
                self.expect_punct("]")?;
                e = Expr::Index(Box::new(e), Box::new(key));
            } else if self.eat_punct("(") {
                let args = self.args(depth)?;
                e = Expr::Call(Box::new(e), args);
            } else {
                return Ok(e);
            }
        }
    }

    fn primary(&mut self, depth: u32) -> Result<Expr, String> {
        match self.peek().clone() {
            Tok::Num(n) => {
                self.next();
                Ok(Expr::Num(n))
            }
            Tok::Str(s) => {
                self.next();
                Ok(Expr::Str(s))
            }
            Tok::Ident(w) => {
                self.next();
                Ok(match &*w {
                    "true" => Expr::Bool(true),
                    "false" => Expr::Bool(false),
                    "null" => Expr::Null,
                    "this" => Expr::This,
                    "function" => {
                        if matches!(self.peek(), Tok::Ident(_)) {
                            self.next(); // a named function expression; the name is not bound
                        }
                        Expr::Func(self.function_rest(depth + 1)?)
                    }
                    _ => Expr::Ident(w),
                })
            }
            Tok::Punct("(") => {
                self.next();
                let e = self.expr(depth + 1)?;
                self.expect_punct(")")?;
                Ok(e)
            }
            Tok::Punct("[") => {
                self.next();
                let mut items = Vec::new();
                loop {
                    if self.eat_punct("]") {
                        break;
                    }
                    if self.is_punct(",") {
                        self.next();
                        items.push(Expr::Ident("undefined".into()));
                        continue;
                    }
                    items.push(self.assign(depth + 1)?);
                    if !self.eat_punct(",") {
                        self.expect_punct("]")?;
                        break;
                    }
                }
                Ok(Expr::Array(items))
            }
            Tok::Punct("{") => {
                self.next();
                let mut props = Vec::new();
                loop {
                    if self.eat_punct("}") {
                        break;
                    }
                    let key = self.property_name()?;
                    self.expect_punct(":")?;
                    let value = self.assign(depth + 1)?;
                    props.push((key, value));
                    if !self.eat_punct(",") {
                        self.expect_punct("}")?;
                        break;
                    }
                }
                Ok(Expr::Object(props))
            }
            _ => Err(self.unexpected()),
        }
    }
}

/// A parsed script, ready to run.
pub(crate) struct Script {
    body: Vec<Stmt>,
}

pub(crate) fn parse(src: &str) -> Result<Script, String> {
    let toks = tokenize(src)?;
    let mut p = Parser { toks, pos: 0 };
    Ok(Script { body: p.program()? })
}

// ---------------------------------------------------------------------------------------------
// Values

pub(crate) type ArrRef = Rc<RefCell<Vec<Value>>>;
pub(crate) type ObjRef = Rc<RefCell<Obj>>;

/// An object: its properties in the order they were first set, as `Object.entries` lists them.
#[derive(Default, Debug)]
pub(crate) struct Obj {
    entries: Vec<(Rc<str>, Value)>,
    index: HashMap<Rc<str>, usize>,
}

impl Obj {
    pub(crate) fn get(&self, key: &str) -> Option<&Value> {
        self.index.get(key).map(|&i| &self.entries[i].1)
    }

    pub(crate) fn set(&mut self, key: Rc<str>, value: Value) {
        match self.index.get(&key) {
            Some(&i) => self.entries[i].1 = value,
            None => {
                self.index.insert(key.clone(), self.entries.len());
                self.entries.push((key, value));
            }
        }
    }

    pub(crate) fn entries(&self) -> &[(Rc<str>, Value)] {
        &self.entries
    }
}

pub(crate) struct Closure {
    def: Rc<FuncDef>,
    env: Rc<Scope>,
}

impl std::fmt::Debug for Closure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "function({})", self.def.params.join(", "))
    }
}

#[derive(Debug)]
pub(crate) enum Native {
    Define,
    Noop,
    IsNaN,
    Push(ArrRef),
    Error(Rc<str>),
}

#[derive(Clone, Debug)]
pub(crate) enum Value {
    Undef,
    Null,
    Bool(bool),
    Num(f64),
    Str(Rc<str>),
    Arr(ArrRef),
    Obj(ObjRef),
    Func(Rc<Closure>),
    Native(Rc<Native>),
}

impl Value {
    pub(crate) fn new_obj() -> Value {
        Value::Obj(Rc::new(RefCell::new(Obj::default())))
    }

    pub(crate) fn as_num(&self) -> Option<f64> {
        match self {
            Value::Num(n) => Some(*n),
            _ => None,
        }
    }

    pub(crate) fn as_obj(&self) -> Option<&ObjRef> {
        match self {
            Value::Obj(o) => Some(o),
            _ => None,
        }
    }

    pub(crate) fn is_callable(&self) -> bool {
        matches!(self, Value::Func(_) | Value::Native(_))
    }

    /// A property, as `obj.key` reads it; `None` for undefined.
    pub(crate) fn prop(&self, key: &str) -> Option<Value> {
        match self {
            Value::Obj(o) => o.borrow().get(key).cloned(),
            _ => None,
        }
    }
}

pub(crate) fn num_to_string(n: f64) -> String {
    if n.is_nan() {
        "NaN".into()
    } else if n.is_infinite() {
        if n > 0.0 {
            "Infinity".into()
        } else {
            "-Infinity".into()
        }
    } else if n == n.trunc() && n.abs() < 1e21 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

pub(crate) fn to_string(v: &Value) -> String {
    match v {
        Value::Undef => "undefined".into(),
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Num(n) => num_to_string(*n),
        Value::Str(s) => s.to_string(),
        Value::Arr(a) => a
            .borrow()
            .iter()
            .map(|x| match x {
                Value::Undef | Value::Null => String::new(),
                x => to_string(x),
            })
            .collect::<Vec<_>>()
            .join(","),
        Value::Obj(_) => "[object Object]".into(),
        Value::Func(_) | Value::Native(_) => "function".into(),
    }
}

fn str_to_number(s: &str) -> f64 {
    let t = s.trim();
    if t.is_empty() {
        return 0.0;
    }
    let (neg, body) = match t.strip_prefix('-') {
        Some(b) => (true, b),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let lower = body.to_ascii_lowercase();
    let radix = |p: &str, r: u32| -> Option<f64> {
        lower
            .strip_prefix(p)
            .and_then(|d| u64::from_str_radix(d, r).ok())
            .map(|v| v as f64)
    };
    if lower.starts_with("0x") || lower.starts_with("0o") || lower.starts_with("0b") {
        if neg || t.starts_with('+') {
            return f64::NAN;
        }
        return radix("0x", 16)
            .or_else(|| radix("0o", 8))
            .or_else(|| radix("0b", 2))
            .unwrap_or(f64::NAN);
    }
    let v = if body == "Infinity" {
        f64::INFINITY
    } else if body
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-'))
        && body.chars().any(|c| c.is_ascii_digit())
    {
        body.parse::<f64>().unwrap_or(f64::NAN)
    } else {
        f64::NAN
    };
    if neg {
        -v
    } else {
        v
    }
}

pub(crate) fn to_number(v: &Value) -> f64 {
    match v {
        Value::Undef => f64::NAN,
        Value::Null => 0.0,
        Value::Bool(b) => *b as u8 as f64,
        Value::Num(n) => *n,
        Value::Str(s) => str_to_number(s),
        Value::Arr(_) => str_to_number(&to_string(v)),
        _ => f64::NAN,
    }
}

/// ECMAScript ToInt32.
pub(crate) fn to_int32(n: f64) -> i32 {
    if !n.is_finite() {
        return 0;
    }
    let m = n.trunc().rem_euclid(4294967296.0);
    m as u32 as i32
}

fn to_uint32(n: f64) -> u32 {
    to_int32(n) as u32
}

pub(crate) fn truthy(v: &Value) -> bool {
    match v {
        Value::Undef | Value::Null => false,
        Value::Bool(b) => *b,
        Value::Num(n) => *n != 0.0 && !n.is_nan(),
        Value::Str(s) => !s.is_empty(),
        _ => true,
    }
}

fn strict_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Undef, Value::Undef) | (Value::Null, Value::Null) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Num(x), Value::Num(y)) => x == y,
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Arr(x), Value::Arr(y)) => Rc::ptr_eq(x, y),
        (Value::Obj(x), Value::Obj(y)) => Rc::ptr_eq(x, y),
        (Value::Func(x), Value::Func(y)) => Rc::ptr_eq(x, y),
        (Value::Native(x), Value::Native(y)) => Rc::ptr_eq(x, y),
        _ => false,
    }
}

fn is_object(v: &Value) -> bool {
    matches!(
        v,
        Value::Arr(_) | Value::Obj(_) | Value::Func(_) | Value::Native(_)
    )
}

fn loose_eq(a: &Value, b: &Value) -> bool {
    use Value::*;
    match (a, b) {
        (Undef | Null, Undef | Null) => true,
        (Undef | Null, _) | (_, Undef | Null) => false,
        (Num(_), Str(_)) | (Str(_), Num(_)) => to_number(a) == to_number(b),
        (Bool(_), _) => loose_eq(&Num(to_number(a)), b),
        (_, Bool(_)) => loose_eq(a, &Num(to_number(b))),
        (x, Num(_) | Str(_)) if is_object(x) => loose_eq(&Str(to_string(x).into()), b),
        (Num(_) | Str(_), y) if is_object(y) => loose_eq(a, &Str(to_string(y).into())),
        _ => strict_eq(a, b),
    }
}

fn type_of(v: &Value) -> &'static str {
    match v {
        Value::Undef => "undefined",
        Value::Null | Value::Arr(_) | Value::Obj(_) => "object",
        Value::Bool(_) => "boolean",
        Value::Num(_) => "number",
        Value::Str(_) => "string",
        Value::Func(_) | Value::Native(_) => "function",
    }
}

/// An array index a key names, if it names one.
fn array_index(key: &Value) -> Option<usize> {
    match key {
        Value::Num(n) if *n >= 0.0 && n.fract() == 0.0 && *n < 4294967295.0 => Some(*n as usize),
        Value::Str(s) => s.parse::<usize>().ok().filter(|i| i.to_string() == **s),
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------------
// Scopes and running

pub(crate) struct Scope {
    vars: RefCell<HashMap<Rc<str>, Value>>,
    parent: Option<Rc<Scope>>,
}

impl Scope {
    fn new(parent: Option<Rc<Scope>>) -> Rc<Scope> {
        Rc::new(Scope {
            vars: RefCell::new(HashMap::new()),
            parent,
        })
    }

    fn declare(&self, name: Rc<str>, value: Value) {
        self.vars.borrow_mut().insert(name, value);
    }

    pub(crate) fn lookup(&self, name: &str) -> Option<Value> {
        let mut s = self;
        loop {
            if let Some(v) = s.vars.borrow().get(name) {
                return Some(v.clone());
            }
            s = s.parent.as_deref()?;
        }
    }

    /// Set an existing variable; false when no scope declares it.
    fn assign(&self, name: &str, value: Value) -> bool {
        let mut s = self;
        loop {
            if let Some(slot) = s.vars.borrow_mut().get_mut(name) {
                *slot = value;
                return true;
            }
            match s.parent.as_deref() {
                Some(p) => s = p,
                None => return false,
            }
        }
    }
}

enum Flow {
    Normal,
    Return(Value),
    Break,
    Continue,
}

/// Runs scripts. One interpreter can run several and call the functions they made.
pub(crate) struct Interp {
    steps: u64,
    budget: u64,
    depth: u32,
    line: u32,
    /// What every `define` dependency resolves to.
    dep: Value,
    /// What the script's `define` factory returned.
    defined: Option<Value>,
}

const MAX_CALL_DEPTH: u32 = 200;

impl Interp {
    pub(crate) fn new() -> Self {
        Interp {
            steps: 0,
            budget: 0,
            depth: 0,
            line: 0,
            dep: Value::Undef,
            defined: None,
        }
    }

    fn step(&mut self) -> Result<(), String> {
        self.steps += 1;
        if self.steps > self.budget {
            Err("the script runs too long".into())
        } else {
            Ok(())
        }
    }

    /// Run an AMD module script with `define`, `window` and `assert` in reach, every dependency
    /// resolving to `dep`, and answer what its factory returned.
    pub(crate) fn run_module(
        &mut self,
        script: &Script,
        dep: Value,
        budget: u64,
    ) -> Result<Value, String> {
        self.steps = 0;
        self.budget = budget;
        let global = Scope::new(None);
        global.declare("define".into(), Value::Native(Rc::new(Native::Define)));
        global.declare("assert".into(), Value::Native(Rc::new(Native::Noop)));
        global.declare("isNaN".into(), Value::Native(Rc::new(Native::IsNaN)));
        global.declare("window".into(), Value::new_obj());
        global.declare("undefined".into(), Value::Undef);
        global.declare("NaN".into(), Value::Num(f64::NAN));
        global.declare("Infinity".into(), Value::Num(f64::INFINITY));
        global.declare("requirejs".into(), Value::Undef);
        global.declare("require".into(), Value::Undef);
        for name in [
            "Error",
            "TypeError",
            "RangeError",
            "ReferenceError",
            "SyntaxError",
        ] {
            global.declare(
                name.into(),
                Value::Native(Rc::new(Native::Error(name.into()))),
            );
        }
        self.dep = dep;
        self.defined = None;
        self.depth = 0;
        hoist(&script.body, &global);
        for stmt in &script.body {
            match self.exec(stmt, &global).map_err(|e| self.at_line(e))? {
                Flow::Normal => {}
                _ => return Err("return, break or continue outside a function".into()),
            }
        }
        self.defined
            .take()
            .ok_or_else(|| "the script defines no module".to_string())
    }

    fn at_line(&self, e: String) -> String {
        if self.line > 0 && !e.contains(" (line ") {
            format!("{e} (line {})", self.line)
        } else {
            e
        }
    }

    /// Call a function the script made, with a fresh step budget of its own.
    pub(crate) fn call_fn(
        &mut self,
        f: &Value,
        args: Vec<Value>,
        budget: u64,
    ) -> Result<Value, String> {
        self.steps = 0;
        self.budget = budget;
        self.depth = 0;
        self.call(f, args).map_err(|e| self.at_line(e))
    }

    /// The value an expression in a function's body evaluates to in that function's own scope
    /// (the scope it closes over), without calling it: for reading what it would pass on.
    pub(crate) fn find_call_arg(&mut self, f: &Value, arg: usize) -> Option<Value> {
        let Value::Func(c) = f else { return None };
        let mut found: Option<&Expr> = None;
        visit_calls(&c.def.body, &mut |args: &[Expr]| {
            if found.is_none() {
                if let Some(e @ Expr::Ident(_)) = args.get(arg) {
                    found = Some(e);
                }
            }
        });
        let Expr::Ident(name) = found? else {
            return None;
        };
        c.env.lookup(name)
    }

    fn call(&mut self, f: &Value, args: Vec<Value>) -> Result<Value, String> {
        self.step()?;
        match f {
            Value::Func(c) => {
                if self.depth >= MAX_CALL_DEPTH {
                    return Err("too much recursion".into());
                }
                let scope = Scope::new(Some(c.env.clone()));
                for (i, p) in c.def.params.iter().enumerate() {
                    scope.declare(p.clone(), args.get(i).cloned().unwrap_or(Value::Undef));
                }
                if !c.def.params.iter().any(|p| &**p == "arguments") {
                    scope.declare("arguments".into(), Value::Arr(Rc::new(RefCell::new(args))));
                }
                hoist(&c.def.body, &scope);
                self.depth += 1;
                let mut out = Ok(Value::Undef);
                for stmt in &c.def.body {
                    match self.exec(stmt, &scope) {
                        Ok(Flow::Normal) => {}
                        Ok(Flow::Return(v)) => {
                            out = Ok(v);
                            break;
                        }
                        Ok(_) => {
                            out = Err("break or continue outside a loop".into());
                            break;
                        }
                        Err(e) => {
                            out = Err(e);
                            break;
                        }
                    }
                }
                self.depth -= 1;
                out
            }
            Value::Native(n) => match &**n {
                Native::Noop => Ok(Value::Undef),
                Native::IsNaN => Ok(Value::Bool(
                    to_number(args.first().unwrap_or(&Value::Undef)).is_nan(),
                )),
                Native::Push(arr) => {
                    let mut a = arr.borrow_mut();
                    a.extend(args);
                    Ok(Value::Num(a.len() as f64))
                }
                Native::Error(name) => {
                    let msg = args.first().map(to_string).unwrap_or_default();
                    Ok(Value::Str(if msg.is_empty() {
                        name.clone()
                    } else {
                        format!("{name}: {msg}").into()
                    }))
                }
                Native::Define => {
                    let factory = args.iter().rev().find(|a| a.is_callable()).cloned();
                    let deps: Vec<Value> = match args.iter().find(|a| matches!(a, Value::Arr(_))) {
                        Some(Value::Arr(a)) => {
                            a.borrow().iter().map(|_| self.dep.clone()).collect()
                        }
                        _ => Vec::new(),
                    };
                    let result = match factory {
                        Some(f) => self.call(&f, deps)?,
                        // define({...}): the module is the object itself
                        None => args.last().cloned().unwrap_or(Value::Undef),
                    };
                    self.defined = Some(result);
                    Ok(Value::Undef)
                }
            },
            other => Err(format!("{} is not a function", to_string(other))),
        }
    }

    fn exec_block(&mut self, stmts: &[Stmt], scope: &Rc<Scope>) -> Result<Flow, String> {
        for s in stmts {
            match self.exec(s, scope)? {
                Flow::Normal => {}
                other => return Ok(other),
            }
        }
        Ok(Flow::Normal)
    }

    fn exec(&mut self, stmt: &Stmt, scope: &Rc<Scope>) -> Result<Flow, String> {
        self.step()?;
        self.line = stmt.line;
        match &stmt.kind {
            StmtKind::Var(decls) => {
                for (name, init) in decls {
                    if let Some(e) = init {
                        let v = self.eval(e, scope)?;
                        if !scope.assign(name, v.clone()) {
                            scope.declare(name.clone(), v);
                        }
                    }
                }
                Ok(Flow::Normal)
            }
            StmtKind::Expr(e) => {
                self.eval(e, scope)?;
                Ok(Flow::Normal)
            }
            StmtKind::Return(e) => {
                let v = match e {
                    Some(e) => self.eval(e, scope)?,
                    None => Value::Undef,
                };
                Ok(Flow::Return(v))
            }
            StmtKind::If(test, then, otherwise) => {
                if truthy(&self.eval(test, scope)?) {
                    self.exec(then, scope)
                } else if let Some(o) = otherwise {
                    self.exec(o, scope)
                } else {
                    Ok(Flow::Normal)
                }
            }
            StmtKind::Block(stmts) => self.exec_block(stmts, scope),
            StmtKind::For {
                init,
                test,
                update,
                body,
            } => {
                if let Some(i) = init {
                    self.exec(i, scope)?;
                }
                loop {
                    self.step()?;
                    if let Some(t) = test {
                        if !truthy(&self.eval(t, scope)?) {
                            break;
                        }
                    }
                    match self.exec(body, scope)? {
                        Flow::Break => break,
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                        Flow::Normal | Flow::Continue => {}
                    }
                    if let Some(u) = update {
                        self.eval(u, scope)?;
                    }
                }
                Ok(Flow::Normal)
            }
            StmtKind::ForIn { name, obj, body } => {
                let keys: Vec<Value> = match self.eval(obj, scope)? {
                    Value::Obj(o) => o
                        .borrow()
                        .entries
                        .iter()
                        .map(|(k, _)| Value::Str(k.clone()))
                        .collect(),
                    Value::Arr(a) => (0..a.borrow().len())
                        .map(|i| Value::Str(i.to_string().into()))
                        .collect(),
                    Value::Str(s) => (0..s.chars().count())
                        .map(|i| Value::Str(i.to_string().into()))
                        .collect(),
                    _ => Vec::new(),
                };
                for k in keys {
                    self.step()?;
                    if !scope.assign(name, k.clone()) {
                        scope.declare(name.clone(), k);
                    }
                    match self.exec(body, scope)? {
                        Flow::Break => break,
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                        Flow::Normal | Flow::Continue => {}
                    }
                }
                Ok(Flow::Normal)
            }
            StmtKind::While(test, body) => {
                loop {
                    self.step()?;
                    if !truthy(&self.eval(test, scope)?) {
                        break;
                    }
                    match self.exec(body, scope)? {
                        Flow::Break => break,
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                        Flow::Normal | Flow::Continue => {}
                    }
                }
                Ok(Flow::Normal)
            }
            StmtKind::DoWhile(body, test) => {
                loop {
                    self.step()?;
                    match self.exec(body, scope)? {
                        Flow::Break => break,
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                        Flow::Normal | Flow::Continue => {}
                    }
                    if !truthy(&self.eval(test, scope)?) {
                        break;
                    }
                }
                Ok(Flow::Normal)
            }
            StmtKind::Switch(disc, cases) => {
                let d = self.eval(disc, scope)?;
                let mut start = None;
                for (i, (test, _)) in cases.iter().enumerate() {
                    if let Some(t) = test {
                        if strict_eq(&d, &self.eval(t, scope)?) {
                            start = Some(i);
                            break;
                        }
                    }
                }
                let start = start.or_else(|| cases.iter().position(|(t, _)| t.is_none()));
                if let Some(s) = start {
                    for (_, body) in &cases[s..] {
                        match self.exec_block(body, scope)? {
                            Flow::Normal => {}
                            Flow::Break => return Ok(Flow::Normal),
                            other => return Ok(other),
                        }
                    }
                }
                Ok(Flow::Normal)
            }
            StmtKind::Break => Ok(Flow::Break),
            StmtKind::Continue => Ok(Flow::Continue),
            StmtKind::Throw(e) => {
                let v = self.eval(e, scope)?;
                Err(to_string(&v))
            }
            StmtKind::Try {
                block,
                catch,
                finally,
            } => {
                let mut out = self.exec_block(block, scope);
                if let (Err(e), Some((param, body))) = (&out, catch) {
                    if e == "the script runs too long" {
                        return out;
                    }
                    if let Some(p) = param {
                        scope.declare(p.clone(), Value::Str(e.clone().into()));
                    }
                    out = self.exec_block(body, scope);
                }
                if let Some(f) = finally {
                    match self.exec_block(f, scope)? {
                        Flow::Normal => {}
                        other => return Ok(other),
                    }
                }
                out
            }
            StmtKind::Func(..) | StmtKind::Empty => Ok(Flow::Normal),
        }
    }

    fn get_prop(&mut self, obj: &Value, key: &Value) -> Result<Value, String> {
        match obj {
            Value::Undef | Value::Null => Err(format!(
                "Cannot read properties of {} (reading '{}')",
                to_string(obj),
                to_string(key)
            )),
            Value::Obj(o) => {
                let k = to_string(key);
                Ok(o.borrow().get(&k).cloned().unwrap_or(Value::Undef))
            }
            Value::Arr(a) => {
                if let Some(i) = array_index(key) {
                    return Ok(a.borrow().get(i).cloned().unwrap_or(Value::Undef));
                }
                Ok(match to_string(key).as_str() {
                    "length" => Value::Num(a.borrow().len() as f64),
                    "push" => Value::Native(Rc::new(Native::Push(a.clone()))),
                    _ => Value::Undef,
                })
            }
            Value::Str(s) => {
                if let Some(i) = array_index(key) {
                    return Ok(s
                        .chars()
                        .nth(i)
                        .map(|c| Value::Str(c.to_string().into()))
                        .unwrap_or(Value::Undef));
                }
                Ok(match to_string(key).as_str() {
                    "length" => Value::Num(s.encode_utf16().count() as f64),
                    _ => Value::Undef,
                })
            }
            _ => Ok(Value::Undef),
        }
    }

    fn set_prop(&mut self, obj: &Value, key: &Value, value: Value) -> Result<(), String> {
        match obj {
            Value::Obj(o) => {
                o.borrow_mut().set(to_string(key).into(), value);
                Ok(())
            }
            Value::Arr(a) => {
                let i = array_index(key).ok_or_else(|| {
                    format!(
                        "setting property '{}' of an array is not supported",
                        to_string(key)
                    )
                })?;
                if i > 10_000_000 {
                    return Err("array too large".into());
                }
                let mut a = a.borrow_mut();
                if a.len() <= i {
                    a.resize(i + 1, Value::Undef);
                }
                a[i] = value;
                Ok(())
            }
            Value::Undef | Value::Null => Err(format!(
                "Cannot set properties of {} (setting '{}')",
                to_string(obj),
                to_string(key)
            )),
            // a property on a primitive or a function is silently dropped
            _ => Ok(()),
        }
    }

    /// Read a reference (name, member or index) for an update or a compound assignment.
    fn read_target(
        &mut self,
        target: &Expr,
        scope: &Rc<Scope>,
    ) -> Result<(Value, Option<(Value, Value)>), String> {
        match target {
            Expr::Ident(name) => {
                let v = scope
                    .lookup(name)
                    .ok_or_else(|| format!("{name} is not defined"))?;
                Ok((v, None))
            }
            Expr::Member(o, name) => {
                let obj = self.eval(o, scope)?;
                let key = Value::Str(name.clone());
                let v = self.get_prop(&obj, &key)?;
                Ok((v, Some((obj, key))))
            }
            Expr::Index(o, k) => {
                let obj = self.eval(o, scope)?;
                let key = self.eval(k, scope)?;
                let v = self.get_prop(&obj, &key)?;
                Ok((v, Some((obj, key))))
            }
            _ => Err("invalid assignment target".into()),
        }
    }

    fn write_target(
        &mut self,
        target: &Expr,
        place: Option<(Value, Value)>,
        value: Value,
        scope: &Rc<Scope>,
    ) -> Result<(), String> {
        match (target, place) {
            (Expr::Ident(name), _) => {
                if !scope.assign(name, value.clone()) {
                    // sloppy mode: an undeclared name becomes a global
                    let mut s: &Scope = scope;
                    while let Some(p) = s.parent.as_deref() {
                        s = p;
                    }
                    s.declare(name.clone(), value);
                }
                Ok(())
            }
            (_, Some((obj, key))) => self.set_prop(&obj, &key, value),
            _ => Err("invalid assignment target".into()),
        }
    }

    fn eval(&mut self, e: &Expr, scope: &Rc<Scope>) -> Result<Value, String> {
        match e {
            Expr::Num(n) => Ok(Value::Num(*n)),
            Expr::Str(s) => Ok(Value::Str(s.clone())),
            Expr::Bool(b) => Ok(Value::Bool(*b)),
            Expr::Null => Ok(Value::Null),
            Expr::This => Ok(Value::Undef),
            Expr::Ident(name) => scope
                .lookup(name)
                .ok_or_else(|| format!("{name} is not defined")),
            Expr::Array(items) => {
                let mut out = Vec::with_capacity(items.len());
                for i in items {
                    out.push(self.eval(i, scope)?);
                }
                Ok(Value::Arr(Rc::new(RefCell::new(out))))
            }
            Expr::Object(props) => {
                let mut o = Obj::default();
                for (k, v) in props {
                    let v = self.eval(v, scope)?;
                    o.set(k.clone(), v);
                }
                Ok(Value::Obj(Rc::new(RefCell::new(o))))
            }
            Expr::Func(def) => Ok(Value::Func(Rc::new(Closure {
                def: def.clone(),
                env: scope.clone(),
            }))),
            Expr::Member(o, name) => {
                let obj = self.eval(o, scope)?;
                self.get_prop(&obj, &Value::Str(name.clone()))
            }
            Expr::Index(o, k) => {
                let obj = self.eval(o, scope)?;
                let key = self.eval(k, scope)?;
                self.get_prop(&obj, &key)
            }
            Expr::Call(callee, args) => {
                let f = self.eval(callee, scope)?;
                let mut vals = Vec::with_capacity(args.len());
                for a in args {
                    vals.push(self.eval(a, scope)?);
                }
                if !f.is_callable() {
                    return Err(format!("{} is not a function", describe(callee)));
                }
                self.call(&f, vals)
            }
            Expr::New(callee, args) => {
                let f = self.eval(callee, scope)?;
                let mut vals = Vec::with_capacity(args.len());
                for a in args {
                    vals.push(self.eval(a, scope)?);
                }
                match &f {
                    Value::Native(n) if matches!(**n, Native::Error(_)) => self.call(&f, vals),
                    _ => Err(format!("new {} is not supported", describe(callee))),
                }
            }
            Expr::Unary(op, x) => {
                if *op == "typeof" {
                    if let Expr::Ident(name) = &**x {
                        return Ok(Value::Str(
                            type_of(&scope.lookup(name).unwrap_or(Value::Undef)).into(),
                        ));
                    }
                }
                let v = self.eval(x, scope)?;
                Ok(match *op {
                    "!" => Value::Bool(!truthy(&v)),
                    "~" => Value::Num(!to_int32(to_number(&v)) as f64),
                    "-" => Value::Num(-to_number(&v)),
                    "+" => Value::Num(to_number(&v)),
                    "typeof" => Value::Str(type_of(&v).into()),
                    _ => Value::Undef,
                })
            }
            Expr::Update { op, prefix, target } => {
                let (old, place) = self.read_target(target, scope)?;
                let old = to_number(&old);
                let new = if *op == "++" { old + 1.0 } else { old - 1.0 };
                self.write_target(target, place, Value::Num(new), scope)?;
                Ok(Value::Num(if *prefix { new } else { old }))
            }
            Expr::Binary(op, a, b) => {
                match *op {
                    "&&" => {
                        let x = self.eval(a, scope)?;
                        return if truthy(&x) {
                            self.eval(b, scope)
                        } else {
                            Ok(x)
                        };
                    }
                    "||" => {
                        let x = self.eval(a, scope)?;
                        return if truthy(&x) {
                            Ok(x)
                        } else {
                            self.eval(b, scope)
                        };
                    }
                    "??" => {
                        let x = self.eval(a, scope)?;
                        return if matches!(x, Value::Undef | Value::Null) {
                            self.eval(b, scope)
                        } else {
                            Ok(x)
                        };
                    }
                    _ => {}
                }
                let x = self.eval(a, scope)?;
                let y = self.eval(b, scope)?;
                binary(op, &x, &y)
            }
            Expr::Cond(t, a, b) => {
                if truthy(&self.eval(t, scope)?) {
                    self.eval(a, scope)
                } else {
                    self.eval(b, scope)
                }
            }
            Expr::Assign(op, target, rhs) => {
                if *op == "=" {
                    // the reference is taken before the value, as in JavaScript
                    let place = match &**target {
                        Expr::Ident(_) => None,
                        Expr::Member(o, name) => {
                            Some((self.eval(o, scope)?, Value::Str(name.clone())))
                        }
                        Expr::Index(o, k) => {
                            let obj = self.eval(o, scope)?;
                            let key = self.eval(k, scope)?;
                            Some((obj, key))
                        }
                        _ => return Err("invalid assignment target".into()),
                    };
                    let v = self.eval(rhs, scope)?;
                    self.write_target(target, place, v.clone(), scope)?;
                    return Ok(v);
                }
                let (old, place) = self.read_target(target, scope)?;
                let r = self.eval(rhs, scope)?;
                let v = binary(&op[..op.len() - 1], &old, &r)?;
                self.write_target(target, place, v.clone(), scope)?;
                Ok(v)
            }
            Expr::Seq(items) => {
                let mut last = Value::Undef;
                for i in items {
                    last = self.eval(i, scope)?;
                }
                Ok(last)
            }
        }
    }
}

fn describe(e: &Expr) -> String {
    match e {
        Expr::Ident(n) => n.to_string(),
        Expr::Member(o, n) => format!("{}.{n}", describe(o)),
        Expr::Index(o, _) => format!("{}[...]", describe(o)),
        _ => "expression".into(),
    }
}

fn binary(op: &str, x: &Value, y: &Value) -> Result<Value, String> {
    let num = |f: fn(f64, f64) -> f64| Value::Num(f(to_number(x), to_number(y)));
    let int = |f: fn(i32, i32) -> i32| {
        Value::Num(f(to_int32(to_number(x)), to_int32(to_number(y))) as f64)
    };
    Ok(match op {
        "+" => {
            let prim = |v: &Value| {
                if is_object(v) {
                    Value::Str(to_string(v).into())
                } else {
                    v.clone()
                }
            };
            let (px, py) = (prim(x), prim(y));
            if matches!(px, Value::Str(_)) || matches!(py, Value::Str(_)) {
                Value::Str(format!("{}{}", to_string(&px), to_string(&py)).into())
            } else {
                Value::Num(to_number(&px) + to_number(&py))
            }
        }
        "-" => num(|a, b| a - b),
        "*" => num(|a, b| a * b),
        "/" => num(|a, b| a / b),
        "%" => num(|a, b| a % b),
        "|" => int(|a, b| a | b),
        "&" => int(|a, b| a & b),
        "^" => int(|a, b| a ^ b),
        "<<" => int(|a, b| a.wrapping_shl(b as u32 & 31)),
        ">>" => int(|a, b| a.wrapping_shr(b as u32 & 31)),
        ">>>" => Value::Num((to_uint32(to_number(x)) >> (to_uint32(to_number(y)) & 31)) as f64),
        "==" => Value::Bool(loose_eq(x, y)),
        "!=" => Value::Bool(!loose_eq(x, y)),
        "===" => Value::Bool(strict_eq(x, y)),
        "!==" => Value::Bool(!strict_eq(x, y)),
        "<" | ">" | "<=" | ">=" => {
            let ord = match (x, y) {
                (Value::Str(a), Value::Str(b)) => Some(a.cmp(b)),
                _ => to_number(x).partial_cmp(&to_number(y)),
            };
            Value::Bool(match ord {
                None => false,
                Some(o) => match op {
                    "<" => o.is_lt(),
                    ">" => o.is_gt(),
                    "<=" => o.is_le(),
                    _ => o.is_ge(),
                },
            })
        }
        "in" => Value::Bool(match y {
            Value::Obj(o) => o.borrow().get(&to_string(x)).is_some(),
            Value::Arr(a) => {
                array_index(x).is_some_and(|i| i < a.borrow().len()) || to_string(x) == "length"
            }
            _ => return Err("'in' needs an object".into()),
        }),
        "instanceof" => Value::Bool(false),
        _ => return Err(format!("operator {op} is not supported")),
    })
}

/// Declare a body's `var`s (as undefined) and its function declarations before it runs.
fn hoist(stmts: &[Stmt], scope: &Rc<Scope>) {
    for s in stmts {
        hoist_one(s, scope);
    }
}

fn hoist_one(s: &Stmt, scope: &Rc<Scope>) {
    let declare_var = |name: &Rc<str>| {
        if !scope.vars.borrow().contains_key(name) {
            scope.declare(name.clone(), Value::Undef);
        }
    };
    match &s.kind {
        StmtKind::Var(decls) => decls.iter().for_each(|(n, _)| declare_var(n)),
        StmtKind::Func(name, def) => scope.declare(
            name.clone(),
            Value::Func(Rc::new(Closure {
                def: def.clone(),
                env: scope.clone(),
            })),
        ),
        StmtKind::If(_, a, b) => {
            hoist_one(a, scope);
            if let Some(b) = b {
                hoist_one(b, scope);
            }
        }
        StmtKind::Block(b) => hoist(b, scope),
        StmtKind::For { init, body, .. } => {
            if let Some(i) = init {
                hoist_one(i, scope);
            }
            hoist_one(body, scope);
        }
        StmtKind::ForIn { name, body, .. } => {
            declare_var(name);
            hoist_one(body, scope);
        }
        StmtKind::While(_, b) | StmtKind::DoWhile(b, _) => hoist_one(b, scope),
        StmtKind::Switch(_, cases) => cases.iter().for_each(|(_, b)| hoist(b, scope)),
        StmtKind::Try {
            block,
            catch,
            finally,
        } => {
            hoist(block, scope);
            if let Some((_, c)) = catch {
                hoist(c, scope);
            }
            if let Some(f) = finally {
                hoist(f, scope);
            }
        }
        _ => {}
    }
}

/// Every call's arguments in a body, outside nested functions.
fn visit_calls<'a>(stmts: &'a [Stmt], f: &mut dyn FnMut(&'a [Expr])) {
    fn expr<'a>(e: &'a Expr, f: &mut dyn FnMut(&'a [Expr])) {
        match e {
            Expr::Call(c, args) | Expr::New(c, args) => {
                f(args);
                expr(c, f);
                args.iter().for_each(|a| expr(a, f));
            }
            Expr::Array(items) | Expr::Seq(items) => items.iter().for_each(|i| expr(i, f)),
            Expr::Object(props) => props.iter().for_each(|(_, v)| expr(v, f)),
            Expr::Member(o, _) | Expr::Unary(_, o) => expr(o, f),
            Expr::Update { target, .. } => expr(target, f),
            Expr::Index(a, b) | Expr::Binary(_, a, b) | Expr::Assign(_, a, b) => {
                expr(a, f);
                expr(b, f);
            }
            Expr::Cond(a, b, c) => {
                expr(a, f);
                expr(b, f);
                expr(c, f);
            }
            _ => {}
        }
    }
    fn stmt<'a>(s: &'a Stmt, f: &mut dyn FnMut(&'a [Expr])) {
        match &s.kind {
            StmtKind::Var(decls) => decls
                .iter()
                .filter_map(|(_, e)| e.as_ref())
                .for_each(|e| expr(e, f)),
            StmtKind::Expr(e) | StmtKind::Throw(e) | StmtKind::Return(Some(e)) => expr(e, f),
            StmtKind::If(t, a, b) => {
                expr(t, f);
                stmt(a, f);
                if let Some(b) = b {
                    stmt(b, f);
                }
            }
            StmtKind::Block(b) => b.iter().for_each(|s| stmt(s, f)),
            StmtKind::For {
                init,
                test,
                update,
                body,
            } => {
                if let Some(i) = init {
                    stmt(i, f);
                }
                if let Some(t) = test {
                    expr(t, f);
                }
                if let Some(u) = update {
                    expr(u, f);
                }
                stmt(body, f);
            }
            StmtKind::ForIn { obj, body, .. } => {
                expr(obj, f);
                stmt(body, f);
            }
            StmtKind::While(t, b) | StmtKind::DoWhile(b, t) => {
                expr(t, f);
                stmt(b, f);
            }
            StmtKind::Switch(d, cases) => {
                expr(d, f);
                for (t, b) in cases {
                    if let Some(t) = t {
                        expr(t, f);
                    }
                    b.iter().for_each(|s| stmt(s, f));
                }
            }
            StmtKind::Try {
                block,
                catch,
                finally,
            } => {
                block.iter().for_each(|s| stmt(s, f));
                if let Some((_, c)) = catch {
                    c.iter().for_each(|s| stmt(s, f));
                }
                if let Some(fin) = finally {
                    fin.iter().for_each(|s| stmt(s, f));
                }
            }
            _ => {}
        }
    }
    stmts.iter().for_each(|s| stmt(s, f));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Result<Value, String> {
        let script = parse(src)?;
        Interp::new().run_module(&script, Value::Undef, 1_000_000)
    }

    fn num(v: &Value, key: &str) -> f64 {
        v.prop(key).and_then(|x| x.as_num()).unwrap_or(f64::NAN)
    }

    #[test]
    fn numbers_names_as_the_generated_modules_do() {
        let m = run("define([], function(m) {\nvar exports = {};\nvar val = 0;\nexports.A = val++;\nexports.B = val++;\n\
             val = exports.C = exports.A; val++;\nexports.D = val++;\nexports.MAX = exports.T_MAX = val++;\nreturn exports;\n});")
        .unwrap();
        assert_eq!(
            [
                num(&m, "A"),
                num(&m, "B"),
                num(&m, "C"),
                num(&m, "D"),
                num(&m, "MAX")
            ],
            [0.0, 1.0, 0.0, 1.0, 2.0]
        );
    }

    #[test]
    fn bit_operators_follow_int32() {
        let m = run("define(function(){ var e = {}; e.h = 1 << 31; e.m = 0x60000000 | e.h; e.u = e.m >>> 0; e.n = ~0; return e; })")
            .unwrap();
        assert_eq!(num(&m, "h"), -2147483648.0);
        assert_eq!(num(&m, "m"), -536870912.0);
        assert_eq!(num(&m, "u"), 3758096384.0);
        assert_eq!(num(&m, "n"), -1.0);
    }

    #[test]
    fn functions_switch_and_loops() {
        let script = parse(
            "define(function(){ var e = {}; e.f = function (x) { switch (x) { case 1: case 2: return 10; case 3: return 7; default: return -1; } };\n\
             e.g = function (n) { var s = 0; for (var i = 0; i < n; ++i) { if (i == 2) continue; s += i; } return s; }\n\
             return e; })",
        )
        .unwrap();
        let mut it = Interp::new();
        let m = it.run_module(&script, Value::Undef, 1_000_000).unwrap();
        let f = m.prop("f").unwrap();
        let g = m.prop("g").unwrap();
        assert_eq!(
            it.call_fn(&f, vec![Value::Num(2.0)], 1000)
                .unwrap()
                .as_num(),
            Some(10.0)
        );
        assert_eq!(
            it.call_fn(&f, vec![Value::Num(9.0)], 1000)
                .unwrap()
                .as_num(),
            Some(-1.0)
        );
        assert_eq!(
            it.call_fn(&g, vec![Value::Num(5.0)], 1000)
                .unwrap()
                .as_num(),
            Some(8.0)
        );
    }

    #[test]
    fn reports_runaway_and_broken_scripts() {
        assert!(run("define(function(){ while (true) {} })")
            .unwrap_err()
            .contains("too long"));
        assert!(run("define(function(){ throw new Error(\"boom\") })")
            .unwrap_err()
            .contains("boom"));
        assert!(run("define([], function(m) { return m.X; })")
            .unwrap_err()
            .contains("Cannot read"));
        assert!(run("define(function(){ var x = ; })").is_err());
        assert!(run("var x = 1;").unwrap_err().contains("defines no module"));
    }
}
