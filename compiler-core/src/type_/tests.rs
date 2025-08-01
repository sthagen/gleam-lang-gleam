use super::*;
use crate::{
    analyse::TargetSupport,
    ast::{TypedModule, TypedStatement, UntypedExpr, UntypedModule},
    build::{Origin, Outcome, Target},
    config::{GleamVersion, PackageConfig},
    error::Error,
    type_::{build_prelude, expression::FunctionDefinition, pretty::Printer},
    uid::UniqueIdGenerator,
    warning::{TypeWarningEmitter, VectorWarningEmitterIO, WarningEmitter, WarningEmitterIO},
};
use ecow::EcoString;
use itertools::Itertools;
use pubgrub::Range;
use std::rc::Rc;
use vec1::Vec1;

use camino::Utf8PathBuf;

mod accessors;
mod assert;
mod assignments;
mod conditional_compilation;
mod custom_types;
mod dead_code_detection;
mod echo;
mod errors;
mod exhaustiveness;
mod externals;
mod functions;
mod guards;
mod imports;
mod let_assert;
mod pipes;
mod pretty;
mod target_implementations;
mod type_alias;
mod use_;
mod version_inference;
mod warnings;

#[macro_export]
macro_rules! assert_infer {
    ($src:expr, $type_:expr $(,)?) => {
        let t = $crate::type_::tests::infer($src);
        assert_eq!(($src, t), ($src, $type_.to_string()),);
    };
}

#[macro_export]
macro_rules! assert_module_infer {
    ($(($name:expr, $module_src:literal)),+, $src:literal, $module:expr $(,)?) => {
        let constructors =
            $crate::type_::tests::infer_module($src, vec![$(("thepackage", $name, $module_src)),*]);
        let expected = $crate::type_::tests::stringify_tuple_strs($module);

        assert_eq!(($src, constructors), ($src, expected));
    };

    ($src:expr, $module:expr $(,)?) => {{
        let constructors = $crate::type_::tests::infer_module($src, vec![]);
        let expected = $crate::type_::tests::stringify_tuple_strs($module);
        assert_eq!(($src, constructors), ($src, expected));
    }};
}

#[macro_export]
macro_rules! assert_js_module_infer {
    ($src:expr, $module:expr $(,)?) => {{
        let constructors = $crate::type_::tests::infer_module_with_target(
            "test_module",
            $src,
            vec![],
            $crate::build::Target::JavaScript,
        );
        let expected = $crate::type_::tests::stringify_tuple_strs($module);
        assert_eq!(($src, constructors), ($src, expected));
    }};
}

#[macro_export]
macro_rules! assert_module_error {
    ($src:expr) => {
        let error = $crate::type_::tests::module_error($src, vec![]);
        let output = format!("----- SOURCE CODE\n{}\n\n----- ERROR\n{}", $src, error);
        insta::assert_snapshot!(insta::internals::AutoName, output, $src);
    };

    ($(($name:expr, $module_src:literal)),+, $src:literal $(,)?) => {
        let error = $crate::type_::tests::module_error(
            $src,
            vec![
                $(("thepackage", $name, $module_src)),*
            ],
        );

        let mut output = String::from("----- SOURCE CODE\n");
        for (name, src) in [$(($name, $module_src)),*] {
            output.push_str(&format!("-- {name}.gleam\n{src}\n\n"));
        }
        output.push_str(&format!("-- main.gleam\n{}\n\n----- ERROR\n{error}", $src));
        insta::assert_snapshot!(insta::internals::AutoName, output, $src);
    };
}

#[macro_export]
macro_rules! assert_internal_module_error {
    ($src:expr) => {
        let error = $crate::type_::tests::internal_module_error($src, vec![]);
        let output = format!("----- SOURCE CODE\n{}\n\n----- ERROR\n{}", $src, error);
        insta::assert_snapshot!(insta::internals::AutoName, output, $src);
    };
}

#[macro_export]
macro_rules! assert_js_module_error {
    ($src:expr) => {
        let error = $crate::type_::tests::module_error_with_target(
            $src,
            vec![],
            $crate::build::Target::JavaScript,
        );
        let output = format!("----- SOURCE CODE\n{}\n\n----- ERROR\n{}", $src, error);
        insta::assert_snapshot!(insta::internals::AutoName, output, $src);
    };
}

#[macro_export]
macro_rules! assert_module_syntax_error {
    ($src:expr) => {
        let error = $crate::type_::tests::syntax_error($src);
        let output = format!("----- SOURCE CODE\n{}\n\n----- ERROR\n{}", $src, error);
        insta::assert_snapshot!(insta::internals::AutoName, output, $src);
    };
}

#[macro_export]
macro_rules! assert_error {
    ($src:expr, $error:expr $(,)?) => {
        let result = $crate::type_::tests::compile_statement_sequence($src)
            .expect_err("should infer an error");
        assert_eq!(($src, sort_options($error)), ($src, sort_options(result)),);
    };

    ($src:expr) => {
        let (error, names) = $crate::type_::tests::compile_statement_sequence($src)
            .expect_err("should infer an error");
        let error = $crate::error::Error::Type {
            names: Box::new(names),
            src: $src.into(),
            path: camino::Utf8PathBuf::from("/src/one/two.gleam"),
            errors: error,
        };
        let error_string = error.pretty_string();
        let output = format!(
            "----- SOURCE CODE\n{}\n\n----- ERROR\n{}",
            $src, error_string
        );
        insta::assert_snapshot!(insta::internals::AutoName, output, $src);
    };
}

fn get_warnings(
    src: &str,
    deps: Vec<DependencyModule<'_>>,
    target: Target,
    gleam_version: Option<Range<Version>>,
) -> Vec<crate::warning::Warning> {
    let warnings = VectorWarningEmitterIO::default();
    _ = compile_module_with_opts(
        "test_module",
        src,
        Some(Rc::new(warnings.clone())),
        deps,
        target,
        TargetSupport::NotEnforced,
        gleam_version,
    )
    .expect("Compilation should succeed");
    warnings.take().into_iter().collect_vec()
}

pub(crate) fn get_printed_warnings(
    src: &str,
    deps: Vec<DependencyModule<'_>>,
    target: Target,
    gleam_version: Option<Range<Version>>,
) -> String {
    print_warnings(get_warnings(src, deps, target, gleam_version))
}

fn print_warnings(warnings: Vec<crate::warning::Warning>) -> String {
    let mut nocolor = termcolor::Buffer::no_color();
    for warning in warnings {
        warning.pretty(&mut nocolor);
    }
    String::from_utf8(nocolor.into_inner()).expect("Error printing produced invalid utf8")
}

#[macro_export]
macro_rules! assert_warning {
    ($src:expr) => {
        let warning = $crate::type_::tests::get_printed_warnings($src, vec![], crate::build::Target::Erlang, None);
        assert!(!warning.is_empty());
        let output = format!("----- SOURCE CODE\n{}\n\n----- WARNING\n{}", $src, warning);
        insta::assert_snapshot!(insta::internals::AutoName, output, $src);
    };

    ($(($name:expr, $module_src:literal)),+, $src:literal $(,)?) => {
        let warning = $crate::type_::tests::get_printed_warnings(
            $src,
            vec![
                $(("thepackage", $name, $module_src)),*
            ],
            crate::build::Target::Erlang,
            None
        );
        assert!(!warning.is_empty());

        let mut output = String::from("----- SOURCE CODE\n");
        for (name, src) in [$(($name, $module_src)),*] {
            output.push_str(&format!("-- {name}.gleam\n{src}\n\n"));
        }
        output.push_str(&format!("-- main.gleam\n{}\n\n----- WARNING\n{warning}", $src));
        insta::assert_snapshot!(insta::internals::AutoName, output, $src);
    };

    ($(($package:expr, $name:expr, $module_src:literal)),+, $src:expr) => {
        let warning = $crate::type_::tests::get_printed_warnings(
            $src,
            vec![$(($package, $name, $module_src)),*],
            crate::build::Target::Erlang,
            None
        );
        assert!(!warning.is_empty());

        let mut output = String::from("----- SOURCE CODE\n");
        for (name, src) in [$(($name, $module_src)),*] {
            output.push_str(&format!("-- {name}.gleam\n{src}\n\n"));
        }
        output.push_str(&format!("-- main.gleam\n{}\n\n----- WARNING\n{warning}", $src));
        insta::assert_snapshot!(insta::internals::AutoName, output, $src);
    };
}

#[macro_export]
macro_rules! assert_js_warning {
    ($src:expr) => {
        let warning = $crate::type_::tests::get_printed_warnings(
            $src,
            vec![],
            crate::build::Target::JavaScript,
            None,
        );
        assert!(!warning.is_empty());
        let output = format!("----- SOURCE CODE\n{}\n\n----- WARNING\n{}", $src, warning);
        insta::assert_snapshot!(insta::internals::AutoName, output, $src);
    };
}

#[macro_export]
macro_rules! assert_js_no_warnings {
    ($src:expr) => {
        let warning = $crate::type_::tests::get_printed_warnings(
            $src,
            vec![],
            crate::build::Target::JavaScript,
            None,
        );
        assert!(warning.is_empty());
    };
}

#[macro_export]
macro_rules! assert_warnings_with_gleam_version {
    ($gleam_version:expr, $src:expr$(,)?) => {
        let warning = $crate::type_::tests::get_printed_warnings(
            $src,
            vec![],
            crate::build::Target::Erlang,
            Some($gleam_version),
        );
        assert!(!warning.is_empty());
        let output = format!("----- SOURCE CODE\n{}\n\n----- WARNING\n{}", $src, warning);
        insta::assert_snapshot!(insta::internals::AutoName, output, $src);
    };
}

#[macro_export]
macro_rules! assert_js_warnings_with_gleam_version {
    ($gleam_version:expr, $src:expr$(,)?) => {
        let warning = $crate::type_::tests::get_printed_warnings(
            $src,
            vec![],
            crate::build::Target::JavaScript,
            Some($gleam_version),
        );
        assert!(!warning.is_empty());
        let output = format!("----- SOURCE CODE\n{}\n\n----- WARNING\n{}", $src, warning);
        insta::assert_snapshot!(insta::internals::AutoName, output, $src);
    };
}

#[macro_export]
macro_rules! assert_js_no_warnings_with_gleam_version {
    ($gleam_version:expr, $src:expr$(,)?) => {
        let warning = $crate::type_::tests::get_printed_warnings(
            $src,
            vec![],
            crate::build::Target::JavaScript,
            Some($gleam_version),
        );
        assert!(warning.is_empty());
    };
}

#[macro_export]
macro_rules! assert_no_warnings {
    ($src:expr $(,)?) => {
        let warnings = $crate::type_::tests::get_warnings($src, vec![], crate::build::Target::Erlang, None);
        assert_eq!(warnings, vec![]);
    };
    ($(($name:expr, $module_src:literal)),+, $src:expr $(,)?) => {
        let warnings = $crate::type_::tests::get_warnings(
            $src,
            vec![$(("thepackage", $name, $module_src)),*],
            crate::build::Target::Erlang,
            None,
        );
        assert_eq!(warnings, vec![]);
    };
    ($(($package:expr, $name:expr, $module_src:literal)),+, $src:expr $(,)?) => {
        let warnings = $crate::type_::tests::get_warnings(
            $src,
            vec![$(($package, $name, $module_src)),*],
            crate::build::Target::Erlang,
            None,
        );
        assert_eq!(warnings, vec![]);
    };
}

fn compile_statement_sequence(
    src: &str,
) -> Result<Vec1<TypedStatement>, (Vec1<crate::type_::Error>, Names)> {
    let ast = crate::parse::parse_statement_sequence(src).expect("syntax error");
    let mut modules = im::HashMap::new();
    let ids = UniqueIdGenerator::new();
    // DUPE: preludeinsertion
    // TODO: Currently we do this here and also in the tests. It would be better
    // to have one place where we create all this required state for use in each
    // place.
    let _ = modules.insert(PRELUDE_MODULE_NAME.into(), build_prelude(&ids));
    let mut problems = Problems::new();
    let mut environment = EnvironmentArguments {
        ids,
        current_package: "thepackage".into(),
        gleam_version: None,
        current_module: "themodule".into(),
        target: Target::Erlang,
        importable_modules: &modules,
        target_support: TargetSupport::Enforced,
        current_origin: Origin::Src,
    }
    .build();
    let res = ExprTyper::new(
        &mut environment,
        FunctionDefinition {
            has_body: true,
            has_erlang_external: false,
            has_javascript_external: false,
        },
        &mut problems,
    )
    .infer_statements(ast);
    match Vec1::try_from_vec(problems.take_errors()) {
        Err(_) => Ok(res),
        Ok(errors) => Err((errors, environment.names)),
    }
}

fn infer(src: &str) -> String {
    let mut printer = Printer::new();
    let result = compile_statement_sequence(src).expect("should successfully infer");
    printer.pretty_print(result.last().type_().as_ref(), 0)
}

pub fn stringify_tuple_strs(module: Vec<(&str, &str)>) -> Vec<(EcoString, String)> {
    module
        .into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .collect()
}

/// A module loaded as a dependency in the tests.
///
/// In order the tuple elements indicate:
/// 1. The package name.
/// 2. The module name.
/// 3. The module source.
type DependencyModule<'a> = (&'a str, &'a str, &'a str);

pub fn infer_module(src: &str, dep: Vec<DependencyModule<'_>>) -> Vec<(EcoString, String)> {
    infer_module_with_target("test_module", src, dep, Target::Erlang)
}

pub fn infer_module_with_target(
    module_name: &str,
    src: &str,
    dep: Vec<DependencyModule<'_>>,
    target: Target,
) -> Vec<(EcoString, String)> {
    let ast = compile_module_with_opts(
        module_name,
        src,
        None,
        dep,
        target,
        TargetSupport::NotEnforced,
        None,
    )
    .expect("should successfully infer");
    ast.type_info
        .values
        .iter()
        .filter(|(_, v)| v.publicity.is_importable())
        .map(|(k, v)| {
            let mut printer = Printer::new();
            (k.clone(), printer.pretty_print(&v.type_, 0))
        })
        .sorted()
        .collect()
}

pub fn compile_module(
    module_name: &str,
    src: &str,
    warnings: Option<Rc<dyn WarningEmitterIO>>,
    dep: Vec<DependencyModule<'_>>,
) -> Result<TypedModule, (Vec<crate::type_::Error>, Names)> {
    compile_module_with_opts(
        module_name,
        src,
        warnings,
        dep,
        Target::Erlang,
        TargetSupport::NotEnforced,
        None,
    )
}

pub fn compile_module_with_opts(
    module_name: &str,
    src: &str,
    warnings: Option<Rc<dyn WarningEmitterIO>>,
    dep: Vec<DependencyModule<'_>>,
    target: Target,
    target_support: TargetSupport,
    gleam_version: Option<Range<Version>>,
) -> Result<TypedModule, (Vec<crate::type_::Error>, Names)> {
    let ids = UniqueIdGenerator::new();
    let mut modules = im::HashMap::new();

    let emitter =
        WarningEmitter::new(warnings.unwrap_or_else(|| Rc::new(VectorWarningEmitterIO::default())));

    // DUPE: preludeinsertion
    // TODO: Currently we do this here and also in the tests. It would be better
    // to have one place where we create all this required state for use in each
    // place.
    let _ = modules.insert(PRELUDE_MODULE_NAME.into(), build_prelude(&ids));
    let mut direct_dependencies = HashMap::from_iter(vec![]);

    for (package, name, module_src) in dep {
        let parsed =
            crate::parse::parse_module(Utf8PathBuf::from("test/path"), module_src, &emitter)
                .expect("syntax error");
        let mut ast = parsed.module;
        ast.name = name.into();
        let line_numbers = LineNumbers::new(module_src);
        let mut config = PackageConfig::default();
        config.name = package.into();
        let module = crate::analyse::ModuleAnalyzerConstructor::<()> {
            target,
            ids: &ids,
            origin: Origin::Src,
            importable_modules: &modules,
            warnings: &TypeWarningEmitter::null(),
            direct_dependencies: &HashMap::new(),
            target_support,
            package_config: &config,
        }
        .infer_module(ast, line_numbers, "".into())
        .expect("should successfully infer");
        let _ = modules.insert(name.into(), module.type_info);

        if package != "non-dependency-package" {
            let _ = direct_dependencies.insert(package.into(), ());
        }
    }

    let parsed = crate::parse::parse_module(Utf8PathBuf::from("test/path"), src, &emitter)
        .expect("syntax error");
    let mut ast = parsed.module;
    ast.name = module_name.into();
    let mut config = PackageConfig::default();
    config.name = "thepackage".into();
    config.gleam_version = gleam_version.map(|v| GleamVersion::from_pubgrub(v));

    let warnings = TypeWarningEmitter::new("/src/warning/wrn.gleam".into(), src.into(), emitter);
    let inference_result = crate::analyse::ModuleAnalyzerConstructor::<()> {
        target,
        ids: &ids,
        origin: Origin::Src,
        importable_modules: &modules,
        warnings: &warnings,
        direct_dependencies: &direct_dependencies,
        target_support: TargetSupport::Enforced,
        package_config: &config,
    }
    .infer_module(ast, LineNumbers::new(src), "".into());

    match inference_result {
        Outcome::Ok(ast) => Ok(ast),
        Outcome::PartialFailure(ast, errors) => Err((errors.into(), ast.names)),
        Outcome::TotalFailure(error) => Err((error.into(), Default::default())),
    }
}

pub fn module_error(src: &str, deps: Vec<DependencyModule<'_>>) -> String {
    module_error_with_target(src, deps, Target::Erlang)
}

pub fn module_error_with_target(
    src: &str,
    deps: Vec<DependencyModule<'_>>,
    target: Target,
) -> String {
    let (error, names) = compile_module_with_opts(
        "themodule",
        src,
        None,
        deps,
        target,
        TargetSupport::NotEnforced,
        None,
    )
    .expect_err("should infer an error");
    let error = Error::Type {
        names: Box::new(names),
        src: src.into(),
        path: Utf8PathBuf::from("/src/one/two.gleam"),
        errors: Vec1::try_from_vec(error).expect("should have at least one error"),
    };
    error.pretty_string()
}

pub fn internal_module_error(src: &str, deps: Vec<DependencyModule<'_>>) -> String {
    internal_module_error_with_target(src, deps, Target::Erlang)
}

pub fn internal_module_error_with_target(
    src: &str,
    deps: Vec<DependencyModule<'_>>,
    target: Target,
) -> String {
    let (error, names) = compile_module_with_opts(
        "thepackage/internal/themodule",
        src,
        None,
        deps,
        target,
        TargetSupport::NotEnforced,
        None,
    )
    .expect_err("should infer an error");
    let error = Error::Type {
        names: Box::new(names),
        src: src.into(),
        path: Utf8PathBuf::from("/src/one/two.gleam"),
        errors: Vec1::try_from_vec(error).expect("should have at least one error"),
    };
    error.pretty_string()
}

pub fn syntax_error(src: &str) -> String {
    let error =
        crate::parse::parse_module(Utf8PathBuf::from("test/path"), src, &WarningEmitter::null())
            .expect_err("should trigger an error when parsing");
    let error = Error::Parse {
        src: src.into(),
        path: Utf8PathBuf::from("/src/one/two.gleam"),
        error: Box::new(error),
    };
    error.pretty_string()
}

#[test]
fn field_map_reorder_test() {
    let int = |value: &str| UntypedExpr::Int {
        value: value.into(),
        int_value: crate::parse::parse_int_value(value).unwrap(),
        location: SrcSpan { start: 0, end: 0 },
    };

    struct Case {
        arity: u32,
        fields: HashMap<EcoString, u32>,
        arguments: Vec<CallArg<UntypedExpr>>,
        expected_result: Result<(), crate::type_::Error>,
        expected_arguments: Vec<CallArg<UntypedExpr>>,
    }

    impl Case {
        fn test(self) {
            let mut arguments = self.arguments;
            let fm = FieldMap {
                arity: self.arity,
                fields: self.fields,
            };
            let location = SrcSpan { start: 0, end: 0 };
            assert_eq!(
                self.expected_result,
                fm.reorder(&mut arguments, location, IncorrectArityContext::Function)
            );
            assert_eq!(self.expected_arguments, arguments);
        }
    }

    Case {
        arity: 0,
        fields: HashMap::new(),
        arguments: vec![],
        expected_result: Ok(()),
        expected_arguments: vec![],
    }
    .test();

    Case {
        arity: 3,
        fields: HashMap::new(),
        arguments: vec![
            CallArg {
                implicit: None,
                location: Default::default(),
                label: None,
                value: int("1"),
            },
            CallArg {
                implicit: None,
                location: Default::default(),
                label: None,
                value: int("2"),
            },
            CallArg {
                implicit: None,
                location: Default::default(),
                label: None,
                value: int("3"),
            },
        ],
        expected_result: Ok(()),
        expected_arguments: vec![
            CallArg {
                implicit: None,
                location: Default::default(),
                label: None,
                value: int("1"),
            },
            CallArg {
                implicit: None,
                location: Default::default(),
                label: None,
                value: int("2"),
            },
            CallArg {
                implicit: None,
                location: Default::default(),
                label: None,
                value: int("3"),
            },
        ],
    }
    .test();

    Case {
        arity: 3,
        fields: [("last".into(), 2)].into(),
        arguments: vec![
            CallArg {
                implicit: None,
                location: Default::default(),
                label: None,
                value: int("1"),
            },
            CallArg {
                implicit: None,
                location: Default::default(),
                label: None,
                value: int("2"),
            },
            CallArg {
                implicit: None,
                location: Default::default(),
                label: Some("last".into()),
                value: int("3"),
            },
        ],
        expected_result: Ok(()),
        expected_arguments: vec![
            CallArg {
                implicit: None,
                location: Default::default(),
                label: None,
                value: int("1"),
            },
            CallArg {
                implicit: None,
                location: Default::default(),
                label: None,
                value: int("2"),
            },
            CallArg {
                implicit: None,
                location: Default::default(),
                label: Some("last".into()),
                value: int("3"),
            },
        ],
    }
    .test();
}

#[test]
fn infer_module_type_retention_test() {
    let module: UntypedModule = crate::ast::Module {
        documentation: vec![],
        name: "ok".into(),
        definitions: vec![],
        type_info: (),
        names: Default::default(),
        unused_definition_positions: Default::default(),
    };
    let direct_dependencies = HashMap::from_iter(vec![]);
    let ids = UniqueIdGenerator::new();
    let mut modules = im::HashMap::new();
    // DUPE: preludeinsertion
    // TODO: Currently we do this here and also in the tests. It would be better
    // to have one place where we create all this required state for use in each
    // place.
    let _ = modules.insert(PRELUDE_MODULE_NAME.into(), build_prelude(&ids));
    let mut config = PackageConfig::default();
    config.name = "thepackage".into();

    let module = crate::analyse::ModuleAnalyzerConstructor::<()> {
        target: Target::Erlang,
        ids: &ids,
        origin: Origin::Src,
        importable_modules: &modules,
        warnings: &TypeWarningEmitter::null(),
        direct_dependencies: &direct_dependencies,
        target_support: TargetSupport::Enforced,
        package_config: &config,
    }
    .infer_module(module, LineNumbers::new(""), "".into())
    .expect("Should infer OK");

    assert_eq!(
        module.type_info,
        ModuleInterface {
            warnings: vec![],
            origin: Origin::Src,
            package: "thepackage".into(),
            name: "ok".into(),
            is_internal: false,
            types: HashMap::new(),
            types_value_constructors: HashMap::from([
                (
                    "Bool".into(),
                    TypeVariantConstructors {
                        type_parameters_ids: vec![],
                        variants: vec![
                            TypeValueConstructor {
                                name: "True".into(),
                                parameters: vec![],
                                documentation: None,
                            },
                            TypeValueConstructor {
                                name: "False".into(),
                                parameters: vec![],
                                documentation: None,
                            }
                        ],
                        opaque: Opaque::NotOpaque,
                    }
                ),
                (
                    "Result".into(),
                    TypeVariantConstructors {
                        type_parameters_ids: vec![1, 2],
                        variants: vec![
                            TypeValueConstructor {
                                name: "Ok".into(),
                                parameters: vec![TypeValueConstructorField {
                                    type_: generic_var(1),
                                    label: None,
                                }],
                                documentation: None,
                            },
                            TypeValueConstructor {
                                name: "Error".into(),
                                parameters: vec![TypeValueConstructorField {
                                    type_: generic_var(2),
                                    label: None,
                                }],
                                documentation: None,
                            }
                        ],
                        opaque: Opaque::NotOpaque,
                    }
                ),
                (
                    "Nil".into(),
                    TypeVariantConstructors {
                        type_parameters_ids: vec![],
                        variants: vec![TypeValueConstructor {
                            name: "Nil".into(),
                            parameters: vec![],
                            documentation: None,
                        }],
                        opaque: Opaque::NotOpaque,
                    }
                )
            ]),
            values: HashMap::new(),
            accessors: HashMap::new(),
            line_numbers: LineNumbers::new(""),
            src_path: "".into(),
            minimum_required_version: Version::new(0, 1, 0),
            type_aliases: HashMap::new(),
            documentation: Vec::new(),
            contains_echo: false,
            references: References::default()
        }
    );
}

#[test]
fn simple_exprs() {
    assert_infer!("True", "Bool");
    assert_infer!("False", "Bool");
    assert_infer!("1", "Int");
    assert_infer!("-2", "Int");
    assert_infer!("1.0", "Float");
    assert_infer!("-8.0", "Float");
    assert_infer!("\"ok\"", "String");
    assert_infer!("\"ok\"", "String");
    assert_infer!("[]", "List(a)");
    assert_infer!("4 % 1", "Int");
    assert_infer!("4 > 1", "Bool");
    assert_infer!("4 >= 1", "Bool");
    assert_infer!("4 <= 1", "Bool");
    assert_infer!("4 < 1", "Bool");

    // Numbers with _'s
    assert_infer!("1000_000", "Int");
    assert_infer!("1_000", "Int");
    assert_infer!("1_000.", "Float");
    assert_infer!("10_000.001", "Float");
    assert_infer!("100_000.", "Float");

    // Nil
    assert_infer!("Nil", "Nil");

    // todo
    assert_infer!("todo", "a");
    assert_infer!("1 == todo", "Bool");
    assert_infer!("todo != 1", "Bool");
    assert_infer!("todo + 1", "Int");
    assert_infer!("todo(\"test\") + 1", "Int");

    // hex, octal, and binary literals
    assert_infer!("0xF", "Int");
    assert_infer!("0o11", "Int");
    assert_infer!("0b1010", "Int");

    // scientific notation
    assert_infer!("6.02e23", "Float");
    assert_infer!("6.02e-23", "Float");
}

#[test]
fn assert() {
    assert_infer!("let assert [] = [] 1", "Int");
    assert_infer!("let assert [a] = [1] a", "Int");
    assert_infer!("let assert [a, 2] = [1] a", "Int");
    assert_infer!("let assert [a, .._] = [1] a", "Int");
    assert_infer!("let assert [a, .._,] = [1] a", "Int");
    assert_infer!("fn(x) { let assert [a] = x a }", "fn(List(a)) -> a");
    assert_infer!("fn(x) { let assert [a] = x a + 1 }", "fn(List(Int)) -> Int");
    assert_infer!("let assert _x = 1 2.0", "Float");
    assert_infer!("let assert _ = 1 2.0", "Float");
    assert_infer!("let assert #(tag, x) = #(1.0, 1) x", "Int");
    assert_infer!("fn(x) { let assert #(a, b) = x a }", "fn(#(a, b)) -> a");
    assert_infer!("let assert 5: Int = 5 5", "Int");
}

#[test]
fn lists() {
    assert_infer!("[]", "List(a)");
    assert_infer!("[1]", "List(Int)");
    assert_infer!("[1, 2, 3]", "List(Int)");
    assert_infer!("[[]]", "List(List(a))");
    assert_infer!("[[1.0, 2.0]]", "List(List(Float))");
    assert_infer!("[fn(x) { x }]", "List(fn(a) -> a)");
    assert_infer!("[fn(x) { x + 1 }]", "List(fn(Int) -> Int)");
    assert_infer!("[fn(x) { x }, fn(x) { x + 1 }]", "List(fn(Int) -> Int)");
    assert_infer!("[fn(x) { x + 1 }, fn(x) { x }]", "List(fn(Int) -> Int)");
    assert_infer!("[[], []]", "List(List(a))");
    assert_infer!("[[], [1]]", "List(List(Int))");

    assert_infer!("[1, ..[2, ..[]]]", "List(Int)");
    assert_infer!("[fn(x) { x }, ..[]]", "List(fn(a) -> a)");
    assert_infer!("let x = [1, ..[]] [2, ..x]", "List(Int)");
}

#[test]
fn trailing_comma_lists() {
    assert_infer!("[1, ..[2, ..[],]]", "List(Int)");
    assert_infer!("[fn(x) { x },..[]]", "List(fn(a) -> a)");

    assert_infer!("let f = fn(x) { x } [f, f]", "List(fn(a) -> a)");
    assert_infer!("[#([], [])]", "List(#(List(a), List(b)))");
}

#[test]
fn tuples() {
    assert_infer!("#(1)", "#(Int)");
    assert_infer!("#(1, 2.0)", "#(Int, Float)");
    assert_infer!("#(1, 2.0, 3)", "#(Int, Float, Int)");
    assert_infer!("#(1, 2.0, #(1, 1))", "#(Int, Float, #(Int, Int))",);
}

#[test]
fn expr_fn() {
    assert_infer!("fn(x) { x }", "fn(a) -> a");
    assert_infer!("fn(x) { x }", "fn(a) -> a");
    assert_infer!("fn(x, y) { x }", "fn(a, b) -> a");
    assert_infer!("fn(x, y) { [] }", "fn(a, b) -> List(c)");
    assert_infer!("let x = 1.0 1", "Int");
    assert_infer!("let id = fn(x) { x } id(1)", "Int");
    assert_infer!("let x = fn() { 1.0 } x()", "Float");
    assert_infer!("fn(x) { x }(1)", "Int");
    assert_infer!("fn() { 1 }", "fn() -> Int");
    assert_infer!("fn() { 1.1 }", "fn() -> Float");
    assert_infer!("fn(x) { 1.1 }", "fn(a) -> Float");
    assert_infer!("fn(x) { x }", "fn(a) -> a");
    assert_infer!("let x = fn(x) { 1.1 } x", "fn(a) -> Float");
    assert_infer!("fn(x, y, z) { 1 }", "fn(a, b, c) -> Int");
    assert_infer!("fn(x) { let y = x y }", "fn(a) -> a");
    assert_infer!("let id = fn(x) { x } id(1)", "Int");
    assert_infer!(
        "let constant = fn(x) { fn(y) { x } } let one = constant(1) one(2.0)",
        "Int",
    );

    assert_infer!("fn(f) { f(1) }", "fn(fn(Int) -> a) -> a");
    assert_infer!("fn(f, x) { f(x) }", "fn(fn(a) -> b, a) -> b");
    assert_infer!("fn(f) { fn(x) { f(x) } }", "fn(fn(a) -> b) -> fn(a) -> b");
    assert_infer!(
        "fn(f) { fn(x) { fn(y) { f(x, y) } } }",
        "fn(fn(a, b) -> c) -> fn(a) -> fn(b) -> c",
    );
    assert_infer!(
        "fn(f) { fn(x, y) { f(x)(y) } }",
        "fn(fn(a) -> fn(b) -> c) -> fn(a, b) -> c",
    );
    assert_infer!(
        "fn(f) { fn(x) { let ff = f ff(x) } }",
        "fn(fn(a) -> b) -> fn(a) -> b",
    );
    assert_infer!(
        "fn(f) { fn(x, y) { let ff = f(x) ff(y) } }",
        "fn(fn(a) -> fn(b) -> c) -> fn(a, b) -> c",
    );
    assert_infer!("fn(x) { fn(y) { x } }", "fn(a) -> fn(b) -> a");
    assert_infer!("fn(f) { f() }", "fn(fn() -> a) -> a");
    assert_infer!("fn(f, x) { f(f(x)) }", "fn(fn(a) -> a, a) -> a");
    assert_infer!(
        "let id = fn(a) { a } fn(x) { x(id) }",
        "fn(fn(fn(a) -> a) -> b) -> b",
    );

    assert_infer!("let add = fn(x, y) { x + y } add(_, 2)", "fn(Int) -> Int");
    assert_infer!("fn(x) { #(1, x) }", "fn(a) -> #(Int, a)");
    assert_infer!("fn(x, y) { #(x, y) }", "fn(a, b) -> #(a, b)");
    assert_infer!("fn(x) { #(x, x) }", "fn(a) -> #(a, a)");
    assert_infer!("fn(x) -> Int { x }", "fn(Int) -> Int");
    assert_infer!("fn(x) -> a { x }", "fn(a) -> a");
    assert_infer!("fn() -> Int { 2 }", "fn() -> Int");
}

#[test]
fn case() {
    assert_infer!("case 1 { a -> 1 }", "Int");
    assert_infer!("case 1 { a -> 1.0 b -> 2.0 c -> 3.0 }", "Float");
    assert_infer!("case 1 { a -> a }", "Int");
    assert_infer!("case 1 { 1 -> 10 2 -> 20 x -> x * 10 }", "Int");
    assert_infer!("case 2.0 { 2.0 -> 1 x -> 0 }", "Int");
    assert_infer!(r#"case "ok" { "ko" -> 1 x -> 0 }"#, "Int");
}

#[test]
fn multiple_subject_case() {
    assert_infer!("case 1, 2.0 { a, b -> a }", "Int");
    assert_infer!("case 1, 2.0 { a, b -> b }", "Float");
    assert_infer!("case 1, 2.0, 3 { a, b, c -> a + c }", "Int");
}

#[test]
fn tuple_index() {
    assert_infer!("#(1, 2.0).0", "Int");
    assert_infer!("#(1, 2.0).1", "Float");
}

#[test]
fn pipe() {
    assert_infer!("1 |> fn(x) { x }", "Int");
    assert_infer!("1.0 |> fn(x) { x }", "Float");
    assert_infer!("let id = fn(x) { x } 1 |> id", "Int");
    assert_infer!("let id = fn(x) { x } 1.0 |> id", "Float");
    assert_infer!("let add = fn(x, y) { x + y } 1 |> add(_, 2)", "Int");
    assert_infer!("let add = fn(x, y) { x + y } 1 |> add(2, _)", "Int");
    assert_infer!("let add = fn(x, y) { x + y } 1 |> add(2)", "Int");
    assert_infer!("let id = fn(x) { x } 1 |> id()", "Int");
    assert_infer!("let add = fn(x) { fn(y) { y + x } } 1 |> add(1)", "Int");
    assert_infer!(
        "let add = fn(x, _, _) { fn(y) { y + x } } 1 |> add(1, 2, 3)",
        "Int"
    );
}

#[test]
fn bit_array() {
    assert_infer!("let assert <<x>> = <<1>> x", "Int");
}

#[test]
fn bit_array2() {
    assert_infer!("let assert <<x>> = <<1>> x", "Int");
}

#[test]
fn bit_array3() {
    assert_infer!("let assert <<x:float>> = <<1>> x", "Float");
}

#[test]
fn bit_array4() {
    assert_infer!("let assert <<x:bytes>> = <<1>> x", "BitArray");
}

#[test]
fn bit_array5() {
    assert_infer!("let assert <<x:bytes>> = <<1>> x", "BitArray");
}

#[test]
fn bit_array6() {
    assert_infer!("let assert <<x:bits>> = <<1>> x", "BitArray");
}

#[test]
fn bit_array7() {
    assert_infer!("let assert <<x:bits>> = <<1>> x", "BitArray");
}

#[test]
fn bit_array8() {
    assert_infer!(
        "let assert <<x:utf8_codepoint>> = <<128013:32>> x",
        "UtfCodepoint"
    );
}

#[test]
fn bit_array9() {
    assert_infer!(
        "let assert <<x:utf16_codepoint>> = <<128013:32>> x",
        "UtfCodepoint"
    );
}

#[test]
fn bit_array10() {
    assert_infer!(
        "let assert <<x:utf32_codepoint>> = <<128013:32>> x",
        "UtfCodepoint"
    );
}

#[test]
fn bit_array11() {
    assert_infer!(
        "let a = <<1>> let assert <<x:bits>> = <<1, a:2-bits>> x",
        "BitArray"
    );
}

#[test]
fn bit_array12() {
    assert_infer!("let x = <<<<1>>:bits, <<2>>:bits>> x", "BitArray");
}

#[test]
fn infer_module_test() {
    assert_module_infer!(
        "pub fn repeat(i, x) {
           case i {
             0 -> []
             i -> [x .. repeat(i - 1, x)]
           }
         }",
        vec![("repeat", "fn(Int, a) -> List(a)")],
    );
}

#[test]
fn infer_module_test1() {
    assert_module_infer!(
        "pub fn length(list) {
           case list {
           [] -> 0
           [x .. xs] -> length(xs) + 1
           }
        }",
        vec![("length", "fn(List(a)) -> Int")],
    );
}

#[test]
fn infer_module_test2() {
    assert_module_infer!(
        "fn private() { 1 }
         pub fn public() { 1 }",
        vec![("public", "fn() -> Int")],
    );
}

#[test]
fn infer_module_test3() {
    assert_module_infer!(
        "pub type Is { Yes No }
         pub fn yes() { Yes }
         pub fn no() { No }",
        vec![
            ("No", "Is"),
            ("Yes", "Is"),
            ("no", "fn() -> Is"),
            ("yes", "fn() -> Is"),
        ],
    );
}

#[test]
fn infer_module_test4() {
    assert_module_infer!(
        "pub type Num { I(Int) }
         pub fn one() { I(1) }",
        vec![("I", "fn(Int) -> Num"), ("one", "fn() -> Num")],
    );
}

#[test]
fn infer_module_test5() {
    assert_module_infer!(
        "pub type Box(a) { Box(a) }
        pub fn int() { Box(1) }
        pub fn float() { Box(1.0) }",
        vec![
            ("Box", "fn(a) -> Box(a)"),
            ("float", "fn() -> Box(Float)"),
            ("int", "fn() -> Box(Int)"),
        ],
    );
}

#[test]
fn infer_module_test6() {
    assert_module_infer!(
        "pub type Singleton { Singleton }
        pub fn go(x) { let Singleton = x 1 }",
        vec![("Singleton", "Singleton"), ("go", "fn(Singleton) -> Int")],
    );
}

#[test]
fn infer_module_test7() {
    assert_module_infer!(
        "pub type Box(a) { Box(a) }
        pub fn unbox(x) { let Box(a) = x a }",
        vec![("Box", "fn(a) -> Box(a)"), ("unbox", "fn(Box(a)) -> a")],
    );
}

#[test]
fn infer_module_test8() {
    assert_module_infer!(
        "pub type I { I(Int) }
        pub fn open(x) { case x { I(i) -> i  } }",
        vec![("I", "fn(Int) -> I"), ("open", "fn(I) -> Int")],
    );
}

#[test]
fn infer_module_test9() {
    assert_module_infer!(
        "pub fn status() { 1 } pub fn list_of(x) { [x] }",
        vec![("list_of", "fn(a) -> List(a)"), ("status", "fn() -> Int")],
    );
}

#[test]
fn infer_module_test10() {
    assert_module_infer!(
        "
@external(erlang, \"\", \"\")
pub fn go(x: String) -> String
",
        vec![("go", "fn(String) -> String")],
    );
}

#[test]
fn infer_module_test11() {
    assert_module_infer!(
        "
@external(erlang, \"\", \"\")
pub fn go(x: Int) -> Float
",
        vec![("go", "fn(Int) -> Float")],
    );
}

#[test]
fn infer_module_test12() {
    assert_module_infer!(
        "
@external(erlang, \"\", \"\")
pub fn go(x: Int) -> Int
",
        vec![("go", "fn(Int) -> Int")],
    );
}

#[test]
fn infer_module_test13() {
    assert_module_infer!(
        "
@external(erlang, \"\", \"\")
pub fn ok() -> fn(Int) -> Int
",
        vec![("ok", "fn() -> fn(Int) -> Int")],
    );
}

#[test]
fn infer_module_test14() {
    assert_module_infer!(
        "
@external(erlang, \"\", \"\")
pub fn go(x: Int) -> b
",
        vec![("go", "fn(Int) -> a")],
    );
}

#[test]
fn infer_module_test15() {
    assert_module_infer!(
        "
@external(erlang, \"\", \"\")
pub fn go(x: Bool) -> b
",
        vec![("go", "fn(Bool) -> a")],
    );
}

#[test]
fn infer_module_test16() {
    assert_module_infer!(
        "
@external(erlang, \"\", \"\")
pub fn go(x: List(a)) -> a
",
        vec![("go", "fn(List(a)) -> a")],
    );
}

#[test]
fn infer_module_test17() {
    assert_module_infer!(
        "
@external(erlang, \"\", \"\")
fn go(x: Int) -> b
        pub fn x() { go(1) }",
        vec![("x", "fn() -> a")],
    );
}

#[test]
fn infer_module_test18() {
    assert_module_infer!(
        "@external(erlang, \"\", \"\")
        fn id(a: a) -> a
        pub fn i(x) { id(x) }
        pub fn a() { id(1) }
        pub fn b() { id(1.0) }",
        vec![
            ("a", "fn() -> Int"),
            ("b", "fn() -> Float"),
            ("i", "fn(a) -> a"),
        ],
    );
}

#[test]
fn infer_module_test19() {
    assert_module_infer!(
        "
@external(erlang, \"\", \"\")
pub fn len(a: List(a)) -> Int
",
        vec![("len", "fn(List(a)) -> Int")],
    );
}

#[test]
fn infer_module_test20() {
    assert_module_infer!(
        "pub type Connection\n
@external(erlang, \"\", \"\")
pub fn is_open(x: Connection) -> Bool
",
        vec![("is_open", "fn(Connection) -> Bool")],
    );
}

#[test]
fn infer_module_test21() {
    assert_module_infer!(
        "pub type Pair(a, b)\n
@external(erlang, \"\", \"\")
pub fn pair(x: a) -> Pair(a, a)
",
        vec![("pair", "fn(a) -> Pair(a, a)")],
    );
}

#[test]
fn infer_module_test22() {
    assert_module_infer!(
        "pub fn one() { 1 }
         pub fn zero() { one() - 1 }
         pub fn two() { one() + zero() }",
        vec![
            ("one", "fn() -> Int"),
            ("two", "fn() -> Int"),
            ("zero", "fn() -> Int"),
        ],
    );
}

#[test]
fn infer_module_test23() {
    assert_module_infer!(
        "pub fn one() { 1 }
         pub fn zero() { one() - 1 }
         pub fn two() { one() + zero() }",
        vec![
            ("one", "fn() -> Int"),
            ("two", "fn() -> Int"),
            ("zero", "fn() -> Int"),
        ],
    );
}

#[test]
fn infer_module_test24() {
    // Structs
    assert_module_infer!(
        "pub type Box { Box(boxed: Int) }",
        vec![("Box", "fn(Int) -> Box")]
    );
}

#[test]
fn infer_module_test25() {
    assert_module_infer!(
        "pub type Tup(a, b) { Tup(first: a, second: b) }",
        vec![("Tup", "fn(a, b) -> Tup(a, b)")]
    );
}

#[test]
fn infer_module_test26() {
    assert_module_infer!(
        "pub type Tup(a, b, c) { Tup(first: a, second: b, third: c) }
         pub fn third(t) { let Tup(_ , _, third: a) = t a }",
        vec![
            ("Tup", "fn(a, b, c) -> Tup(a, b, c)"),
            ("third", "fn(Tup(a, b, c)) -> c"),
        ],
    );
}

#[test]
fn infer_label_shorthand_pattern() {
    assert_module_infer!(
        "pub type Tup(a, b, c) { Tup(first: a, second: b, third: c) }
         pub fn third(t) { let Tup(_, _, third:) = t third }",
        vec![
            ("Tup", "fn(a, b, c) -> Tup(a, b, c)"),
            ("third", "fn(Tup(a, b, c)) -> c"),
        ],
    );
}

#[test]
fn infer_module_test27() {
    // Anon structs
    assert_module_infer!(
        "pub fn ok(x) { #(1, x) }",
        vec![("ok", "fn(a) -> #(Int, a)")],
    );
}

#[test]
fn infer_module_test28() {
    assert_module_infer!(
        "
@external(erlang, \"\", \"\")
pub fn ok(a: Int) -> #(Int, Int)
",
        vec![("ok", "fn(Int) -> #(Int, Int)")],
    );
}

#[test]
fn infer_module_test29() {
    assert_module_infer!(
        "
@external(erlang, \"\", \"\")
pub fn go(a: #(a, c)) -> c
",
        vec![("go", "fn(#(a, b)) -> b")],
    );
}

#[test]
fn infer_module_test30() {
    assert_module_infer!(
        "pub fn always(ignore _a, return b) { b }",
        vec![("always", "fn(a, b) -> b")],
    );
}

#[test]
fn infer_module_test31() {
    // Using types before they are defined

    assert_module_infer!(
        "pub type I { I(Num) } pub type Num { Num }",
        vec![("I", "fn(Num) -> I"), ("Num", "Num")]
    );
}

#[test]
fn infer_module_test32() {
    assert_module_infer!(
        "pub type I { I(Num) } pub type Num",
        vec![("I", "fn(Num) -> I")]
    );
}

#[test]
fn type_alias() {
    assert_module_infer!(
        "type Html = String
         pub fn go() { 1 }",
        vec![("go", "fn() -> Int")],
    );
    assert_module_infer!(
        "type IntString = Result(Int, String)
         pub fn ok_one() -> IntString { Ok(1) }",
        vec![("ok_one", "fn() -> Result(Int, String)")]
    );
}

#[test]
fn build_in_type_alias_shadow() {
    // We can create an alias with the same name as a built in type
    assert_module_infer!(
        "type Int = Float
         pub fn ok_one() -> Int { 1.0 }",
        vec![("ok_one", "fn() -> Float")]
    );
}

#[test]
fn accessor() {
    // We can access fields on custom types with only one variant
    assert_module_infer!(
        "
pub type Person { Person(name: String, age: Int) }
pub fn get_age(person: Person) { person.age }
pub fn get_name(person: Person) { person.name }",
        vec![
            ("Person", "fn(String, Int) -> Person"),
            ("get_age", "fn(Person) -> Int"),
            ("get_name", "fn(Person) -> String"),
        ]
    );

    // We can access fields on custom types with only one variant
    assert_module_infer!(
        "
pub type One { One(name: String) }
pub type Two { Two(one: One) }
pub fn get(x: Two) { x.one.name }",
        vec![
            ("One", "fn(String) -> One"),
            ("Two", "fn(One) -> Two"),
            ("get", "fn(Two) -> String"),
        ]
    );
}

#[test]
fn generic_accessor() {
    // Field access correctly handles type parameters
    assert_module_infer!(
        "
pub type Box(a) { Box(inner: a) }
pub fn get_box(x: Box(Box(a))) { x.inner }
pub fn get_generic(x: Box(a)) { x.inner }
pub fn get_get_box(x: Box(Box(a))) { x.inner.inner }
pub fn get_int(x: Box(Int)) { x.inner }
pub fn get_string(x: Box(String)) { x.inner }
",
        vec![
            ("Box", "fn(a) -> Box(a)"),
            ("get_box", "fn(Box(Box(a))) -> Box(a)"),
            ("get_generic", "fn(Box(a)) -> a"),
            ("get_get_box", "fn(Box(Box(a))) -> a"),
            ("get_int", "fn(Box(Int)) -> Int"),
            ("get_string", "fn(Box(String)) -> String"),
        ]
    );
}

#[test]
fn generic_accessor_later_defined() {
    // Field access works before type is defined
    assert_module_infer!(
        "
pub fn name(cat: Cat) {
  cat.name
}

pub opaque type Cat {
  Cat(name: String)
}",
        vec![("name", "fn(Cat) -> String"),]
    );
}

#[test]
fn custom_type_annotation() {
    // We can annotate let with custom types
    assert_module_infer!(
        "
        pub type Person {
            Person(name: String, age: Int)
        }

        pub fn create_person(name: String) {
            let x: Person = Person(name: name, age: 1)
            x
        }",
        vec![
            ("Person", "fn(String, Int) -> Person"),
            ("create_person", "fn(String) -> Person"),
        ]
    );

    assert_module_infer!(
        "
        pub type Box(inner) {
            Box(inner)
        }

        pub fn create_int_box(value: Int) {
            let x: Box(Int) = Box(value)
            x
        }

        pub fn create_float_box(value: Float) {
            let x: Box(Float) = Box(value)
            x
        }",
        vec![
            ("Box", "fn(a) -> Box(a)"),
            ("create_float_box", "fn(Float) -> Box(Float)"),
            ("create_int_box", "fn(Int) -> Box(Int)"),
        ]
    );
}

#[test]
fn opaque_accessors() {
    // Opaque type constructors are available in the module where they are defined
    // but are not exported
    assert_module_infer!(
        "
pub opaque type One { One(name: String) }
pub fn get(x: One) { x.name }",
        vec![("get", "fn(One) -> String"),]
    );
}

#[test]
fn fn_annotation_reused() {
    // Type variables are shared between function annotations and function
    // annotations within their body
    assert_module_infer!(
        "
        pub type Box(a) {
            Box(value: a)
        }
        pub fn go(box1: Box(a)) {
            fn(box2: Box(a)) { box1.value == box2.value }
        }",
        vec![
            ("Box", "fn(a) -> Box(a)"),
            ("go", "fn(Box(a)) -> fn(Box(a)) -> Bool")
        ]
    );

    // Type variables are shared between function annotations and let
    // annotations within their body
    assert_module_infer!(
        "
        pub type Box(a) {
            Box(value: a)
        }
        pub fn go(box1: Box(a)) {
            let x: Box(a) = box1
            fn(box2: Box(a)) { x.value == box2.value }
        }",
        vec![
            ("Box", "fn(a) -> Box(a)"),
            ("go", "fn(Box(a)) -> fn(Box(a)) -> Bool")
        ]
    );
}

#[test]
fn accessor_multiple_variants() {
    // We can access fields on custom types with multiple variants
    assert_module_infer!(
        "
pub type Person {
    Teacher(name: String, title: String)
    Student(name: String, age: Int)
}
pub fn get_name(person: Person) { person.name }",
        vec![
            ("Student", "fn(String, Int) -> Person"),
            ("Teacher", "fn(String, String) -> Person"),
            ("get_name", "fn(Person) -> String"),
        ]
    );
}

#[test]
fn record_accessor_multiple_variants_parameterised_types() {
    // We can access fields on custom types with multiple variants
    // In positions other than the 1st field
    assert_module_infer!(
        "
pub type Person {
    Teacher(name: String, age: List(Int), title: String)
    Student(name: String, age: List(Int))
}
pub fn get_name(person: Person) { person.name }
pub fn get_age(person: Person) { person.age }",
        vec![
            ("Student", "fn(String, List(Int)) -> Person"),
            ("Teacher", "fn(String, List(Int), String) -> Person"),
            ("get_age", "fn(Person) -> List(Int)"),
            ("get_name", "fn(Person) -> String"),
        ]
    );
}

#[test]
fn accessor_multiple_variants_positions_other_than_first() {
    // We can access fields on custom types with multiple variants
    // In positions other than the 1st field
    assert_module_infer!(
        "
pub type Person {
    Teacher(name: String, age: Int, title: String)
    Student(name: String, age: Int)
}
pub fn get_name(person: Person) { person.name }
pub fn get_age(person: Person) { person.age }",
        vec![
            ("Student", "fn(String, Int) -> Person"),
            ("Teacher", "fn(String, Int, String) -> Person"),
            ("get_age", "fn(Person) -> Int"),
            ("get_name", "fn(Person) -> String"),
        ]
    );
}

#[test]
fn box_record() {
    assert_module_infer!(
        "
pub type Box {
  Box(a: Nil, b: Int, c: Int, d: Int)
}

pub fn main() {
  Box(b: 1, c: 1, d: 1, a: Nil)
}",
        vec![
            ("Box", "fn(Nil, Int, Int, Int) -> Box"),
            ("main", "fn() -> Box"),
        ],
    );
}

#[test]
fn record_update_no_fields() {
    // No arguments given to a record update
    assert_module_infer!(
        "
        pub type Person {
            Person(name: String, age: Int)
        }
        pub fn identity(person: Person) {
            Person(..person)
        }",
        vec![
            ("Person", "fn(String, Int) -> Person"),
            ("identity", "fn(Person) -> Person")
        ]
    );
}

#[test]
fn record_update() {
    // Some arguments given to a record update
    assert_module_infer!(
        "
        pub type Person {
            Person(name: String, age: Int)
        }
        pub fn update_name(person: Person, name: String) {
            Person(..person, name: name)
        }",
        vec![
            ("Person", "fn(String, Int) -> Person"),
            ("update_name", "fn(Person, String) -> Person")
        ]
    );
}

#[test]
fn record_update_all_fields() {
    // All arguments given in order to a record update
    assert_module_infer!(
        "
        pub type Person {
            Person(name: String, age: Int)
        }
        pub fn update_person(person: Person, name: String, age: Int) {
            Person(..person, name: name, age: age, )
        }",
        vec![
            ("Person", "fn(String, Int) -> Person"),
            ("update_person", "fn(Person, String, Int) -> Person")
        ]
    );
}

#[test]
fn record_update_out_of_order() {
    // All arguments given out of order to a record update
    assert_module_infer!(
        "
        pub type Person {
            Person(name: String, age: Int)
        }
        pub fn update_person(person: Person, name: String, age: Int) {
            Person(..person, age: age, name: name)
        }",
        vec![
            ("Person", "fn(String, Int) -> Person"),
            ("update_person", "fn(Person, String, Int) -> Person")
        ]
    );
}

#[test]
fn record_update_generic() {
    // A record update with polymorphic types
    assert_module_infer!(
        "
        pub type Box(a, b) {
            Box(left: a, right: b)
        }

        pub fn combine_boxes(a: Box(Int, Bool), b: Box(Bool, Int)) {
            Box(..a, left: a.left + b.right, right: b.left)
        }",
        vec![
            ("Box", "fn(a, b) -> Box(a, b)"),
            (
                "combine_boxes",
                "fn(Box(Int, Bool), Box(Bool, Int)) -> Box(Int, Bool)"
            )
        ]
    );
}

#[test]
fn record_update_generic_unannotated() {
    // A record update with unannotated polymorphic types
    assert_module_infer!(
        "
        pub type Box(a, b) {
            Box(left: a, right: b)
        }

        pub fn combine_boxes(a: Box(t1, t2), b: Box(t2, t1)) {
            Box(..a, left: b.right, right: b.left)
        }",
        vec![
            ("Box", "fn(a, b) -> Box(a, b)"),
            ("combine_boxes", "fn(Box(a, b), Box(b, a)) -> Box(a, b)")
        ]
    );
}

#[test]
fn record_update_variant_inference() {
    assert_module_infer!(
        "
pub type Shape {
  Circle(cx: Int, cy: Int, radius: Int)
  Square(x: Int, y: Int, width: Int, height: Int)
}

pub fn grow(shape) {
  case shape {
    Circle(radius:, ..) as circle -> Circle(..circle, radius: radius + 1)
    Square(width:, height:, ..) as square -> Square(..square, width: width + 1, height: height + 1)
  }
}
",
        vec![
            ("Circle", "fn(Int, Int, Int) -> Shape"),
            ("Square", "fn(Int, Int, Int, Int) -> Shape"),
            ("grow", "fn(Shape) -> Shape")
        ]
    );
}

#[test]
fn record_access_variant_inference() {
    assert_module_infer!(
        "
pub type Wibble {
  Wibble(a: Int, b: Int)
  Wobble(a: Int, c: Int)
}

pub fn get(wibble) {
  case wibble {
    Wibble(..) as w -> w.b
    Wobble(..) as w -> w.c
  }
}
",
        vec![
            ("Wibble", "fn(Int, Int) -> Wibble"),
            ("Wobble", "fn(Int, Int) -> Wibble"),
            ("get", "fn(Wibble) -> Int")
        ]
    );
}

#[test]
fn local_variable_variant_inference() {
    assert_module_infer!(
        "
pub type Wibble {
  Wibble(a: Int, b: Int)
  Wobble(a: Int, c: Int)
}

pub fn main() {
  let always_wibble = Wibble(1, 2)
  always_wibble.b
}
",
        vec![
            ("Wibble", "fn(Int, Int) -> Wibble"),
            ("Wobble", "fn(Int, Int) -> Wibble"),
            ("main", "fn() -> Int")
        ]
    );
}

#[test]
fn record_update_variant_inference_for_original_variable() {
    assert_module_infer!(
        r#"
pub type Wibble {
  Wibble(a: Int, b: Int)
  Wobble(a: Int, c: String)
}

pub fn update(wibble: Wibble) -> Wibble {
  case wibble {
    Wibble(..) -> Wibble(..wibble, a: 1)
    Wobble(..) -> Wobble(..wibble, c: "hello")
  }
}
"#,
        vec![
            ("Wibble", "fn(Int, Int) -> Wibble"),
            ("Wobble", "fn(Int, String) -> Wibble"),
            ("update", "fn(Wibble) -> Wibble")
        ]
    );
}

#[test]
fn record_access_variant_inference_for_original_variable() {
    assert_module_infer!(
        "
pub type Wibble {
  Wibble(a: Int, b: Int)
  Wobble(a: Int, c: Int)
}

pub fn get(wibble) {
  case wibble {
    Wibble(..) -> wibble.b
    Wobble(..) -> wibble.c
  }
}
",
        vec![
            ("Wibble", "fn(Int, Int) -> Wibble"),
            ("Wobble", "fn(Int, Int) -> Wibble"),
            ("get", "fn(Wibble) -> Int")
        ]
    );
}

#[test]
fn variant_inference_for_imported_type() {
    assert_module_infer!(
        (
            "wibble",
            "
pub type Wibble {
  Wibble(a: Int, b: Int)
  Wobble(a: Int, c: Int)
}
"
        ),
        "
import wibble.{Wibble, Wobble}

pub fn main(wibble) {
  case wibble {
    Wibble(..) -> Wibble(a: 1, b: wibble.b + 1)
    Wobble(..) -> Wobble(..wibble, c: wibble.c - 4)
  }
}
",
        vec![("main", "fn(Wibble) -> Wibble")]
    );
}

#[test]
fn local_variable_variant_inference_for_imported_type() {
    assert_module_infer!(
        (
            "wibble",
            "
pub type Wibble {
  Wibble(a: Int, b: Int)
  Wobble(a: Int, c: Int)
}
"
        ),
        "
import wibble.{Wibble}

pub fn main() {
  let wibble = Wibble(4, 9)
  Wibble(..wibble, b: wibble.b)
}
",
        vec![("main", "fn() -> Wibble")]
    );
}

#[test]
fn record_update_variant_inference_fails_for_several_possible_variants() {
    assert_module_error!(
        "
pub type Vector {
  Vector2(x: Float, y: Float)
  Vector3(x: Float, y: Float, z: Float)
}

pub fn increase_y(vector, by increase) {
  case vector {
    Vector2(y:, ..) as vector | Vector3(y:, ..) as vector ->
      Vector2(..vector, y: y +. increase)
  }
}
"
    );
}

#[test]
fn record_update_variant_inference_fails_for_several_possible_variants_on_subject_variable() {
    assert_module_error!(
        r#"
pub type Wibble {
  Wibble(a: Int, b: Int)
  Wobble(a: Int, c: String)
}

pub fn update(wibble: Wibble) -> Wibble {
  case wibble {
    Wibble(..) | Wobble(..) -> Wibble(..wibble, a: 1)
  }
}
"#
    );
}

#[test]
fn type_unification_does_not_cause_false_positives_for_variant_matching() {
    assert_module_error!(
        r#"
pub type Wibble {
  Wibble(a: Int, b: Int)
  Wobble(a: Int, c: String)
}

pub fn wibbler() { todo }

pub fn main() {
  let c = wibbler()

  case todo {
    Wibble(..) -> Wibble(..c, b: 1)
    _ -> todo
  }
}
"#
    );
}

#[test]
fn type_unification_does_not_allow_different_variants_to_be_treated_as_safe() {
    assert_module_error!(
        r#"
pub type Wibble {
  Wibble(a: Int, b: Int)
  Wobble(a: Int, c: String)
}

pub fn main() {
  let a = case todo {
    Wibble(..) as b -> Wibble(..b, b: 1)
    Wobble(..) as b -> Wobble(..b, c: "a")
  }

  a.b
}
"#
    );
}

#[test]
fn record_update_variant_inference_in_alternate_pattern_with_all_same_variants() {
    assert_module_infer!(
        r#"
pub type Vector {
  Vector2(x: Float, y: Float)
  Vector3(x: Float, y: Float, z: Float)
}

pub fn increase_y(vector, by increase) {
  case vector {
    Vector2(y:, ..) as vector -> Vector2(..vector, y: y +. increase)
    Vector3(y:, z: 12.3, ..) as vector | Vector3(y:, z: 15.0, ..) as vector ->
      Vector3(..vector, y: y +. increase, z: 0.0)
    _ -> panic as "Could not increase Y"
  }
}
"#,
        vec![
            ("Vector2", "fn(Float, Float) -> Vector"),
            ("Vector3", "fn(Float, Float, Float) -> Vector"),
            ("increase_y", "fn(Vector, Float) -> Vector")
        ]
    );
}

#[test]
fn variant_inference_does_not_escape_clause_scope() {
    assert_module_error!(
        "
pub type Thingy {
  A(a: Int)
  B(x: Int, b: Int)
}

pub fn fun(x) {
  case x {
    A(..) -> x.a
    B(..) -> x.b
  }
  x.b
}
"
    );
}

#[test]
// https://github.com/gleam-lang/gleam/issues/3797
fn type_unification_removes_inferred_variant_in_tuples() {
    assert_module_error!(
        r#"
pub type Either(a, b) {
  Left(value: a)
  Right(value: b)
}

fn a_or_b(_first: value, second: value) -> value {
  second
}

pub fn main() {
  let #(right) = a_or_b(#(Left(5)), #(Right("hello")))
  Left(..right, value: 10)
}
"#
    );
}

#[test]
// https://github.com/gleam-lang/gleam/issues/3797
fn type_unification_removes_inferred_variant_in_functions() {
    assert_module_error!(
        r#"
pub type Either(a, b) {
  Left(value: a)
  Right(value: b)
}

fn a_or_b(_first: value, second: value) -> value {
  second
}

pub fn main() {
  let func = a_or_b(fn() { Left(1) }, fn() { Right("hello") })
  Left(..func(), value: 10)
}
"#
    );
}

#[test]
// https://github.com/gleam-lang/gleam/issues/3797
fn type_unification_removes_inferred_variant_in_nested_type() {
    assert_module_error!(
        r#"
pub type Box(a) {
  Box(inner: a)
}

pub type Either(a, b) {
  Left(value: a)
  Right(value: b)
}

fn a_or_b(_first: value, second: value) -> value {
  second
}

pub fn main() {
  let Box(inner) = a_or_b(Box(Left(1)), Box(Right("hello")))
  Left(..inner, value: 10)
}
"#
    );
}

#[test]
fn module_constants() {
    assert_module_infer!(
        "
    pub const test_int1 = 123
    pub const test_int2: Int = 321
    pub const test_int3 = 0xE
    pub const test_int4 = 0o10
    pub const test_int5 = 0o10011
    pub const test_float: Float = 4.2
    pub const test_string = \"hey!\"
    pub const test_list = [1,2,3]
    pub const test_tuple = #(\"yes!\", 42)
    pub const test_var1 = test_int1
    pub const test_var2: Int = test_int1",
        vec![
            ("test_float", "Float"),
            ("test_int1", "Int"),
            ("test_int2", "Int"),
            ("test_int3", "Int"),
            ("test_int4", "Int"),
            ("test_int5", "Int"),
            ("test_list", "List(Int)"),
            ("test_string", "String"),
            ("test_tuple", "#(String, Int)"),
            ("test_var1", "Int"),
            ("test_var2", "Int")
        ],
    );
}

#[test]
fn custom_type_module_constants() {
    assert_module_infer!(
        "pub type Test { A }
        pub const some_test = A",
        vec![("A", "Test"), ("some_test", "Test")],
    );
}

#[test]
fn module_constant_functions() {
    assert_module_infer!(
        "pub fn int_identity(i: Int) -> Int { i }
        pub const int_identity_alias1 = int_identity
        pub const int_identity_alias2 = int_identity_alias1
        pub const int_identity_alias3: fn(Int) -> Int = int_identity_alias2",
        vec![
            ("int_identity", "fn(Int) -> Int"),
            ("int_identity_alias1", "fn(Int) -> Int"),
            ("int_identity_alias2", "fn(Int) -> Int"),
            ("int_identity_alias3", "fn(Int) -> Int"),
        ]
    );
}

#[test]
fn functions_used_before_definition() {
    assert_module_infer!(
        "pub fn a() { b() }
         pub fn b() { 1 }",
        vec![("a", "fn() -> Int"), ("b", "fn() -> Int")],
    );
}

#[test]
fn functions_used_before_definition1() {
    assert_module_infer!(
        "pub fn a() { b() + c() }
         fn b() { 1 }
         fn c() { 1 }",
        vec![("a", "fn() -> Int")],
    );
}

#[test]
fn functions_used_before_definition2() {
    assert_module_infer!(
        "fn b() { 1 }
         pub fn a() { b() + c() }
         fn c() { 1 }",
        vec![("a", "fn() -> Int")],
    );
}

#[test]
fn functions_used_before_definition3() {
    assert_module_infer!(
        "pub fn a() { Thing }
         pub type Thing { Thing }",
        vec![("Thing", "Thing"), ("a", "fn() -> Thing"),],
    );
}

#[test]
fn types_used_before_definition() {
    assert_module_infer!(
        "pub type Y { Y(X) }
         pub type X",
        vec![("Y", "fn(X) -> Y")],
    );
}

#[test]
fn types_used_before_definition1() {
    assert_module_infer!(
        "pub type Y { Y(x: X) }
         pub type X",
        vec![("Y", "fn(X) -> Y")],
    );
}

#[test]
fn consts_used_before_definition() {
    assert_module_infer!(
        "pub fn a() { b }
        const b = 1",
        vec![("a", "fn() -> Int")],
    );
}

#[test]
fn mutual_recursion() {
    assert_module_infer!(
        "pub fn a() { b() }
         fn b() { a() }",
        vec![("a", "fn() -> a")],
    );

    assert_module_infer!(
        "pub fn a() { b() }
         fn b() { a() + 1 }",
        vec![("a", "fn() -> Int")],
    );
}

#[test]
fn type_annotations() {
    assert_module_infer!(
        "pub type Box(x) { Box(label: String, contents: x) }
         pub fn id(x: Box(y)) { x }",
        vec![
            ("Box", "fn(String, a) -> Box(a)"),
            ("id", "fn(Box(a)) -> Box(a)"),
        ],
    );

    assert_module_infer!("pub fn go(x: Int) { x }", vec![("go", "fn(Int) -> Int")],);
    assert_module_infer!("pub fn go(x: b) -> b { x }", vec![("go", "fn(a) -> a")],);
    assert_module_infer!("pub fn go(x) -> b { x }", vec![("go", "fn(a) -> a")],);
    assert_module_infer!("pub fn go(x: b) { x }", vec![("go", "fn(a) -> a")],);
    assert_module_infer!(
        "pub fn go(x: List(b)) -> List(b) { x }",
        vec![("go", "fn(List(a)) -> List(a)")],
    );
    assert_module_infer!(
        "pub fn go(x: List(b)) { x }",
        vec![("go", "fn(List(a)) -> List(a)")],
    );
    assert_module_infer!(
        "pub fn go(x: List(String)) { x }",
        vec![("go", "fn(List(String)) -> List(String)")],
    );
    assert_module_infer!("pub fn go(x: b, y: c) { x }", vec![("go", "fn(a, b) -> a")],);
    assert_module_infer!("pub fn go(x) -> Int { x }", vec![("go", "fn(Int) -> Int")],);

    assert_module_infer!(
        "pub fn id(x: x) { x }
         pub fn float() { id(1.0) }
         pub fn int() { id(1) }",
        vec![
            ("float", "fn() -> Float"),
            ("id", "fn(a) -> a"),
            ("int", "fn() -> Int"),
        ],
    );
}

#[test]
fn early_function_generalisation() {
    assert_module_infer!(
        "pub fn id(x) { x }
         pub fn int() { id(1) }",
        vec![("id", "fn(a) -> a"), ("int", "fn() -> Int")],
    );
}

#[test]
fn early_function_generalisation2() {
    assert_module_infer!(
        "pub fn id(x) { x }
         pub fn int() { id(1) }
         pub fn float() { id(1.0) }
         ",
        vec![
            ("float", "fn() -> Float"),
            ("id", "fn(a) -> a"),
            ("int", "fn() -> Int"),
        ],
    );
}

// https://github.com/gleam-lang/gleam/issues/970
#[test]
fn bit_array_pattern_unification() {
    assert_module_infer!(
        "pub fn m(x) { case x { <<>> -> Nil _ -> Nil} }",
        vec![("m", "fn(BitArray) -> Nil")],
    );
}

// https://github.com/gleam-lang/gleam/issues/970
#[test]
fn bit_array_pattern_unification2() {
    assert_module_infer!(
        "pub fn m(x) { case x { <<>> -> Nil _ -> Nil} }",
        vec![("m", "fn(BitArray) -> Nil")],
    );
}

// https://github.com/gleam-lang/gleam/issues/983
#[test]
fn qualified_prelude() {
    assert_module_infer!(
        "import gleam
pub fn a() {
  gleam.Ok(1)
}",
        vec![("a", "fn() -> Result(Int, a)")],
    );
}

// https://github.com/gleam-lang/gleam/issues/1029
#[test]
fn empty_list_const() {
    assert_module_infer!(
        "pub const empty = []
pub fn a() {
    empty
}",
        vec![("a", "fn() -> List(a)"), ("empty", "List(a)")],
    );
}

#[test]
fn let_as_expression() {
    assert_infer!("let x = 1", "Int");
}

#[test]
fn let_as_expression1() {
    assert_infer!("let x = { let x = 1 }", "Int");
}

#[test]
fn let_as_expression2() {
    assert_infer!("let x = { let x = 1. }", "Float");
}

#[test]
fn string_concat_ok() {
    assert_infer!(r#" "1" <> "2" "#, "String");
}

#[test]
fn string_concat_ko_1() {
    assert_error!(r#" "1" <> 2 "#);
}

#[test]
fn string_concat_ko_2() {
    assert_error!(r#" 1 <> "2" "#);
}

// https://github.com/gleam-lang/gleam/issues/1087
#[test]
fn generic_inner_access() {
    assert_module_infer!(
        "pub type B(b) { B(value: b) }
pub fn b_get_first(b: B(#(a))) {
  b.value.0
}",
        vec![("B", "fn(a) -> B(a)"), ("b_get_first", "fn(B(#(a))) -> a")],
    );
}

// https://github.com/gleam-lang/gleam/issues/1093
#[test]
fn fn_contextual_info() {
    assert_module_infer!(
        "
type Box {
  Box(inner: Int)
}

fn call(argument: t, function: fn(t) -> tt) -> tt {
  function(argument)
}

pub fn main() {
  call(Box(1), fn(box) { box.inner })
}
",
        vec![("main", "fn() -> Int")],
    );
}

// https://github.com/gleam-lang/gleam/issues/1519
#[test]
fn permit_holes_in_fn_args_and_returns() {
    assert_module_infer!(
        "pub fn run(args: List(_)) -> List(_) {
  todo
}",
        vec![("run", "fn(List(a)) -> List(b)")],
    );
}

// Rattard's parser issue
#[test]
fn block_maths() {
    assert_module_infer!(
        "pub fn do(max, min) {
  { max -. min } /. { max +. min }
}",
        vec![("do", "fn(Float, Float) -> Float")],
    );
}

#[test]
fn infer_label_shorthand_in_call_arg() {
    assert_module_infer!(
        "
    pub fn main() {
        let arg1 = 1
        let arg2 = 1.0
        let arg3 = False
        wibble(arg2:, arg3:, arg1:)
    }

    pub fn wibble(arg1 arg1: Int, arg2 arg2: Float, arg3 arg3: Bool) { Nil }
        ",
        vec![
            ("main", "fn() -> Nil"),
            ("wibble", "fn(Int, Float, Bool) -> Nil")
        ],
    );
}

#[test]
fn infer_label_shorthand_in_constructor_arg() {
    assert_module_infer!(
        "
    pub type Wibble { Wibble(arg1: Int, arg2: Bool, arg3: Float) }
    pub fn main() {
        let arg1 = 1
        let arg2 = True
        let arg3 = 1.0
        Wibble(arg2:, arg3:, arg1:)
    }
",
        vec![
            ("Wibble", "fn(Int, Bool, Float) -> Wibble"),
            ("main", "fn() -> Wibble"),
        ],
    );
}

#[test]
fn infer_label_shorthand_in_constant_constructor_arg() {
    assert_module_infer!(
        "
    pub type Wibble { Wibble(arg1: Int, arg2: Bool, arg3: Float) }
    pub const arg1 = 1
    pub const arg2 = True
    pub const arg3 = 1.0

    pub const wibble = Wibble(arg2:, arg3:, arg1:)
",
        vec![
            ("Wibble", "fn(Int, Bool, Float) -> Wibble"),
            ("arg1", "Int"),
            ("arg2", "Bool"),
            ("arg3", "Float"),
            ("wibble", "Wibble")
        ],
    );
}

#[test]
fn infer_label_shorthand_in_pattern_arg() {
    assert_module_infer!(
        "
    pub type Wibble { Wibble(arg1: Int, arg2: Bool, arg3: Int) }
    pub fn main() {
        case Wibble(1, True, 2) {
           Wibble(arg2:, arg3:, arg1:) if arg2 -> arg1 * arg3
           _ -> 0
        }
    }
",
        vec![
            ("Wibble", "fn(Int, Bool, Int) -> Wibble"),
            ("main", "fn() -> Int")
        ],
    );
}

#[test]
fn infer_label_shorthand_in_record_update_arg() {
    assert_module_infer!(
        "
    pub type Wibble { Wibble(arg1: Int, arg2: Bool, arg3: Float) }
    pub fn main() {
        let wibble = Wibble(1, True, 2.0)
        let arg3 = 3.0
        let arg2 = False
        Wibble(..wibble, arg3:, arg2:)
    }
",
        vec![
            ("Wibble", "fn(Int, Bool, Float) -> Wibble"),
            ("main", "fn() -> Wibble")
        ],
    );
}

#[test]
fn public_type_from_internal_module_has_internal_publicity() {
    let module = compile_module("thepackage/internal", "pub type Wibble", None, vec![]).unwrap();
    let type_ = module.type_info.get_public_type("Wibble").unwrap();
    assert!(type_.publicity.is_internal());
}

#[test]
fn internal_type_from_internal_module_has_internal_publicity() {
    let module = compile_module(
        "thepackage/internal",
        "@internal pub type Wibble",
        None,
        vec![],
    )
    .unwrap();
    let type_ = module.type_info.get_public_type("Wibble").unwrap();
    assert!(type_.publicity.is_internal());
}

#[test]
fn private_type_from_internal_module_is_not_exposed_as_internal() {
    let module = compile_module("thepackage/internal", "type Wibble", None, vec![]).unwrap();
    assert!(module.type_info.get_public_type("Wibble").is_none());
}

#[test]
fn assert_suitable_main_function_not_module_function() {
    let value = ValueConstructor {
        publicity: Publicity::Public,
        deprecation: Deprecation::NotDeprecated,
        type_: fn_(vec![], int()),
        variant: ValueConstructorVariant::ModuleConstant {
            documentation: None,
            location: Default::default(),
            module: "module".into(),
            literal: Constant::Int {
                location: Default::default(),
                value: "1".into(),
                int_value: 1.into(),
            },
            implementations: Implementations {
                gleam: true,
                uses_erlang_externals: false,
                uses_javascript_externals: false,
                can_run_on_erlang: true,
                can_run_on_javascript: true,
            },
            name: "main".into(),
        },
    };
    assert!(
        assert_suitable_main_function(&value, &"module".into(), Origin::Src, Target::Erlang)
            .is_err(),
    );
}

#[test]
fn assert_suitable_main_function_wrong_arity() {
    let value = ValueConstructor {
        publicity: Publicity::Public,
        deprecation: Deprecation::NotDeprecated,
        type_: fn_(vec![], int()),
        variant: ValueConstructorVariant::ModuleFn {
            name: "name".into(),
            field_map: None,
            arity: 1,
            documentation: None,
            location: Default::default(),
            module: "module".into(),
            external_erlang: None,
            external_javascript: None,
            implementations: Implementations {
                gleam: true,
                uses_erlang_externals: false,
                uses_javascript_externals: false,
                can_run_on_erlang: true,
                can_run_on_javascript: true,
            },
            purity: Purity::Pure,
        },
    };
    assert!(
        assert_suitable_main_function(&value, &"module".into(), Origin::Src, Target::Erlang)
            .is_err(),
    );
}

#[test]
fn assert_suitable_main_function_ok() {
    let value = ValueConstructor {
        publicity: Publicity::Public,
        deprecation: Deprecation::NotDeprecated,
        type_: fn_(vec![], int()),
        variant: ValueConstructorVariant::ModuleFn {
            name: "name".into(),
            field_map: None,
            arity: 0,
            documentation: None,
            location: Default::default(),
            module: "module".into(),
            external_erlang: None,
            external_javascript: None,
            implementations: Implementations {
                gleam: true,
                uses_erlang_externals: false,
                uses_javascript_externals: false,
                can_run_on_erlang: true,
                can_run_on_javascript: true,
            },
            purity: Purity::Pure,
        },
    };
    assert!(
        assert_suitable_main_function(&value, &"module".into(), Origin::Src, Target::Erlang)
            .is_ok(),
    );
}

#[test]
fn assert_suitable_main_function_erlang_not_supported() {
    let value = ValueConstructor {
        publicity: Publicity::Public,
        deprecation: Deprecation::NotDeprecated,
        type_: fn_(vec![], int()),
        variant: ValueConstructorVariant::ModuleFn {
            name: "name".into(),
            field_map: None,
            arity: 0,
            documentation: None,
            location: Default::default(),
            module: "module".into(),
            external_erlang: Some(("wibble".into(), "wobble".into())),
            external_javascript: Some(("wobble".into(), "wibble".into())),
            implementations: Implementations {
                gleam: false,
                uses_erlang_externals: true,
                uses_javascript_externals: true,
                can_run_on_erlang: false,
                can_run_on_javascript: true,
            },
            purity: Purity::Impure,
        },
    };
    assert!(
        assert_suitable_main_function(&value, &"module".into(), Origin::Src, Target::Erlang)
            .is_err(),
    );
}

#[test]
fn assert_suitable_main_function_javascript_not_supported() {
    let value = ValueConstructor {
        publicity: Publicity::Public,
        deprecation: Deprecation::NotDeprecated,
        type_: fn_(vec![], int()),
        variant: ValueConstructorVariant::ModuleFn {
            name: "name".into(),
            field_map: None,
            arity: 0,
            documentation: None,
            location: Default::default(),
            module: "module".into(),
            external_erlang: Some(("wibble".into(), "wobble".into())),
            external_javascript: Some(("wobble".into(), "wibble".into())),
            implementations: Implementations {
                gleam: false,
                uses_erlang_externals: true,
                uses_javascript_externals: true,
                can_run_on_erlang: true,
                can_run_on_javascript: false,
            },
            purity: Purity::Impure,
        },
    };
    assert!(
        assert_suitable_main_function(&value, &"module".into(), Origin::Src, Target::JavaScript)
            .is_err(),
    );
}

#[test]
fn pipe_with_annonymous_unannotated_functions() {
    assert_module_infer!(
        r#"
pub fn main() {
  let a = 1
     |> fn (x) { #(x, x + 1) }
     |> fn (x) { x.0 }
     |> fn (x) { x }
}
"#,
        vec![("main", "fn() -> Int")]
    );
}

#[test]
fn pipe_with_annonymous_unannotated_functions_wrong_arity1() {
    assert_module_error!(
        r#"
pub fn main() {
  let a = 1
     |> fn (x) { #(x, x + 1) }
     |> fn (x, y) { x.0 }
     |> fn (x) { x }
}
"#
    );
}

#[test]
fn pipe_with_annonymous_unannotated_functions_wrong_arity2() {
    assert_module_error!(
        r#"
pub fn main() {
  let a = 1
     |> fn (x) { #(x, x + 1) }
     |> fn (x) { x.0 }
     |> fn (x, y) { x }
}
"#
    );
}

#[test]
fn pipe_with_annonymous_unannotated_functions_wrong_arity3() {
    assert_module_error!(
        r#"
pub fn main() {
  let a = 1
     |> fn (x) { #(x, x + 1) }
     |> fn (x) { x.0 }
     |> fn () { x }
}
"#
    );
}

#[test]
fn pipe_with_annonymous_mixed_functions() {
    assert_module_infer!(
        r#"
pub fn main() {
  let a = "abc"
     |> fn (x) { #(x, x <> "d") }
     |> fn (x) { x.0 }
     |> fn (x: String) { x }
}
"#,
        vec![("main", "fn() -> String")]
    );
}

#[test]
fn pipe_with_annonymous_functions_using_structs() {
    // https://github.com/gleam-lang/gleam/issues/2504
    assert_module_infer!(
        r#"
type Date {
  Date(day: Day)
}
type Day {
  Day(year: Int)
}
fn now() -> Date {
  Date(Day(2024))
}
fn get_day(date: Date) -> Day {
  date.day
}
pub fn main() {
  now() |> get_day() |> fn (it) { it.year }
}
"#,
        vec![("main", "fn() -> Int")]
    );
}

#[test]
fn labelled_argument_ordering() {
    // https://github.com/gleam-lang/gleam/issues/3671
    assert_module_infer!(
        "
type A { A }
type B { B }
type C { C }
type D { D }

fn wibble(a a: A, b b: B, c c: C, d d: D) {
  Nil
}

pub fn main() {
  wibble(A, C, D, b: B)
  wibble(A, C, D, b: B)
  wibble(B, C, D, a: A)
  wibble(B, C, a: A, d: D)
  wibble(B, C, d: D, a: A)
  wibble(B, D, a: A, c: C)
  wibble(B, D, c: C, a: A)
  wibble(C, D, b: B, a: A)
}
",
        vec![("main", "fn() -> Nil")]
    );
}

#[test]
fn variant_inference_allows_inference() {
    // https://github.com/gleam-lang/gleam/pull/3647#issuecomment-2423146977
    assert_module_infer!(
        "
pub type Wibble {
  Wibble(a: Int)
  Wobble(b: Int)
}

pub fn do_a_thing(wibble) {
  case wibble {
    Wibble(..) -> wibble.a
    _ -> todo
  }
  wibble
}
",
        vec![
            ("Wibble", "fn(Int) -> Wibble"),
            ("Wobble", "fn(Int) -> Wibble"),
            ("do_a_thing", "fn(Wibble) -> Wibble")
        ]
    );
}

#[test]
fn variant_inference_allows_inference2() {
    // https://github.com/gleam-lang/gleam/pull/3647#issuecomment-2423146977
    assert_module_infer!(
        "
pub type Box(a) {
  Box(inner: a)
  UnBox
}

pub fn rebox(box) {
  case box {
    Box(..) -> Box(box.inner + 1)
    UnBox -> UnBox
  }
}
",
        vec![
            ("Box", "fn(a) -> Box(a)"),
            ("UnBox", "Box(a)"),
            ("rebox", "fn(Box(Int)) -> Box(Int)")
        ]
    );
}

#[test]
// https://github.com/gleam-lang/gleam/issues/3861
fn variant_inference_on_literal_record() {
    assert_module_error!(
        "
pub type Wibble {
  Wibble
  Wobble
}

pub fn main() {
  case Wibble, Wobble {
    Wibble, Wibble -> todo
  }
}
"
    );
}

#[test]
fn variant_inference_on_prelude_types() {
    assert_module_infer!(
        "
pub fn main() {
  let always_ok = Ok(10)
  case always_ok {
    Ok(1) -> 1
    Ok(2) -> 3
    _ -> panic
  }
}
",
        vec![("main", "fn() -> Int")]
    );
}

#[test]
fn variant_inference_with_let_assert() {
    assert_module_infer!(
        "
type Wibble {
  Wibble(n: Int, b: Bool)
  Wobble
}

pub fn main() {
  let wibble = wibble()
  let assert Wibble(..) = wibble
  wibble.n
}

fn wibble() {
  Wibble(1, True)
}
",
        vec![("main", "fn() -> Int")]
    );
}

#[test]
fn variant_inference_with_let_assert_and_alias() {
    assert_module_infer!(
        "
type Wibble {
  Wibble(n: Int, b: Bool)
  Wobble
}

pub fn main() {
  let assert Wibble(..) as wibble = wibble()
  wibble.n
}

fn wibble() {
  Wibble(1, True)
}
",
        vec![("main", "fn() -> Int")]
    );
}

#[test]
fn private_types_not_available_in_other_modules() {
    assert_module_error!(
        ("wibble", "type Wibble"),
        "
import wibble

type Wibble {
  Wibble(wibble.Wibble)
}
"
    );
}

#[test]
fn unlabelled_argument_not_allowed_after_labelled_argument() {
    assert_module_error!(
        "
pub type Bad {
  Bad(labelled: Int, Float)
}
"
    );
}

#[test]
fn function_parameter_errors_do_not_stop_inference() {
    assert_module_error!(
        "
pub fn wibble(x: NonExistent) {
  1 + False
}
"
    );
}

// https://github.com/gleam-lang/gleam/issues/4287
#[test]
fn no_stack_overflow_for_nested_use() {
    assert_module_infer!(
        (
            "gleam/dynamic/decode",
            "
pub fn string() {
  Nil
}

pub fn field(name, decoder, callback) {
  callback(Nil)
}
"
        ),
        r#"
import gleam/dynamic/decode

pub fn main() {
  use _n1 <- decode.field("n1", decode.string)
  use _n2 <- decode.field("n2", decode.string)
  use _n3 <- decode.field("n3", decode.string)
  use _n4 <- decode.field("n4", decode.string)
  use _n5 <- decode.field("n5", decode.string)
  use _n6 <- decode.field("n6", decode.string)
  use _n7 <- decode.field("n7", decode.string)
  use _n8 <- decode.field("n8", decode.string)
  use _n9 <- decode.field("n9", decode.string)
  use _n10 <- decode.field("n10", decode.string)
  use _n11 <- decode.field("n11", decode.string)
  use _n12 <- decode.field("n12", decode.string)
  use _n13 <- decode.field("n13", decode.string)
  use _n14 <- decode.field("n14", decode.string)
  use _n15 <- decode.field("n15", decode.string)
  use _n16 <- decode.field("n16", decode.string)
  use _n17 <- decode.field("n17", decode.string)
  use _n18 <- decode.field("n18", decode.string)
  use _n19 <- decode.field("n19", decode.string)
  use _n20 <- decode.field("n20", decode.string)
  use _n21 <- decode.field("n21", decode.string)
  use _n22 <- decode.field("n22", decode.string)
  use _n23 <- decode.field("n23", decode.string)
  use _n24 <- decode.field("n24", decode.string)
  use _n25 <- decode.field("n25", decode.string)
  use _n26 <- decode.field("n26", decode.string)
  use _n27 <- decode.field("n27", decode.string)
  use _n28 <- decode.field("n28", decode.string)
  use _n29 <- decode.field("n29", decode.string)
  use _n30 <- decode.field("n30", decode.string)
  use _n31 <- decode.field("n31", decode.string)
  use _n32 <- decode.field("n32", decode.string)
  use _n33 <- decode.field("n33", decode.string)
  use _n34 <- decode.field("n34", decode.string)
  use _n35 <- decode.field("n35", decode.string)
  use _n36 <- decode.field("n36", decode.string)
  use _n37 <- decode.field("n37", decode.string)
  use _n38 <- decode.field("n38", decode.string)
  use _n39 <- decode.field("n39", decode.string)
  use _n40 <- decode.field("n40", decode.string)
  use _n41 <- decode.field("n41", decode.string)
  use _n42 <- decode.field("n42", decode.string)
  use _n43 <- decode.field("n43", decode.string)
  use _n44 <- decode.field("n44", decode.string)
  use _n45 <- decode.field("n45", decode.string)
  use _n46 <- decode.field("n46", decode.string)
  use _n47 <- decode.field("n47", decode.string)
  use _n48 <- decode.field("n48", decode.string)
  use _n49 <- decode.field("n49", decode.string)
  use _n50 <- decode.field("n50", decode.string)
  use _n51 <- decode.field("n51", decode.string)
  use _n52 <- decode.field("n52", decode.string)
  use _n53 <- decode.field("n53", decode.string)
  use _n54 <- decode.field("n54", decode.string)
  use _n55 <- decode.field("n55", decode.string)
  Nil
}
"#,
        vec![("main", "fn() -> Nil")]
    );
}
