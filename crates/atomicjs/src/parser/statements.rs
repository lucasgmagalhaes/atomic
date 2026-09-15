//! @spec atomicjs-profiling#conditional-else
//! Statement grammar parsing kept separate from expression precedence parsing.

use super::*;

impl Parser {
    pub(super) fn parse_statement(&mut self) -> Result<Stmt, ParseError> {
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
        Ok(Stmt::Function(self.parse_function_common(true)?))
    }

    /// Parses everything after `function`; declarations use a name while
    /// expression-level anonymous functions pass `false` from `parse_primary`.
    pub(super) fn parse_function_common(
        &mut self,
        named: bool,
    ) -> Result<FunctionDecl, ParseError> {
        let name = named.then(|| self.expect_identifier()).transpose()?;
        self.expect(&Token::LParen)?;
        let mut params = Vec::new();
        while !self.check(&Token::RParen) {
            params.push(self.expect_identifier()?);
            if !self.check(&Token::Comma) {
                break;
            }
            self.advance();
        }
        self.expect(&Token::RParen)?;
        self.expect(&Token::LBrace)?;
        let body = self.parse_statement_list()?;
        Ok(FunctionDecl { name, params, body })
    }

    fn parse_for(&mut self) -> Result<Stmt, ParseError> {
        self.expect(&Token::For)?;
        self.expect(&Token::LParen)?;
        let init = self.parse_let()?;
        let cond = self.parse_expr()?;
        self.expect(&Token::Semicolon)?;
        let update = self.parse_expr()?;
        self.expect(&Token::RParen)?;
        self.expect(&Token::LBrace)?;
        let body = self.parse_statement_list()?;
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
        let body = self.parse_statement_list()?;
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
        let then_branch = self.parse_statement_list()?;
        let else_branch = if self.check(&Token::Else) {
            self.advance();
            self.expect(&Token::LBrace)?;
            self.parse_statement_list()?
        } else {
            Vec::new()
        };
        Ok(Stmt::If {
            cond,
            then_branch,
            else_branch,
        })
    }

    fn parse_block(&mut self) -> Result<Stmt, ParseError> {
        self.expect(&Token::LBrace)?;
        Ok(Stmt::Block(self.parse_statement_list()?))
    }

    fn parse_statement_list(&mut self) -> Result<Vec<Stmt>, ParseError> {
        let mut statements = Vec::new();
        while !self.check(&Token::RBrace) {
            statements.push(self.parse_statement()?);
        }
        self.expect(&Token::RBrace)?;
        Ok(statements)
    }
}
