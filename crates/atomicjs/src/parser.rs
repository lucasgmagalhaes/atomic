//! @spec atomicjs-profiling#conditional-else
//! Recursive-descent parser for the spike's minimal grammar — see
//! spec/proposals/ATOMIC_JS_SPIKE.md §5.2. Precedence, low to high:
//! assignment (`=`/`+=`) -> relational (`<`) -> additive (`+`) -> unary
//! prefix `++` -> postfix `++` -> call/member (`()`/`.`) -> primary.

use crate::ast::{BinOp, Expr, FunctionDecl, Stmt};
use crate::lexer::Token;

#[derive(Debug, PartialEq)]
pub struct ParseError(pub String);

pub fn parse(tokens: Vec<Token>) -> Result<Vec<Stmt>, ParseError> {
    let mut parser = Parser { tokens, pos: 0 };
    let mut program = Vec::new();
    while !parser.check(&Token::Eof) {
        program.push(parser.parse_statement()?);
    }
    Ok(program)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn check(&self, t: &Token) -> bool {
        self.peek() == t
    }

    fn advance(&mut self) -> Token {
        let t = self.tokens[self.pos].clone();
        self.pos += 1;
        t
    }

    fn expect(&mut self, t: &Token) -> Result<(), ParseError> {
        if self.check(t) {
            self.advance();
            Ok(())
        } else {
            Err(ParseError(format!(
                "expected {t:?}, found {:?}",
                self.peek()
            )))
        }
    }

    fn expect_identifier(&mut self) -> Result<String, ParseError> {
        match self.advance() {
            Token::Identifier(name) => Ok(name),
            other => Err(ParseError(format!("expected identifier, found {other:?}"))),
        }
    }

    // ---- statements ----

    fn parse_statement(&mut self) -> Result<Stmt, ParseError> {
        match self.peek() {
            Token::Let => self.parse_let(),
            Token::Const => self.parse_const(),
            Token::Function => self.parse_function_decl(),
            Token::For => self.parse_for(),
            Token::While => self.parse_while(),
            Token::If => self.parse_if(),
            Token::Return => self.parse_return(),
            Token::LBrace => self.parse_block(),
            _ => {
                let expr = self.parse_expr()?;
                self.expect(&Token::Semicolon)?;
                Ok(Stmt::Expr(expr))
            }
        }
    }

    fn parse_let(&mut self) -> Result<Stmt, ParseError> {
        self.expect(&Token::Let)?;
        let name = self.expect_identifier()?;
        self.expect(&Token::Assign)?;
        let value = self.parse_expr()?;
        self.expect(&Token::Semicolon)?;
        Ok(Stmt::Let { name, value })
    }

    fn parse_const(&mut self) -> Result<Stmt, ParseError> {
        self.expect(&Token::Const)?;
        let name = self.expect_identifier()?;
        self.expect(&Token::Assign)?;
        let value = self.parse_expr()?;
        self.expect(&Token::Semicolon)?;
        Ok(Stmt::Const { name, value })
    }

    fn parse_function_decl(&mut self) -> Result<Stmt, ParseError> {
        self.expect(&Token::Function)?;
        let decl = self.parse_function_common(true)?;
        Ok(Stmt::Function(decl))
    }

    /// Parses everything after the `function` keyword itself, which the
    /// caller must already have consumed (statement-level declarations via
    /// `parse_function_decl`, expression-level anonymous functions via
    /// `parse_primary`'s own `Token::Function` arm).
    fn parse_function_common(&mut self, named: bool) -> Result<FunctionDecl, ParseError> {
        let name = if named {
            Some(self.expect_identifier()?)
        } else {
            None
        };
        self.expect(&Token::LParen)?;
        let mut params = Vec::new();
        if !self.check(&Token::RParen) {
            loop {
                params.push(self.expect_identifier()?);
                if self.check(&Token::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
        }
        self.expect(&Token::RParen)?;
        self.expect(&Token::LBrace)?;
        let mut body = Vec::new();
        while !self.check(&Token::RBrace) {
            body.push(self.parse_statement()?);
        }
        self.expect(&Token::RBrace)?;
        Ok(FunctionDecl { name, params, body })
    }

    fn parse_for(&mut self) -> Result<Stmt, ParseError> {
        self.expect(&Token::For)?;
        self.expect(&Token::LParen)?;
        // Only a `let` init clause is supported — matches every reference
        // program (§4); a bare expression or `const` init would need a
        // grammar extension not currently needed.
        let init = self.parse_let()?;
        let cond = self.parse_expr()?;
        self.expect(&Token::Semicolon)?;
        let update = self.parse_expr()?;
        self.expect(&Token::RParen)?;
        self.expect(&Token::LBrace)?;
        let mut body = Vec::new();
        while !self.check(&Token::RBrace) {
            body.push(self.parse_statement()?);
        }
        self.expect(&Token::RBrace)?;
        Ok(Stmt::For {
            init: Box::new(init),
            cond,
            update,
            body,
        })
    }

    fn parse_while(&mut self) -> Result<Stmt, ParseError> {
        self.expect(&Token::While)?;
        self.expect(&Token::LParen)?;
        let cond = self.parse_expr()?;
        self.expect(&Token::RParen)?;
        self.expect(&Token::LBrace)?;
        let mut body = Vec::new();
        while !self.check(&Token::RBrace) {
            body.push(self.parse_statement()?);
        }
        self.expect(&Token::RBrace)?;
        Ok(Stmt::While { cond, body })
    }

    fn parse_return(&mut self) -> Result<Stmt, ParseError> {
        self.expect(&Token::Return)?;
        if self.check(&Token::Semicolon) {
            self.advance();
            Ok(Stmt::Return(None))
        } else {
            let expr = self.parse_expr()?;
            self.expect(&Token::Semicolon)?;
            Ok(Stmt::Return(Some(expr)))
        }
    }

    fn parse_if(&mut self) -> Result<Stmt, ParseError> {
        self.expect(&Token::If)?;
        self.expect(&Token::LParen)?;
        let cond = self.parse_expr()?;
        self.expect(&Token::RParen)?;
        self.expect(&Token::LBrace)?;
        let mut then_branch = Vec::new();
        while !self.check(&Token::RBrace) {
            then_branch.push(self.parse_statement()?);
        }
        self.expect(&Token::RBrace)?;
        let mut else_branch = Vec::new();
        if self.check(&Token::Else) {
            self.advance();
            self.expect(&Token::LBrace)?;
            while !self.check(&Token::RBrace) {
                else_branch.push(self.parse_statement()?);
            }
            self.expect(&Token::RBrace)?;
        }
        Ok(Stmt::If {
            cond,
            then_branch,
            else_branch,
        })
    }

    fn parse_block(&mut self) -> Result<Stmt, ParseError> {
        self.expect(&Token::LBrace)?;
        let mut stmts = Vec::new();
        while !self.check(&Token::RBrace) {
            stmts.push(self.parse_statement()?);
        }
        self.expect(&Token::RBrace)?;
        Ok(Stmt::Block(stmts))
    }

    // ---- expressions ----

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> Result<Expr, ParseError> {
        let left = self.parse_relational()?;
        if self.check(&Token::Assign) {
            self.advance();
            let value = self.parse_assignment()?;
            return Ok(Expr::Assign {
                target: Box::new(left),
                value: Box::new(value),
            });
        }
        if self.check(&Token::PlusAssign) || self.check(&Token::StarAssign) {
            let op = if self.check(&Token::PlusAssign) {
                BinOp::Add
            } else {
                BinOp::Mul
            };
            self.advance();
            let value = self.parse_assignment()?;
            return Ok(Expr::CompoundAssign {
                op,
                target: Box::new(left),
                value: Box::new(value),
            });
        }
        Ok(left)
    }

    fn parse_relational(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_additive()?;
        while self.check(&Token::Less) || self.check(&Token::Greater) {
            let greater = self.check(&Token::Greater);
            self.advance();
            let right = self.parse_additive()?;
            let previous = left;
            left = if greater {
                Expr::Binary {
                    op: BinOp::Less,
                    left: Box::new(right),
                    right: Box::new(previous),
                }
            } else {
                Expr::Binary {
                    op: BinOp::Less,
                    left: Box::new(previous),
                    right: Box::new(right),
                }
            };
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_multiplicative()?;
        while self.check(&Token::Plus) || self.check(&Token::Minus) {
            let op = if self.check(&Token::Plus) {
                BinOp::Add
            } else {
                BinOp::Sub
            };
            self.advance();
            let right = self.parse_multiplicative()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_unary()?;
        while self.check(&Token::Star) || self.check(&Token::Slash) || self.check(&Token::Percent) {
            let op = match self.advance() {
                Token::Star => BinOp::Mul,
                Token::Slash => BinOp::Div,
                Token::Percent => BinOp::Mod,
                _ => unreachable!(),
            };
            let right = self.parse_unary()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        if self.check(&Token::Increment) {
            self.advance();
            let target = self.parse_unary()?;
            return Ok(Expr::Increment {
                target: Box::new(target),
                prefix: true,
            });
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_call_or_member()?;
        if self.check(&Token::Increment) {
            self.advance();
            expr = Expr::Increment {
                target: Box::new(expr),
                prefix: false,
            };
        }
        Ok(expr)
    }

    fn parse_call_or_member(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_primary()?;
        loop {
            if self.check(&Token::Dot) {
                self.advance();
                let property = self.expect_identifier()?;
                expr = Expr::Member {
                    object: Box::new(expr),
                    property,
                };
            } else if self.check(&Token::LParen) {
                self.advance();
                let mut args = Vec::new();
                if !self.check(&Token::RParen) {
                    loop {
                        args.push(self.parse_expr()?);
                        if self.check(&Token::Comma) {
                            self.advance();
                        } else {
                            break;
                        }
                    }
                }
                self.expect(&Token::RParen)?;
                expr = Expr::Call {
                    callee: Box::new(expr),
                    args,
                };
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        match self.advance() {
            Token::Number(n) => Ok(Expr::Number(n)),
            Token::Identifier(name) => Ok(Expr::Identifier(name)),
            Token::LParen => {
                let expr = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(expr)
            }
            Token::LBrace => self.parse_object_literal(),
            Token::Function => {
                let decl = self.parse_function_common(false)?;
                Ok(Expr::FunctionExpr(decl))
            }
            other => Err(ParseError(format!(
                "unexpected token in expression: {other:?}"
            ))),
        }
    }

    /// Caller has already consumed the opening `{`.
    fn parse_object_literal(&mut self) -> Result<Expr, ParseError> {
        let mut props = Vec::new();
        if !self.check(&Token::RBrace) {
            loop {
                let key = self.expect_identifier()?;
                self.expect(&Token::Colon)?;
                let value = self.parse_expr()?;
                props.push((key, value));
                if self.check(&Token::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
        }
        self.expect(&Token::RBrace)?;
        Ok(Expr::ObjectLiteral(props))
    }
}
