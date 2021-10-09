use codegen::ErlangApp;

use crate::{
    build::{
        dep_tree, package_compiler, package_compiler::PackageCompiler, project_root::ProjectRoot,
        telemetry::Telemetry, Origin, Package, Target,
    },
    codegen,
    config::PackageConfig,
    io::{FileSystemIO, FileSystemWriter},
    paths, type_, warning, Error, Warning,
};
use std::{collections::HashMap, path::PathBuf};

#[derive(Debug)]
pub struct ProjectCompiler<'a, IO> {
    root: &'a ProjectRoot,
    root_config: PackageConfig,
    configs: HashMap<String, PackageConfig>,
    packages: HashMap<String, Package>,
    importable_modules: HashMap<String, type_::Module>,
    defined_modules: HashMap<String, PathBuf>,
    warnings: Vec<Warning>,
    telemetry: Box<dyn Telemetry>,
    io: IO,
}

// TODO: test top level package has test modules compiled
// TODO: test that tests cannot be imported into src
// TODO: test that dep cycles are not allowed between packages

impl<'a, IO> ProjectCompiler<'a, IO>
where
    IO: FileSystemIO + Clone,
{
    pub fn new(
        root: &'a ProjectRoot,
        root_config: PackageConfig,
        configs: HashMap<String, PackageConfig>,
        telemetry: Box<dyn Telemetry>,
        io: IO,
    ) -> Self {
        let estimated_number_of_modules = configs.len() * 5;
        Self {
            packages: HashMap::with_capacity(configs.len()),
            importable_modules: HashMap::with_capacity(estimated_number_of_modules),
            defined_modules: HashMap::with_capacity(estimated_number_of_modules),
            warnings: Vec::new(),
            root_config,
            telemetry,
            configs,
            root,
            io,
        }
    }

    pub fn compile(mut self) -> Result<HashMap<String, Package>, Error> {
        // Determine package processing order
        let sequence = order_packages(&self.configs)?;

        // Read and type check deps packages
        for name in sequence {
            let config = self.configs.remove(&name).expect("Missing package config");
            self.compile_package(name, config, SourceLocations::Src)?;
        }

        // Read and type check top level package
        let root_config = std::mem::replace(&mut self.root_config, Default::default());
        let name = root_config.name.clone();
        self.compile_package(name, root_config, SourceLocations::SrcAndTest)?;

        Ok(self.packages)
    }

    fn compile_package(
        &mut self,
        name: String,
        config: PackageConfig,
        locations: SourceLocations,
    ) -> Result<(), Error> {
        self.telemetry.compiling_package(&name);
        let test_path = match locations {
            SourceLocations::SrcAndTest => Some(paths::build_deps_package_test(&name)),
            _ => None,
        };

        // TODO: this isn't the right location.
        let out_path = paths::build_deps_package_src(&name);
        let options = package_compiler::Options {
            target: Target::Erlang,
            src_path: paths::build_deps_package_src(&name),
            out_path: out_path.clone(),
            test_path,
            name: name.clone(),
        };

        let mut compiler = options.into_compiler(self.io.clone())?;

        // Compile project
        let compiled = compiler.compile(
            &mut self.warnings,
            &mut self.importable_modules,
            &mut self.defined_modules,
        )?;
        ErlangApp::new(&out_path).render(self.io.clone(), &config, &compiled.modules)?;

        let _ = self.packages.insert(name, compiled);
        Ok(())
    }
}

#[derive(Debug, PartialEq)]
enum SourceLocations {
    Src,
    SrcAndTest,
}

fn order_packages(configs: &HashMap<String, PackageConfig>) -> Result<Vec<String>, Error> {
    dep_tree::toposort_deps(configs.values().map(package_deps_for_graph).collect())
        .map_err(convert_deps_tree_error)
}

fn convert_deps_tree_error(e: dep_tree::Error) -> Error {
    match e {
        dep_tree::Error::Cycle(packages) => Error::PackageCycle { packages },
    }
}

fn package_deps_for_graph(config: &PackageConfig) -> (String, Vec<String>) {
    let name = config.name.to_string();
    let deps: Vec<_> = config
        .dependencies
        .iter()
        .map(|(dep, _)| dep.to_string())
        .collect();
    (name, deps)
}
