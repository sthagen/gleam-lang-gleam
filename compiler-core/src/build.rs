#![allow(warnings)]

// TODO: Avoid rebuilding clean modules
// TODO: Download deps from Hex
// TODO: Support compilation of rebar3 packages
// TODO: Track removed files in src and test so they can be removed from _build
// TODO: Test profile and default profile
// TODO: Only compile test code in test profile
// TODO: Full .app generation
// TODO: Validate config.otp_start_module does not contain '
// TODO: Validate config.otp_start_module has a start function
// - custom output paths
// - no .app generation
// - no Erlang generation

mod dep_tree;
pub mod package_compiler;
mod project_compiler;
mod telemetry;

#[cfg(test)]
mod package_compilation_tests;

pub use self::package_compiler::PackageCompiler;
pub use self::project_compiler::ProjectCompiler;
pub use self::telemetry::Telemetry;

use crate::{
    ast::TypedModule,
    config::{self, PackageConfig},
    erlang,
    error::{Error, FileIoAction, FileKind},
    io::OutputFile,
    type_,
};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, ffi::OsString, fs::DirEntry, path::PathBuf, process};
use strum::{Display, EnumString, EnumVariantNames, VariantNames};

#[derive(
    Debug, Serialize, Deserialize, Display, EnumString, EnumVariantNames, Clone, Copy, PartialEq,
)]
#[strum(serialize_all = "lowercase")]
pub enum Target {
    Erlang,
    JavaScript,
}

impl Target {
    pub fn variant_strings() -> Vec<String> {
        Self::VARIANTS.iter().map(|s| s.to_string()).collect()
    }
}

#[derive(
    Debug, Serialize, Deserialize, Display, EnumString, EnumVariantNames, Clone, Copy, PartialEq,
)]
#[strum(serialize_all = "lowercase")]
pub enum Mode {
    Dev,
    Prod,
}

#[derive(Debug)]
pub struct Package {
    pub name: String,
    pub modules: Vec<Module>,
}

#[derive(Debug)]
pub struct Module {
    pub name: String,
    pub code: String,
    pub input_path: PathBuf,
    pub origin: Origin,
    pub ast: TypedModule,
}

impl Module {
    pub fn compiled_erlang_path(&self) -> PathBuf {
        let mut path = self.name.replace("/", "@");
        path.push_str(".erl");
        PathBuf::from(path)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Origin {
    Src,
    Test,
}
