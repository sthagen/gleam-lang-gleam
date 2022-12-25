//! This file is generated by build.rs
//! Do not edit it directly, instead add new test cases to ./cases

#[test]
fn erlang_import() {
    let output =
        crate::prepare("./cases/erlang_import");
    insta::assert_snapshot!(
        "erlang_import",
        output,
        "./cases/erlang_import"
    );
}

#[test]
fn opaque_type_destructure() {
    let output =
        crate::prepare("./cases/opaque_type_destructure");
    insta::assert_snapshot!(
        "opaque_type_destructure",
        output,
        "./cases/opaque_type_destructure"
    );
}

#[test]
fn erlang_nested() {
    let output =
        crate::prepare("./cases/erlang_nested");
    insta::assert_snapshot!(
        "erlang_nested",
        output,
        "./cases/erlang_nested"
    );
}

#[test]
fn erlang_nested_qualified_constant() {
    let output =
        crate::prepare("./cases/erlang_nested_qualified_constant");
    insta::assert_snapshot!(
        "erlang_nested_qualified_constant",
        output,
        "./cases/erlang_nested_qualified_constant"
    );
}

#[test]
fn erlang_escape_names() {
    let output =
        crate::prepare("./cases/erlang_escape_names");
    insta::assert_snapshot!(
        "erlang_escape_names",
        output,
        "./cases/erlang_escape_names"
    );
}

#[test]
fn opaque_type_accessor() {
    let output =
        crate::prepare("./cases/opaque_type_accessor");
    insta::assert_snapshot!(
        "opaque_type_accessor",
        output,
        "./cases/opaque_type_accessor"
    );
}

#[test]
fn src_importing_test() {
    let output =
        crate::prepare("./cases/src_importing_test");
    insta::assert_snapshot!(
        "src_importing_test",
        output,
        "./cases/src_importing_test"
    );
}

#[test]
fn erlang_app_generation() {
    let output =
        crate::prepare("./cases/erlang_app_generation");
    insta::assert_snapshot!(
        "erlang_app_generation",
        output,
        "./cases/erlang_app_generation"
    );
}

#[test]
fn import_shadowed_name_warning() {
    let output =
        crate::prepare("./cases/import_shadowed_name_warning");
    insta::assert_snapshot!(
        "import_shadowed_name_warning",
        output,
        "./cases/import_shadowed_name_warning"
    );
}

#[test]
fn javascript_d_ts() {
    let output =
        crate::prepare("./cases/javascript_d_ts");
    insta::assert_snapshot!(
        "javascript_d_ts",
        output,
        "./cases/javascript_d_ts"
    );
}

#[test]
fn unknown_module_field_in_import() {
    let output =
        crate::prepare("./cases/unknown_module_field_in_import");
    insta::assert_snapshot!(
        "unknown_module_field_in_import",
        output,
        "./cases/unknown_module_field_in_import"
    );
}

#[test]
fn erlang_bug_752() {
    let output =
        crate::prepare("./cases/erlang_bug_752");
    insta::assert_snapshot!(
        "erlang_bug_752",
        output,
        "./cases/erlang_bug_752"
    );
}

#[test]
fn unknown_module_field_in_constant() {
    let output =
        crate::prepare("./cases/unknown_module_field_in_constant");
    insta::assert_snapshot!(
        "unknown_module_field_in_constant",
        output,
        "./cases/unknown_module_field_in_constant"
    );
}

#[test]
fn duplicate_module() {
    let output =
        crate::prepare("./cases/duplicate_module");
    insta::assert_snapshot!(
        "duplicate_module",
        output,
        "./cases/duplicate_module"
    );
}

#[test]
fn imported_external_fns() {
    let output =
        crate::prepare("./cases/imported_external_fns");
    insta::assert_snapshot!(
        "imported_external_fns",
        output,
        "./cases/imported_external_fns"
    );
}

#[test]
fn erlang_empty() {
    let output =
        crate::prepare("./cases/erlang_empty");
    insta::assert_snapshot!(
        "erlang_empty",
        output,
        "./cases/erlang_empty"
    );
}

#[test]
fn import_cycle() {
    let output =
        crate::prepare("./cases/import_cycle");
    insta::assert_snapshot!(
        "import_cycle",
        output,
        "./cases/import_cycle"
    );
}

#[test]
fn alias_unqualified_import() {
    let output =
        crate::prepare("./cases/alias_unqualified_import");
    insta::assert_snapshot!(
        "alias_unqualified_import",
        output,
        "./cases/alias_unqualified_import"
    );
}

#[test]
fn hello_joe() {
    let output =
        crate::prepare("./cases/hello_joe");
    insta::assert_snapshot!(
        "hello_joe",
        output,
        "./cases/hello_joe"
    );
}

#[test]
fn imported_record_constructors() {
    let output =
        crate::prepare("./cases/imported_record_constructors");
    insta::assert_snapshot!(
        "imported_record_constructors",
        output,
        "./cases/imported_record_constructors"
    );
}

#[test]
fn unknown_module_field_in_expression() {
    let output =
        crate::prepare("./cases/unknown_module_field_in_expression");
    insta::assert_snapshot!(
        "unknown_module_field_in_expression",
        output,
        "./cases/unknown_module_field_in_expression"
    );
}

#[test]
fn javascript_import() {
    let output =
        crate::prepare("./cases/javascript_import");
    insta::assert_snapshot!(
        "javascript_import",
        output,
        "./cases/javascript_import"
    );
}

#[test]
fn variable_or_module() {
    let output =
        crate::prepare("./cases/variable_or_module");
    insta::assert_snapshot!(
        "variable_or_module",
        output,
        "./cases/variable_or_module"
    );
}

#[test]
fn imported_constants() {
    let output =
        crate::prepare("./cases/imported_constants");
    insta::assert_snapshot!(
        "imported_constants",
        output,
        "./cases/imported_constants"
    );
}

#[test]
fn erlang_import_shadowing_prelude() {
    let output =
        crate::prepare("./cases/erlang_import_shadowing_prelude");
    insta::assert_snapshot!(
        "erlang_import_shadowing_prelude",
        output,
        "./cases/erlang_import_shadowing_prelude"
    );
}

#[test]
fn javascript_empty() {
    let output =
        crate::prepare("./cases/javascript_empty");
    insta::assert_snapshot!(
        "javascript_empty",
        output,
        "./cases/javascript_empty"
    );
}

#[test]
fn opaque_type_construct() {
    let output =
        crate::prepare("./cases/opaque_type_construct");
    insta::assert_snapshot!(
        "opaque_type_construct",
        output,
        "./cases/opaque_type_construct"
    );
}

