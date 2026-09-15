//! @spec atomicjs-profiling#conditional-else
//! AST types produced by `parser` — see spec/proposals/ATOMIC_JS_SPIKE.md
//! §5.2. Consumed as compiler input starting in step 3 (§8).

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Less,
}

/// `name: None` for a function *expression* (e.g. the anonymous closure
/// `makeCounter` returns); `Some` for a function *declaration*.
#[derive(Debug, Clone)]
pub struct FunctionDecl {
    pub name: Option<String>,
    pub params: Vec<String>,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone)]
pub enum Expr {
    Number(f64),
    Identifier(String),
    Assign {
        target: Box<Expr>,
        value: Box<Expr>,
    },
    CompoundAssign {
        op: BinOp,
        target: Box<Expr>,
        value: Box<Expr>,
    },
    /// Both `++x` (prefix: true) and `x++` (prefix: false) compile
    /// identically (§5.3's compiler note) — none of the five reference
    /// programs observe postfix increment's old-value result, only prefix's
    /// new-value result (`return ++count;`), so this is a documented,
    /// honest scope cut rather than full postfix semantics.
    Increment {
        target: Box<Expr>,
        prefix: bool,
    },
    Binary {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    Member {
        object: Box<Expr>,
        property: String,
    },
    ObjectLiteral(Vec<(String, Expr)>),
    FunctionExpr(FunctionDecl),
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Expr(Expr),
    Let {
        name: String,
        value: Expr,
    },
    Const {
        name: String,
        value: Expr,
    },
    /// A named function declaration (`FunctionDecl.name` is always `Some`
    /// here — parser guarantees this, see `parse_function_decl`).
    Function(FunctionDecl),
    For {
        init: Box<Stmt>,
        cond: Expr,
        update: Expr,
        body: Vec<Stmt>,
    },
    If {
        cond: Expr,
        then_branch: Vec<Stmt>,
        else_branch: Vec<Stmt>,
    },
    Return(Option<Expr>),
    Block(Vec<Stmt>),
}
