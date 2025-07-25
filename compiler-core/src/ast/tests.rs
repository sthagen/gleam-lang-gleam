use std::sync::Arc;

use camino::Utf8PathBuf;
use ecow::EcoString;

use crate::analyse::TargetSupport;
use crate::build::{ExpressionPosition, Origin, Target};
use crate::config::PackageConfig;
use crate::line_numbers::LineNumbers;
use crate::type_::error::{VariableDeclaration, VariableOrigin, VariableSyntax};
use crate::type_::expression::{FunctionDefinition, Purity};
use crate::type_::{Deprecation, PRELUDE_MODULE_NAME, Problems};
use crate::warning::WarningEmitter;
use crate::{
    ast::{SrcSpan, TypedExpr},
    build::Located,
    type_::{
        self, AccessorsMap, EnvironmentArguments, ExprTyper, FieldMap, ModuleValueConstructor,
        RecordAccessor, Type, ValueConstructor, ValueConstructorVariant,
    },
    uid::UniqueIdGenerator,
    warning::TypeWarningEmitter,
};

use super::{Publicity, Statement, TypedModule, TypedStatement};

fn compile_module(src: &str) -> TypedModule {
    use crate::type_::build_prelude;
    let parsed =
        crate::parse::parse_module(Utf8PathBuf::from("test/path"), src, &WarningEmitter::null())
            .expect("syntax error");
    let ast = parsed.module;
    let ids = UniqueIdGenerator::new();
    let mut config = PackageConfig::default();
    config.name = "thepackage".into();
    let mut modules = im::HashMap::new();
    // DUPE: preludeinsertion
    // TODO: Currently we do this here and also in the tests. It would be better
    // to have one place where we create all this required state for use in each
    // place.
    let _ = modules.insert(PRELUDE_MODULE_NAME.into(), build_prelude(&ids));
    let line_numbers = LineNumbers::new(src);
    let mut config = PackageConfig::default();
    config.name = "thepackage".into();

    crate::analyse::ModuleAnalyzerConstructor::<()> {
        target: Target::Erlang,
        ids: &ids,
        origin: Origin::Src,
        importable_modules: &modules,
        warnings: &TypeWarningEmitter::null(),
        direct_dependencies: &std::collections::HashMap::new(),
        target_support: TargetSupport::Enforced,
        package_config: &config,
    }
    .infer_module(ast, line_numbers, "".into())
    .expect("should successfully infer")
}

fn get_bare_expression(statement: &TypedStatement) -> &TypedExpr {
    match statement {
        Statement::Expression(expression) => expression,
        Statement::Use(_) | Statement::Assignment(_) | Statement::Assert(_) => {
            panic!("Expected expression, got {statement:?}")
        }
    }
}

fn compile_expression(src: &str) -> TypedStatement {
    let ast = crate::parse::parse_statement_sequence(src).expect("syntax error");

    let mut modules = im::HashMap::new();
    let ids = UniqueIdGenerator::new();
    // DUPE: preludeinsertion
    // TODO: Currently we do this here and also in the tests. It would be better
    // to have one place where we create all this required state for use in each
    // place.
    let _ = modules.insert(PRELUDE_MODULE_NAME.into(), type_::build_prelude(&ids));
    let mut environment = EnvironmentArguments {
        ids,
        current_package: "thepackage".into(),
        gleam_version: None,
        current_module: "mymod".into(),
        target: Target::Erlang,
        importable_modules: &modules,
        target_support: TargetSupport::Enforced,
        current_origin: Origin::Src,
    }
    .build();

    // Insert a cat record to use in the tests
    let cat_type = Arc::new(Type::Named {
        publicity: Publicity::Public,
        package: "mypackage".into(),
        module: "mymod".into(),
        name: "Cat".into(),
        arguments: vec![],
        inferred_variant: None,
    });
    let variant = ValueConstructorVariant::Record {
        documentation: Some("wibble".into()),
        variants_count: 1,
        name: "Cat".into(),
        arity: 2,
        location: SrcSpan { start: 12, end: 15 },
        field_map: Some(FieldMap {
            arity: 2,
            fields: [("name".into(), 0), ("age".into(), 1)].into(),
        }),
        module: "mymod".into(),
        variant_index: 0,
    };
    environment.insert_variable(
        "Cat".into(),
        variant,
        type_::fn_(vec![type_::string(), type_::int()], cat_type.clone()),
        Publicity::Public,
        Deprecation::NotDeprecated,
    );

    let accessors = [
        (
            "name".into(),
            RecordAccessor {
                index: 0,
                label: "name".into(),
                type_: type_::string(),
            },
        ),
        (
            "age".into(),
            RecordAccessor {
                index: 1,
                label: "age".into(),
                type_: type_::int(),
            },
        ),
    ];

    environment.insert_accessors(
        "Cat".into(),
        AccessorsMap {
            publicity: Publicity::Public,
            type_: cat_type,
            shared_accessors: accessors.clone().into(),
            variant_specific_accessors: vec![accessors.into()],
        },
    );
    let mut problems = Problems::new();
    ExprTyper::new(
        &mut environment,
        FunctionDefinition {
            has_body: true,
            has_erlang_external: false,
            has_javascript_external: false,
        },
        &mut problems,
    )
    .infer_statements(ast)
    .first()
    .clone()
}

#[test]
fn find_node_todo() {
    let statement = compile_expression(r#" todo "#);
    let expr = get_bare_expression(&statement);
    assert_eq!(expr.find_node(0), None);
    assert_eq!(
        expr.find_node(1),
        Some(Located::Expression {
            expression: expr,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        expr.find_node(4),
        Some(Located::Expression {
            expression: expr,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        expr.find_node(5),
        Some(Located::Expression {
            expression: expr,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(expr.find_node(6), None);
}

#[test]
fn find_node_todo_with_string() {
    let statement = compile_expression(r#" todo as "ok" "#);
    let expr = get_bare_expression(&statement);
    let message = TypedExpr::String {
        location: SrcSpan { start: 9, end: 13 },
        type_: type_::string(),
        value: "ok".into(),
    };

    assert_eq!(expr.find_node(0), None);
    assert_eq!(
        expr.find_node(1),
        Some(Located::Expression {
            expression: expr,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        expr.find_node(12),
        Some(Located::Expression {
            expression: &message,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        expr.find_node(13),
        Some(Located::Expression {
            expression: &message,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(expr.find_node(14), None);
}

#[test]
fn find_node_string() {
    let statement = compile_expression(r#" "ok" "#);
    let expr = get_bare_expression(&statement);
    assert_eq!(expr.find_node(0), None);
    assert_eq!(
        expr.find_node(1),
        Some(Located::Expression {
            expression: expr,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        expr.find_node(4),
        Some(Located::Expression {
            expression: expr,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        expr.find_node(5),
        Some(Located::Expression {
            expression: expr,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(expr.find_node(6), None);
}

#[test]
fn find_node_float() {
    let statement = compile_expression(r#" 1.02 "#);
    let expr = get_bare_expression(&statement);
    assert_eq!(expr.find_node(0), None);
    assert_eq!(
        expr.find_node(1),
        Some(Located::Expression {
            expression: expr,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        expr.find_node(4),
        Some(Located::Expression {
            expression: expr,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        expr.find_node(5),
        Some(Located::Expression {
            expression: expr,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(expr.find_node(6), None);
}

#[test]
fn find_node_int() {
    let statement = compile_expression(r#" 1302 "#);
    let expr = get_bare_expression(&statement);
    assert_eq!(expr.find_node(0), None);
    assert_eq!(
        expr.find_node(1),
        Some(Located::Expression {
            expression: expr,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        expr.find_node(4),
        Some(Located::Expression {
            expression: expr,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        expr.find_node(5),
        Some(Located::Expression {
            expression: expr,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(expr.find_node(6), None);
}

#[test]
fn find_node_var() {
    let statement = compile_expression(
        r#"{let wibble = 1
wibble}"#,
    );
    let expr = get_bare_expression(&statement);

    let int1 = TypedExpr::Int {
        location: SrcSpan { start: 14, end: 15 },
        value: "1".into(),
        int_value: 1.into(),
        type_: type_::int(),
    };

    let var = TypedExpr::Var {
        location: SrcSpan { start: 16, end: 22 },
        constructor: ValueConstructor {
            deprecation: Deprecation::NotDeprecated,
            publicity: Publicity::Private,
            variant: ValueConstructorVariant::LocalVariable {
                location: SrcSpan { start: 5, end: 11 },
                origin: VariableOrigin {
                    syntax: VariableSyntax::Variable("wibble".into()),
                    declaration: VariableDeclaration::LetPattern,
                },
            },
            type_: type_::int(),
        },
        name: "wibble".into(),
    };

    assert_eq!(
        expr.find_node(15),
        Some(Located::Expression {
            expression: &int1,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        expr.find_node(16),
        Some(Located::Expression {
            expression: &var,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        expr.find_node(21),
        Some(Located::Expression {
            expression: &var,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        expr.find_node(22),
        Some(Located::Expression {
            expression: &var,
            position: ExpressionPosition::Expression
        })
    );
}

#[test]
fn find_node_sequence() {
    let block = compile_expression(r#"{ 1 2 3 }"#);
    assert!(block.find_node(0).is_none());
    assert!(block.find_node(1).is_none());
    assert!(block.find_node(2).is_some());
    assert!(block.find_node(3).is_some());
    assert!(block.find_node(4).is_some());
    assert!(block.find_node(5).is_some());
    assert!(block.find_node(6).is_some());
    assert!(block.find_node(7).is_some());
}

#[test]
fn find_node_list() {
    let statement = compile_expression(r#"[1, 2, 3]"#);
    let list = get_bare_expression(&statement);

    let int1 = TypedExpr::Int {
        location: SrcSpan { start: 1, end: 2 },
        type_: type_::int(),
        value: "1".into(),
        int_value: 1.into(),
    };
    let int2 = TypedExpr::Int {
        location: SrcSpan { start: 4, end: 5 },
        type_: type_::int(),
        value: "2".into(),
        int_value: 2.into(),
    };
    let int3 = TypedExpr::Int {
        location: SrcSpan { start: 7, end: 8 },
        type_: type_::int(),
        value: "3".into(),
        int_value: 3.into(),
    };

    assert_eq!(
        list.find_node(0),
        Some(Located::Expression {
            expression: list,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        list.find_node(1),
        Some(Located::Expression {
            expression: &int1,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        list.find_node(2),
        Some(Located::Expression {
            expression: &int1,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        list.find_node(3),
        Some(Located::Expression {
            expression: list,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        list.find_node(4),
        Some(Located::Expression {
            expression: &int2,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        list.find_node(5),
        Some(Located::Expression {
            expression: &int2,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        list.find_node(6),
        Some(Located::Expression {
            expression: list,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        list.find_node(7),
        Some(Located::Expression {
            expression: &int3,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        list.find_node(8),
        Some(Located::Expression {
            expression: &int3,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        list.find_node(9),
        Some(Located::Expression {
            expression: list,
            position: ExpressionPosition::Expression
        })
    );
}

#[test]
fn find_node_tuple() {
    let statement = compile_expression(r#"#(1, 2, 3)"#);
    let tuple = get_bare_expression(&statement);

    let int1 = TypedExpr::Int {
        location: SrcSpan { start: 2, end: 3 },
        type_: type_::int(),
        value: "1".into(),
        int_value: 1.into(),
    };
    let int2 = TypedExpr::Int {
        location: SrcSpan { start: 5, end: 6 },
        type_: type_::int(),
        value: "2".into(),
        int_value: 2.into(),
    };
    let int3 = TypedExpr::Int {
        location: SrcSpan { start: 8, end: 9 },
        type_: type_::int(),
        value: "3".into(),
        int_value: 3.into(),
    };

    assert_eq!(
        tuple.find_node(0),
        Some(Located::Expression {
            expression: tuple,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        tuple.find_node(1),
        Some(Located::Expression {
            expression: tuple,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        tuple.find_node(2),
        Some(Located::Expression {
            expression: &int1,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        tuple.find_node(3),
        Some(Located::Expression {
            expression: &int1,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        tuple.find_node(4),
        Some(Located::Expression {
            expression: tuple,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        tuple.find_node(5),
        Some(Located::Expression {
            expression: &int2,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        tuple.find_node(6),
        Some(Located::Expression {
            expression: &int2,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        tuple.find_node(7),
        Some(Located::Expression {
            expression: tuple,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        tuple.find_node(8),
        Some(Located::Expression {
            expression: &int3,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        tuple.find_node(9),
        Some(Located::Expression {
            expression: &int3,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        tuple.find_node(10),
        Some(Located::Expression {
            expression: tuple,
            position: ExpressionPosition::Expression
        })
    );
}

#[test]
fn find_node_binop() {
    let statement = compile_expression(r#"1 + 2"#);
    let expr = get_bare_expression(&statement);
    assert!(expr.find_node(0).is_some());
    assert!(expr.find_node(1).is_some());
    assert!(expr.find_node(2).is_none());
    assert!(expr.find_node(3).is_none());
    assert!(expr.find_node(4).is_some());
    assert!(expr.find_node(5).is_some());
}

#[test]
fn find_node_tuple_index() {
    let statement = compile_expression(r#"#(1).0"#);
    let expr = get_bare_expression(&statement);

    let int = TypedExpr::Int {
        location: SrcSpan { start: 2, end: 3 },
        value: "1".into(),
        int_value: 1.into(),
        type_: type_::int(),
    };

    assert_eq!(
        expr.find_node(2),
        Some(Located::Expression {
            expression: &int,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        expr.find_node(5),
        Some(Located::Expression {
            expression: expr,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        expr.find_node(6),
        Some(Located::Expression {
            expression: expr,
            position: ExpressionPosition::Expression
        })
    );
}

#[test]
fn find_node_module_select() {
    let expr = TypedExpr::ModuleSelect {
        location: SrcSpan { start: 1, end: 4 },
        field_start: 2,
        type_: type_::int(),
        label: "label".into(),
        module_name: "name".into(),
        module_alias: "alias".into(),
        constructor: ModuleValueConstructor::Fn {
            module: "module".into(),
            name: "function".into(),
            external_erlang: None,
            external_javascript: None,
            location: SrcSpan { start: 1, end: 55 },
            documentation: None,
            field_map: None,
            purity: Purity::Pure,
        },
    };

    assert_eq!(expr.find_node(0), None);
    assert_eq!(
        expr.find_node(1),
        Some(Located::ModuleName {
            location: SrcSpan::new(1, 1),
            name: &"name".into(),
            layer: super::Layer::Value
        })
    );
    assert_eq!(
        expr.find_node(2),
        Some(Located::Expression {
            expression: &expr,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        expr.find_node(3),
        Some(Located::Expression {
            expression: &expr,
            position: ExpressionPosition::Expression
        })
    );
}

#[test]
fn find_node_fn() {
    let statement = compile_expression("fn() { 1 }");
    let expr = get_bare_expression(&statement);

    let int = TypedExpr::Int {
        location: SrcSpan { start: 7, end: 8 },
        value: "1".into(),
        int_value: 1.into(),
        type_: type_::int(),
    };

    assert_eq!(
        expr.find_node(0),
        Some(Located::Expression {
            expression: expr,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        expr.find_node(6),
        Some(Located::Expression {
            expression: expr,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        expr.find_node(7),
        Some(Located::Expression {
            expression: &int,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        expr.find_node(8),
        Some(Located::Expression {
            expression: &int,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        expr.find_node(9),
        Some(Located::Expression {
            expression: expr,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        expr.find_node(10),
        Some(Located::Expression {
            expression: expr,
            position: ExpressionPosition::Expression
        })
    );
}

#[test]
fn find_node_call() {
    let statement = compile_expression("fn(_, _) { 1 }(1, 2)");
    let expr = get_bare_expression(&statement);

    let return_ = TypedExpr::Int {
        location: SrcSpan { start: 11, end: 12 },
        value: "1".into(),
        int_value: 1.into(),
        type_: type_::int(),
    };

    let arg1 = TypedExpr::Int {
        location: SrcSpan { start: 15, end: 16 },
        value: "1".into(),
        int_value: 1.into(),
        type_: type_::int(),
    };

    let arg2 = TypedExpr::Int {
        location: SrcSpan { start: 18, end: 19 },
        value: "2".into(),
        int_value: 2.into(),
        type_: type_::int(),
    };

    let TypedExpr::Call {
        fun: called_function,
        arguments: function_arguments,
        ..
    } = expr
    else {
        panic!("Expression was not a function call");
    };

    assert_eq!(
        expr.find_node(11),
        Some(Located::Expression {
            expression: &return_,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        expr.find_node(15),
        Some(Located::Expression {
            expression: &arg1,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        expr.find_node(16),
        Some(Located::Expression {
            expression: &arg1,
            position: ExpressionPosition::ArgumentOrLabel {
                called_function,
                function_arguments
            }
        })
    );
    assert_eq!(
        expr.find_node(17),
        Some(Located::Expression {
            expression: expr,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        expr.find_node(18),
        Some(Located::Expression {
            expression: &arg2,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        expr.find_node(19),
        Some(Located::Expression {
            expression: &arg2,
            position: ExpressionPosition::ArgumentOrLabel {
                called_function,
                function_arguments
            }
        })
    );
    assert_eq!(
        expr.find_node(20),
        Some(Located::Expression {
            expression: expr,
            position: ExpressionPosition::Expression
        })
    );
}

#[test]
fn find_node_record_access() {
    let statement = compile_expression(r#"Cat("Nubi", 3).name"#);
    let access = get_bare_expression(&statement);

    let string = TypedExpr::String {
        location: SrcSpan { start: 4, end: 10 },
        value: "Nubi".into(),
        type_: type_::string(),
    };

    let int = TypedExpr::Int {
        location: SrcSpan { start: 12, end: 13 },
        value: "3".into(),
        int_value: 3.into(),
        type_: type_::int(),
    };

    assert_eq!(
        access.find_node(4),
        Some(Located::Expression {
            expression: &string,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        access.find_node(9),
        Some(Located::Expression {
            expression: &string,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        access.find_node(12),
        Some(Located::Expression {
            expression: &int,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        access.find_node(15),
        Some(Located::Expression {
            expression: access,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        access.find_node(18),
        Some(Located::Expression {
            expression: access,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        access.find_node(19),
        Some(Located::Expression {
            expression: access,
            position: ExpressionPosition::Expression
        })
    );
}

#[test]
fn find_node_record_update() {
    let statement = compile_expression(r#"Cat(..Cat("Nubi", 3), age: 4)"#);
    let update = get_bare_expression(&statement);

    let cat = TypedExpr::Var {
        location: SrcSpan { start: 0, end: 3 },
        constructor: ValueConstructor {
            publicity: Publicity::Public,
            deprecation: Deprecation::NotDeprecated,
            variant: ValueConstructorVariant::Record {
                name: "Cat".into(),
                arity: 2,
                field_map: Some(FieldMap {
                    arity: 2,
                    fields: [(EcoString::from("age"), 1), (EcoString::from("name"), 0)].into(),
                }),
                location: SrcSpan { start: 12, end: 15 },
                module: "mymod".into(),
                variants_count: 1,
                variant_index: 0,
                documentation: Some("wibble".into()),
            },
            type_: type_::fn_(
                vec![type_::string(), type_::int()],
                type_::named("mypackage", "mymod", "Cat", Publicity::Public, vec![]),
            ),
        },
        name: "Cat".into(),
    };

    let int = TypedExpr::Int {
        location: SrcSpan { start: 27, end: 28 },
        value: "4".into(),
        int_value: 4.into(),
        type_: type_::int(),
    };

    assert_eq!(
        update.find_node(0),
        Some(Located::Expression {
            expression: &cat,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        update.find_node(3),
        Some(Located::Expression {
            expression: &cat,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        update.find_node(27),
        Some(Located::Expression {
            expression: &int,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        update.find_node(28),
        Some(Located::Expression {
            expression: &int,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        update.find_node(29),
        Some(Located::Expression {
            expression: update,
            position: ExpressionPosition::Expression
        })
    );
}

#[test]
fn find_node_case() {
    let statement = compile_expression(
        r#"
case 1, 2 {
  _, _ -> 3
}
"#,
    );
    let case = get_bare_expression(&statement);

    let int1 = TypedExpr::Int {
        location: SrcSpan { start: 6, end: 7 },
        value: "1".into(),
        int_value: 1.into(),
        type_: type_::int(),
    };

    let int2 = TypedExpr::Int {
        location: SrcSpan { start: 9, end: 10 },
        value: "2".into(),
        int_value: 2.into(),
        type_: type_::int(),
    };

    let int3 = TypedExpr::Int {
        location: SrcSpan { start: 23, end: 24 },
        value: "3".into(),
        int_value: 3.into(),
        type_: type_::int(),
    };

    assert_eq!(
        case.find_node(1),
        Some(Located::Expression {
            expression: case,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        case.find_node(6),
        Some(Located::Expression {
            expression: &int1,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        case.find_node(9),
        Some(Located::Expression {
            expression: &int2,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        case.find_node(23),
        Some(Located::Expression {
            expression: &int3,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        case.find_node(25),
        Some(Located::Expression {
            expression: case,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        case.find_node(26),
        Some(Located::Expression {
            expression: case,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(case.find_node(27), None);
}

#[test]
fn find_node_bool() {
    let statement = compile_expression(r#"!True"#);
    let negate = get_bare_expression(&statement);

    let bool = TypedExpr::Var {
        location: SrcSpan { start: 1, end: 5 },
        constructor: ValueConstructor {
            deprecation: Deprecation::NotDeprecated,
            publicity: Publicity::Public,
            variant: ValueConstructorVariant::Record {
                documentation: None,
                variants_count: 2,
                name: "True".into(),
                arity: 0,
                field_map: None,
                location: SrcSpan { start: 0, end: 0 },
                module: PRELUDE_MODULE_NAME.into(),
                variant_index: 0,
            },
            type_: type_::bool_with_variant(Some(true)),
        },
        name: "True".into(),
    };

    assert_eq!(
        negate.find_node(0),
        Some(Located::Expression {
            expression: negate,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        negate.find_node(1),
        Some(Located::Expression {
            expression: &bool,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        negate.find_node(2),
        Some(Located::Expression {
            expression: &bool,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        negate.find_node(3),
        Some(Located::Expression {
            expression: &bool,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        negate.find_node(4),
        Some(Located::Expression {
            expression: &bool,
            position: ExpressionPosition::Expression
        })
    );
    assert_eq!(
        negate.find_node(5),
        Some(Located::Expression {
            expression: &bool,
            position: ExpressionPosition::Expression
        })
    );
}

#[test]
fn find_node_statement_fn() {
    let module = compile_module(
        r#"

pub fn main() {
  Nil
}

"#,
    );

    assert!(module.find_node(0).is_none());
    assert!(module.find_node(1).is_none());

    // The fn
    assert!(module.find_node(2).is_some());
    assert!(module.find_node(24).is_some());
    assert!(module.find_node(25).is_some());
    assert!(module.find_node(26).is_none());
}

#[test]
fn find_node_statement_import() {
    let module = compile_module(
        r#"
import gleam
"#,
    );

    assert!(module.find_node(0).is_none());

    // The import
    assert!(module.find_node(1).is_some());
    assert!(module.find_node(12).is_some());
    assert!(module.find_node(13).is_some());
    assert!(module.find_node(14).is_none());
}

#[test]
fn find_node_use() {
    let use_ = compile_expression(
        r#"
use x <- fn(f) { f(1) }
124
"#,
    );

    assert!(use_.find_node(0).is_none());
    assert!(use_.find_node(1).is_some()); // The use
    assert!(use_.find_node(23).is_some());
    assert!(use_.find_node(26).is_some()); // The int
}
