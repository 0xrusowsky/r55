use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
};
use toml::Value;
use tracing::{debug, info};

use crate::compile::{CompileError, ContractProject, ContractTarget, GeneratedContract};

/// Generate temporary crates for all contract targets in the given projects
pub fn generate_temporary_crates(
    projects: &[ContractProject],
    temp_dir: &Path,
    project_root: &Path,
) -> Result<Vec<GeneratedContract>, CompileError> {
    let mut generated_contracts = Vec::new();

    for project in projects {
        debug!("Generating temporary crates for project: {}", project.name);

        for target in &project.targets {
            let target_temp_dir = temp_dir.join(&project.name).join(&target.module);

            // Create the temporary directory structure
            fs::create_dir_all(&target_temp_dir)?;
            fs::create_dir_all(target_temp_dir.join("src"))?;

            // Populate the temp dir with the modified files from the working dir 
            generate_cargo_toml(&project, target, &target_temp_dir, project_root)?;
            copy_source_and_shared_modules(&project, target, &target_temp_dir)?;
            create_cargo_config(&target_temp_dir, project_root)?;

            // Add to generated contracts
            let dependencies = build_dependency_list(&project, target);
            generated_contracts.push(GeneratedContract {
                path: target_temp_dir,
                name: target.generated_package.clone(),
                deps: dependencies,
                original_source_path: target
                    .source_file
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .to_path_buf(),
            });
        }
    }

    Ok(generated_contracts)
}

/// Generate Cargo.toml for a temporary crate
fn generate_cargo_toml(
    example: &ContractProject,
    target: &ContractTarget,
    target_dir: &Path,
    project_root: &Path,
) -> Result<(), CompileError> {
    // Start with a basic template
    let mut cargo_toml = format!(
        r#"[package]
name = "{}"
version = "0.1.0"
edition = "2021"

[workspace]

[features]
default = []
deploy = []
deployable = []
interface-only = []

[dependencies]
"#,
        target.generated_package
    );

    // Add all dependencies - both base and marker
    let mut processed_deps = std::collections::HashSet::new();

    // First add base dependencies with adjusted paths
    for (name, details) in &example.base_deps {
        // Don't skip marker dependencies - we need them for external crates
        processed_deps.insert(name.clone());

        // Add the dependency
        if let Some(dep_table) = details.as_table() {
            let mut dep_entry = format!("{} = {{", name);
            let mut first = true;

            // Handle path dependencies specially - adjust relative paths
            if let Some(Value::String(rel_path)) = dep_table.get("path") {
                let source_rel_path = Path::new(rel_path);
                let source_abs_path = example.path.join(source_rel_path).canonicalize()?;

                // Calculate relative path from target_dir to the dependency
                let rel_path_from_target = pathdiff::diff_paths(&source_abs_path, target_dir)
                    .ok_or_else(|| {
                        CompileError::PathError(format!(
                            "Failed to calculate relative path from {:?} to {:?}",
                            target_dir, source_abs_path
                        ))
                    })?;

                if !first {
                    dep_entry.push_str(", ");
                }
                first = false;
                dep_entry.push_str(&format!("path = \"{}\"", rel_path_from_target.display()));
            }

            // For other dependencies, just copy the original entry
            for (k, v) in dep_table {
                if k == "path" {
                    continue; // Handled above
                }

                let formatted_value = format_toml_value(v);
                if !first {
                    dep_entry.push_str(", ");
                }
                first = false;
                dep_entry.push_str(&format!("{} = {}", k, formatted_value));
            }

            // Close the dependency entry
            dep_entry.push_str("}");
            cargo_toml.push_str(&format!("{}\n", dep_entry));
        } else {
            // Simple dependency format
            cargo_toml.push_str(&format!("{} = {:?}\n", name, details));
        }
    }

    // Add dependencies on other generated contracts from the same example
    for (other_target_name, _) in &example.deployable_deps {
        // Skip if already processed or self-reference
        if processed_deps.contains(other_target_name)
            || other_target_name == &target.generated_package
        {
            continue;
        }

        // Add this marker dependency
        processed_deps.insert(other_target_name.clone());

        // Extract module name from the target name (example-module format)
        let parts: Vec<&str> = other_target_name.split('-').collect();
        if parts.len() < 2 {
            continue;
        }

        let other_module = parts[1..].join("-");

        // Calculate relative path to the other target
        let other_target_path = format!("../{}", other_module);

        cargo_toml.push_str(&format!(
            "{} = {{ path = \"{}\", features = [\"interface-only\"] }}\n",
            other_target_name, other_target_path
        ));
    }

    // Add bin targets
    cargo_toml.push_str(
        r#"
[[bin]]
name = "runtime"
path = "src/lib.rs"

[[bin]]
name = "deploy"
path = "src/lib.rs"
required-features = ["deploy"]

[profile.release]
lto = true
opt-level = "z"
"#,
    );

    // Write to file
    fs::write(target_dir.join("Cargo.toml"), cargo_toml)?;

    Ok(())
}

/// Format a TOML value as a string
fn format_toml_value(value: &Value) -> String {
    match value {
        Value::String(s) => format!("\"{}\"", s),
        Value::Integer(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Boolean(b) => b.to_string(),
        Value::Datetime(dt) => format!("\"{}\"", dt),
        Value::Array(arr) => {
            if arr.is_empty() {
                return "[]".to_string();
            }

            let items: Vec<String> = arr.iter().map(format_toml_value).collect();
            format!("[{}]", items.join(", "))
        }
        Value::Table(table) => {
            if table.is_empty() {
                return "{}".to_string();
            }

            let mut result = String::from("{ ");
            let mut first = true;

            for (k, v) in table {
                if !first {
                    result.push_str(", ");
                }
                first = false;
                result.push_str(&format!("{} = {}", k, format_toml_value(v)));
            }

            result.push_str(" }");
            result
        }
    }
}

/// Copy source files and shared modules to the temporary crate
fn copy_source_and_shared_modules(
    project: &ContractProject,
    target: &ContractTarget,
    target_dir: &Path,
) -> Result<(), CompileError> {
    let src_dir = target_dir.join("src");

    // Read the contract source file
    let source_content = fs::read_to_string(&target.source_file)?;

    // Write as lib.rs in the temporary crate
    fs::write(src_dir.join("lib.rs"), source_content)?;

    // Copy shared modules
    for module_name in &project.shared_modules {
        let module_path = project.path.join("src").join(format!("{}.rs", module_name));

        if module_path.exists() {
            let module_content = fs::read_to_string(&module_path)?;
            fs::write(src_dir.join(format!("{}.rs", module_name)), module_content)?;
        } else {
            // Try module/mod.rs structure
            let mod_dir_path = project.path.join("src").join(module_name);
            let mod_file_path = mod_dir_path.join("mod.rs");

            if mod_file_path.exists() {
                // Create the module directory
                let target_mod_dir = src_dir.join(module_name);
                fs::create_dir_all(&target_mod_dir)?;

                // Copy mod.rs
                let mod_content = fs::read_to_string(&mod_file_path)?;
                fs::write(target_mod_dir.join("mod.rs"), mod_content)?;

                // Copy all other files in the module directory
                if let Ok(entries) = fs::read_dir(mod_dir_path) {
                    for entry in entries.filter_map(|e| e.ok()) {
                        let path = entry.path();
                        if path.is_file() && path.file_name().unwrap() != "mod.rs" {
                            let file_name = path.file_name().unwrap();
                            fs::copy(&path, target_mod_dir.join(file_name))?;
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Create .cargo/config.toml in the temporary crate
fn create_cargo_config(target_dir: &Path, project_root: &Path) -> Result<(), CompileError> {
    let cargo_dir = target_dir.join(".cargo");
    fs::create_dir_all(&cargo_dir)?;

    // Calculate relative path from target directory to project root's r5-rust-rt.x
    let rust_rt_path = project_root.join("r5-rust-rt.x");
    let rel_rust_rt_path = pathdiff::diff_paths(&rust_rt_path, target_dir).ok_or_else(|| {
        CompileError::PathError(format!(
            "Failed to calculate relative path from {:?} to {:?}",
            target_dir, rust_rt_path
        ))
    })?;

    let config_content = format!(
        r#"[target.riscv64imac-unknown-none-elf]
rustflags = [
  "-C", "link-arg=-T{}",
  "-C", "llvm-args=--inline-threshold=275"
]

[build]
target = "riscv64imac-unknown-none-elf"
"#,
        rel_rust_rt_path.display()
    );

    fs::write(cargo_dir.join("config.toml"), config_content)?;

    Ok(())
}

/// Build a list of dependencies for a contract
fn build_dependency_list(project: &ContractProject, target: &ContractTarget) -> Vec<String> {
    let mut dependencies = Vec::new();

    for (dep_name, _) in &project.deployable_deps {
        // Skip self-reference
        if dep_name == &target.generated_package {
            continue;
        }

        dependencies.push(dep_name.clone());
    }

    dependencies
}
