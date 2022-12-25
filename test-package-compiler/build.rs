use std::path::PathBuf;

pub fn main() {
    println!("cargo:rerun-if-changed=cases");

    let mut module = "//! This file is generated by build.rs
//! Do not edit it directly, instead add new test cases to ./cases

"
    .to_string();

    let cases = PathBuf::from("./cases");

    for entry in std::fs::read_dir(&cases).unwrap() {
        let name = entry.unwrap().file_name().into_string().unwrap();
        let path = cases.join(&name);
        let path = path.to_str().unwrap();
        module.push_str(&format!(
            r#"#[test]
fn {name}() {{
    let output =
        crate::prepare("{path}");
    insta::assert_snapshot!(
        "{name}",
        output,
        "{path}"
    );
}}

"#
        ));
    }

    let out = PathBuf::from("./src/generated_tests.rs");
    std::fs::write(out, module).unwrap();
}
