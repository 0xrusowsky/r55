use std::{
    collections::{HashMap, HashSet},
    fmt, fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
};
use syn::{Attribute, Item, ItemImpl};
use thiserror::Error;
use toml::Value;
use tracing::{debug, error, info};

#[derive(Debug, Error)]
pub enum CompileError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Parsing error: {0}")]
    SynError(#[from] syn::Error),
    #[error("Invalid TOML format: {0}")]
    TomlError(#[from] toml::de::Error),
    #[error("Invalid path: {0}")]
    PathError(String),
    #[error("No contract found in file: {0}")]
    NoContractFound(String),
    #[error("Cyclic dependency")]
    CyclicDependency,
    #[error("Missing required deployable dependency: {0}")]
    MissingDeployableDependency(String),
}

/// Represents a contract target within a project
#[derive(Debug, Clone)]
pub struct ContractTarget {
    /// The contract struct name
    pub ident: String,
    /// The module name where the contract is defined
    pub module: String,
    /// Path to the source file
    pub source_file: PathBuf,
    /// Generated package name
    pub generated_package: String,
}

/// Represents a source project that may contain multiple smart-contracts
#[derive(Debug, Clone)]
pub struct ContractProject {
    /// Directory path of the example
    pub path: PathBuf,
    /// Name of the project directory
    pub name: String,
    /// Contract targets within this project
    pub targets: Vec<ContractTarget>,
    /// Shared modules used by contracts
    pub shared_modules: HashSet<String>,
    /// Base dependencies from `Cargo.toml`
    pub base_deps: HashMap<String, Value>,
    /// Deployable contract dependencies
    pub deployable_deps: HashMap<String, String>,
}

/// Represents a generated (temporary) crate under `target/`
#[derive(Debug, Clone)]
pub struct GeneratedContract {
    /// Path to the generated crate
    pub path: PathBuf,
    /// Package name of the generated crate
    pub name: String,
    /// Dependencies on other generated contracts
    pub deps: Vec<String>,
    /// Original source file path
    pub original_source_path: PathBuf,
}

impl fmt::Display for GeneratedContract {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.deps.is_empty() {
            write!(f, "{}", self.name)
        } else {
            write!(f, "{} with deps: [", self.name)?;
            for (i, dep) in self.deps.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{}", dep)?;
            }
            write!(f, "]")
        }
    }
}

impl GeneratedContract {
    pub fn compile(&self) -> eyre::Result<Vec<u8>> {
        // First compile runtime
        self.compile_runtime()?;

        // Then compile deployment code
        let bytecode = self.compile_deploy()?;
        let mut prefixed_bytecode = vec![0xff]; // Add the 0xff prefix
        prefixed_bytecode.extend_from_slice(&bytecode);

        Ok(prefixed_bytecode)
    }

    fn compile_runtime(&self) -> eyre::Result<Vec<u8>> {
        debug!("Compiling runtime: {}", self.name);

        let path = self
            .path
            .to_str()
            .ok_or_else(|| eyre::eyre!("Failed to convert path to string: {:?}", self.path))?;

        let status = Command::new("cargo")
            .arg("+nightly-2025-01-07")
            .arg("build")
            .arg("-r")
            .arg("--lib")
            .arg("-Z")
            .arg("build-std=core,alloc")
            .arg("--target")
            .arg("riscv64imac-unknown-none-elf")
            .arg("--bin")
            .arg("runtime")
            .current_dir(path)
            .status()
            .expect("Failed to execute cargo command");

        if !status.success() {
            error!("Cargo command failed with status: {}", status);
            std::process::exit(1);
        } else {
            info!("Cargo command completed successfully");
        }

        let bin_path = PathBuf::from(path)
            .join("target")
            .join("riscv64imac-unknown-none-elf")
            .join("release")
            .join("runtime");

        let mut file = fs::File::open(&bin_path).map_err(|e| {
            eyre::eyre!(
                "Failed to open runtime binary {}: {}",
                bin_path.display(),
                e
            )
        })?;

        // Read the file contents into a vector
        let mut bytecode = Vec::new();
        file.read_to_end(&mut bytecode)
            .map_err(|e| eyre::eyre!("Failed to read runtime binary: {}", e))?;

        Ok(bytecode)
    }

    // Requires previous runtime compilation
    fn compile_deploy(&self) -> eyre::Result<Vec<u8>> {
        debug!("Compiling deploy: {}", self.name);

        let path = self
            .path
            .to_str()
            .ok_or_else(|| eyre::eyre!("Failed to convert path to string: {:?}", self.path))?;

        let status = Command::new("cargo")
            .arg("+nightly-2025-01-07")
            .arg("build")
            .arg("-r")
            .arg("--lib")
            .arg("-Z")
            .arg("build-std=core,alloc")
            .arg("--target")
            .arg("riscv64imac-unknown-none-elf")
            .arg("--bin")
            .arg("deploy")
            .arg("--features")
            .arg("deploy")
            .current_dir(path)
            .status()
            .expect("Failed to execute cargo command");

        if !status.success() {
            error!("Cargo command failed with status: {}", status);
            std::process::exit(1);
        } else {
            info!("Cargo command completed successfully");
        }

        let bin_path = PathBuf::from(path)
            .join("target")
            .join("riscv64imac-unknown-none-elf")
            .join("release")
            .join("deploy");

        let mut file = fs::File::open(&bin_path).map_err(|e| {
            eyre::eyre!("Failed to open deploy binary {}: {}", bin_path.display(), e)
        })?;

        // Read the file contents into a vector
        let mut bytecode = Vec::new();
        file.read_to_end(&mut bytecode)
            .map_err(|e| eyre::eyre!("Failed to read deploy binary: {}", e))?;

        Ok(bytecode)
    }
}

/// Finds all R55 smart-contract projects in a directory
pub fn find_r55_contract_projects(dir: &Path) -> Result<Vec<ContractProject>, CompileError> {
    let mut examples = Vec::new();

    // Scan subdirectories for potential examples
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();

            // Skip if not a directory
            if !path.is_dir() {
                continue;
            }

            // Check for Cargo.toml
            let cargo_path = path.join("Cargo.toml");
            if !cargo_path.exists() {
                continue;
            }

            // Try to parse as R55 project unit
            match parse_contract_project(&cargo_path) {
                Ok(project) => {
                    examples.push(project);
                }
                Err(e) => {
                    debug!("Skipping directory {:?}: {}", path, e);
                }
            }
        }
    }

    Ok(examples)
}

/// Parse a smart-contract project directory into a `ContractProject`
fn parse_contract_project(cargo_toml_path: &Path) -> Result<ContractProject, CompileError> {
    let example_dir = cargo_toml_path.parent().ok_or_else(|| {
        CompileError::PathError(format!(
            "Failed to get parent directory of {:?}",
            cargo_toml_path
        ))
    })?;

    let example_name = example_dir
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| {
            CompileError::PathError(format!(
                "Failed to get directory name from {:?}",
                example_dir
            ))
        })?
        .to_string();

    // Parse Cargo.toml
    let cargo_content = fs::read_to_string(cargo_toml_path)?;
    let cargo_toml: Value = toml::from_str(&cargo_content)?;

    // Extract base package name
    let base_package_name = cargo_toml
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .ok_or_else(|| CompileError::PathError("Missing package.name in Cargo.toml".to_string()))?
        .to_string();

    // Extract base dependencies
    let mut base_deps = HashMap::new();
    let mut deployable_deps = HashMap::new();

    if let Some(Value::Table(deps)) = cargo_toml.get("dependencies") {
        for (name, details) in deps {
            // Check if this is a marker dependency with `deployable` feature
            if let Some(dep_table) = details.as_table() {
                if let Some(Value::Array(features)) = dep_table.get("features") {
                    if features.contains(&Value::String("deployable".to_string())) {
                        deployable_deps.insert(name.clone(), name.clone());
                    }
                }
            }

            // Add to base dependencies (even if it's also a marker)
            base_deps.insert(name.clone(), details.clone());
        }
    }

    // Scan src directory for contract targets and shared modules
    let src_dir = example_dir.join("src");
    let lib_rs_path = src_dir.join("lib.rs");

    if !lib_rs_path.exists() {
        return Err(CompileError::PathError(format!(
            "Missing src/lib.rs in {:?}",
            example_dir
        )));
    }

    // Parse lib.rs to find module declarations
    let lib_content = fs::read_to_string(&lib_rs_path)?;
    let lib_ast = syn::parse_file(&lib_content)?;

    let mut module_names = HashSet::new();
    for item in &lib_ast.items {
        if let Item::Mod(item_mod) = item {
            if item_mod.content.is_none() {
                // External module
                module_names.insert(item_mod.ident.to_string());
            }
        }
    }

    // Find contract targets in each module
    let mut targets = Vec::new();
    let mut shared_modules = HashSet::new();

    for module_name in &module_names {
        let module_path = src_dir.join(format!("{}.rs", module_name));

        if !module_path.exists() {
            // Try module/mod.rs structure
            let alt_module_path = src_dir.join(module_name).join("mod.rs");
            if !alt_module_path.exists() {
                debug!(
                    "Could not find module file for {} at {:?} or {:?}",
                    module_name, module_path, alt_module_path
                );
                continue;
            }
        }

        // Parse the module file
        let module_content = fs::read_to_string(&module_path)?;
        let module_ast = syn::parse_file(&module_content)?;

        // Look for #[contract] annotation on impl blocks
        let mut has_contract = false;
        for item in &module_ast.items {
            if let Item::Impl(item_impl) = item {
                if has_contract_attribute(&item_impl.attrs) {
                    // Found a contract
                    if let Some(struct_name) = extract_struct_name(item_impl) {
                        has_contract = true;

                        // Generate package name based on project name and module name
                        let generated_pkg_name = format!("{}-{}", example_name, module_name);

                        targets.push(ContractTarget {
                            ident: struct_name,
                            module: module_name.clone(),
                            source_file: module_path.clone(),
                            generated_package: generated_pkg_name,
                        });
                    }
                }
            }
        }

        if !has_contract {
            // If no contract was found, this is a shared module
            shared_modules.insert(module_name.clone());
        }
    }

    if targets.is_empty() {
        // No contracts found - try to find a contract in lib.rs itself
        for item in &lib_ast.items {
            if let Item::Impl(item_impl) = item {
                if has_contract_attribute(&item_impl.attrs) {
                    if let Some(struct_name) = extract_struct_name(item_impl) {
                        // When contract is in lib.rs, use the project name as the generated name
                        targets.push(ContractTarget {
                            ident: struct_name,
                            module: String::new(),
                            source_file: lib_rs_path.clone(),
                            generated_package: base_package_name.clone(),
                        });
                    }
                }
            }
        }
    }

    if targets.is_empty() {
        return Err(CompileError::NoContractFound(
            example_dir.to_string_lossy().into(),
        ));
    }

    Ok(ContractProject {
        path: example_dir.to_path_buf(),
        name: example_name,
        targets,
        shared_modules,
        base_deps,
        deployable_deps,
    })
}

/// Sort generated contracts based on their dependencies
pub fn sort_generated_contracts(
    contracts: Vec<GeneratedContract>,
) -> Result<Vec<GeneratedContract>, CompileError> {
    // Create dependency mapping
    let mut dependency_map: HashMap<String, Vec<String>> = HashMap::new();
    for contract in &contracts {
        dependency_map.insert(contract.name.clone(), contract.deps.clone());
    }

    // Keep track of sorted and remaining contracts
    let mut sorted = Vec::new();
    let mut remaining = contracts;

    // Continue until all contracts are sorted
    while !remaining.is_empty() {
        let initial_len = remaining.len();
        let mut next_remaining = Vec::new();

        for contract in remaining {
            let deps = dependency_map.get(&contract.name).unwrap();

            // Check if all dependencies are already in sorted list
            let all_deps_sorted = deps
                .iter()
                .all(|dep| sorted.iter().any(|c: &GeneratedContract| &c.name == dep));

            if all_deps_sorted {
                sorted.push(contract);
            } else {
                next_remaining.push(contract);
            }
        }

        remaining = next_remaining;

        // If no progress was made, we have a cycle
        if remaining.len() == initial_len && !remaining.is_empty() {
            return Err(CompileError::CyclicDependency);
        }
    }

    Ok(sorted)
}

fn has_contract_attribute(attrs: &[Attribute]) -> bool {
    attrs
        .iter()
        .any(|attr| attr.path.segments.len() == 1 && attr.path.segments[0].ident == "contract")
}

fn extract_struct_name(item_impl: &ItemImpl) -> Option<String> {
    match &*item_impl.self_ty {
        syn::Type::Path(type_path) if !type_path.path.segments.is_empty() => {
            // Get the last segment of the path (the type name)
            let segment = type_path.path.segments.last().unwrap();
            Some(segment.ident.to_string())
        }
        _ => None,
    }
}
