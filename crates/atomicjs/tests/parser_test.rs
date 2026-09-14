use atomicjs::ast::{BinOp, Expr, Stmt};
use atomicjs::lexer::tokenize;
use atomicjs::parser::parse;

mod common;

fn parse_source(source: &str) -> Vec<Stmt> {
    parse(tokenize(source).unwrap()).unwrap()
}

#[test]
fn parses_let_statement_with_number_value() {
    let program = parse_source("let total = 0;");
    assert_eq!(program.len(), 1);
    match &program[0] {
        Stmt::Let { name, value } => {
            assert_eq!(name, "total");
            assert!(matches!(value, Expr::Number(n) if *n == 0.0));
        }
        other => panic!("expected Stmt::Let, got {other:?}"),
    }
}

#[test]
fn parses_const_with_object_literal() {
    let program = parse_source("const player = { level: 10, damage: 20 };");
    match &program[0] {
        Stmt::Const { name, value } => {
            assert_eq!(name, "player");
            match value {
                Expr::ObjectLiteral(props) => {
                    assert_eq!(props.len(), 2);
                    assert_eq!(props[0].0, "level");
                    assert!(matches!(props[0].1, Expr::Number(n) if n == 10.0));
                    assert_eq!(props[1].0, "damage");
                    assert!(matches!(props[1].1, Expr::Number(n) if n == 20.0));
                }
                other => panic!("expected ObjectLiteral, got {other:?}"),
            }
        }
        other => panic!("expected Stmt::Const, got {other:?}"),
    }
}

#[test]
fn parses_member_access_and_binary_add() {
    let program = parse_source("player.damage + player.level;");
    match &program[0] {
        Stmt::Expr(Expr::Binary { op, left, right }) => {
            assert_eq!(*op, BinOp::Add);
            assert!(matches!(**left, Expr::Member { ref property, .. } if property == "damage"));
            assert!(matches!(**right, Expr::Member { ref property, .. } if property == "level"));
        }
        other => panic!("expected a top-level Binary(Add) expression, got {other:?}"),
    }
}

#[test]
fn parses_subtraction_at_additive_precedence() {
    let program = parse_source("10 - 3 + 1;");
    match &program[0] {
        Stmt::Expr(Expr::Binary {
            op: BinOp::Add,
            left,
            right,
        }) => {
            assert!(matches!(**left, Expr::Binary { op: BinOp::Sub, .. }));
            assert!(matches!(**right, Expr::Number(n) if n == 1.0));
        }
        other => panic!("expected left-associative Add/Sub expression, got {other:?}"),
    }
}

#[test]
fn parses_function_declaration_with_params_and_body() {
    let program = parse_source(
        r#"
        function sum(n) {
            return n;
        }
        "#,
    );
    match &program[0] {
        Stmt::Function(decl) => {
            assert_eq!(decl.name.as_deref(), Some("sum"));
            assert_eq!(decl.params, vec!["n".to_string()]);
            assert_eq!(decl.body.len(), 1);
            assert!(
                matches!(decl.body[0], Stmt::Return(Some(Expr::Identifier(ref n))) if n == "n")
            );
        }
        other => panic!("expected Stmt::Function, got {other:?}"),
    }
}

#[test]
fn parses_for_loop_structure() {
    let program = parse_source(
        r#"
        for (let i = 0; i < n; i++) {
            total += i;
        }
        "#,
    );
    match &program[0] {
        Stmt::For {
            init,
            cond,
            update,
            body,
        } => {
            assert!(matches!(**init, Stmt::Let { ref name, .. } if name == "i"));
            assert!(matches!(
                cond,
                Expr::Binary {
                    op: BinOp::Less,
                    ..
                }
            ));
            assert!(matches!(update, Expr::Increment { prefix: false, .. }));
            assert_eq!(body.len(), 1);
        }
        other => panic!("expected Stmt::For, got {other:?}"),
    }
}

#[test]
fn parses_return_with_and_without_value() {
    let program = parse_source("function f() { return 1; } function g() { return; }");
    match &program[0] {
        Stmt::Function(decl) => {
            assert!(matches!(decl.body[0], Stmt::Return(Some(Expr::Number(n))) if n == 1.0));
        }
        other => panic!("expected Stmt::Function, got {other:?}"),
    }
    match &program[1] {
        Stmt::Function(decl) => {
            assert!(matches!(decl.body[0], Stmt::Return(None)));
        }
        other => panic!("expected Stmt::Function, got {other:?}"),
    }
}

#[test]
fn parses_compound_assignment() {
    let program = parse_source("total += i;");
    match &program[0] {
        Stmt::Expr(Expr::CompoundAssign { op, target, value }) => {
            assert_eq!(*op, BinOp::Add);
            assert!(matches!(**target, Expr::Identifier(ref n) if n == "total"));
            assert!(matches!(**value, Expr::Identifier(ref n) if n == "i"));
        }
        other => panic!("expected CompoundAssign, got {other:?}"),
    }
}

#[test]
fn parses_prefix_increment() {
    let program = parse_source("++count;");
    match &program[0] {
        Stmt::Expr(Expr::Increment { target, prefix }) => {
            assert!(prefix);
            assert!(matches!(**target, Expr::Identifier(ref n) if n == "count"));
        }
        other => panic!("expected prefix Increment, got {other:?}"),
    }
}

#[test]
fn parses_postfix_increment() {
    let program = parse_source("i++;");
    match &program[0] {
        Stmt::Expr(Expr::Increment { target, prefix }) => {
            assert!(!prefix);
            assert!(matches!(**target, Expr::Identifier(ref n) if n == "i"));
        }
        other => panic!("expected postfix Increment, got {other:?}"),
    }
}

#[test]
fn parses_function_call_with_arguments() {
    let program = parse_source("sum(1000000);");
    match &program[0] {
        Stmt::Expr(Expr::Call { callee, args }) => {
            assert!(matches!(**callee, Expr::Identifier(ref n) if n == "sum"));
            assert_eq!(args.len(), 1);
            assert!(matches!(args[0], Expr::Number(n) if n == 1000000.0));
        }
        other => panic!("expected Call, got {other:?}"),
    }
}

#[test]
fn parses_call_with_zero_arguments() {
    let program = parse_source("counter();");
    match &program[0] {
        Stmt::Expr(Expr::Call { args, .. }) => assert!(args.is_empty()),
        other => panic!("expected Call, got {other:?}"),
    }
}

#[test]
fn parses_anonymous_function_expression_as_return_value() {
    let program = parse_source(
        r#"
        function makeCounter() {
            return function () {
                return 1;
            };
        }
        "#,
    );
    match &program[0] {
        Stmt::Function(decl) => match &decl.body[0] {
            Stmt::Return(Some(Expr::FunctionExpr(inner))) => {
                assert!(inner.name.is_none());
                assert!(inner.params.is_empty());
            }
            other => panic!("expected Return(FunctionExpr), got {other:?}"),
        },
        other => panic!("expected Stmt::Function, got {other:?}"),
    }
}

#[test]
fn parses_every_reference_program_without_error() {
    for (name, source) in common::all() {
        let tokens =
            tokenize(source).unwrap_or_else(|e| panic!("failed to tokenize {name:?}: {e:?}"));
        parse(tokens).unwrap_or_else(|e| panic!("failed to parse {name:?}: {e:?}"));
    }
}

#[test]
fn rejects_missing_semicolon() {
    let tokens = tokenize("let total = 0").unwrap();
    assert!(parse(tokens).is_err());
}

#[test]
fn rejects_unbalanced_braces() {
    let tokens = tokenize("function f() { return 1;").unwrap();
    assert!(parse(tokens).is_err());
}

#[test]
fn rejects_unexpected_token_in_expression_position() {
    let tokens = tokenize("let x = ;").unwrap();
    assert!(parse(tokens).is_err());
}
