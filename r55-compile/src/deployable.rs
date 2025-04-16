use std::{fs, path::Path};
use tracing::{debug, info};

use crate::compile::{CompileError, GeneratedContract};

pub fn generate_deployable(
    contract: &GeneratedContract,
    target_source: bool,
) -> Result<(), CompileError> {
    if contract.deps.is_empty() {
        return Ok(());
    }

    // Generate the file content
    let mut content = String::new();

    // Add header comments + common imports
    content.push_str("//! Auto-generated based on Cargo.toml dependencies\n");
    content.push_str(
        "//! This file provides `Deployable` implementations for contract dependencies\n",
    );
    content.push_str("//! TODO (phase-2): rather than using `fn deploy(args: Args)`, figure out the constructor selector from the contract dependency\n\n");

    content.push_str("use alloy_core::primitives::{Address, Bytes};\n");
    content.push_str("use eth_riscv_runtime::{create::Deployable, InitInterface, ReadOnly};\n");
    content.push_str("use core::include_bytes;\n\n");

    // Add imports for each dependency
    for dep_name in &contract.deps {
        // Keep original module name (lowercase package name)
        let module_name = dep_name.to_lowercase();

        // For interface name, use uppercase I + camel case name (IERC20)
        let interface_name = format!("I{}", extract_contract_name(dep_name));

        content.push_str(&format!("use {}::{};\n", module_name, interface_name));
    }
    content.push('\n');

    // Add bytecode constants for each dependency
    for dep_name in &contract.deps {
        // Use uppercase for constant name
        let const_name = dep_name.to_uppercase().replace('-', "_");

        // Calculate the output bytecode path relative to the contract's directory
        let bytecode_path =
            Path::new("../../../../r55-output-bytecode").join(format!("{}.bin", dep_name));

        content.push_str(&format!(
            "const {}_BYTECODE: &'static [u8] = include_bytes!(\"{}\");\n",
            const_name,
            bytecode_path.display()
        ));
    }
    content.push('\n');

    // Add Deployable implementation for each dependency
    for dep_name in &contract.deps {
        // Use proper case for struct name (ERC20, not erc20)
        let struct_name = extract_contract_name(dep_name);
        let interface_name = format!("I{}", struct_name);

        content.push_str(&format!("pub struct {};\n\n", struct_name));
        content.push_str(&format!("impl Deployable for {} {{\n", struct_name));
        content.push_str(&format!(
            "    type Interface = {}<ReadOnly>;\n\n",
            interface_name
        ));
        content.push_str("    fn __runtime() -> &'static [u8] {\n");
        content.push_str(&format!(
            "        {}_BYTECODE\n",
            dep_name.to_uppercase().replace('-', "_")
        ));
        content.push_str("    }\n");
        content.push_str("}\n\n");
    }

    // Write to file
    let output_path = if target_source {
        debug!("TEMP DIR: {:?}", contract.path);
        contract.path.join("src").join("deployable.rs")
    } else {
        debug!("WORKING DIR: {:?}", contract.original_source_path);
        contract
            .original_source_path
            .join("src")
            .join("deployable.rs")
    };
    fs::write(&output_path, content)?;

    info!(
        "Generated {:?} for contract: {}",
        output_path, contract.name
    );

    Ok(())
}

/// Extract a properly cased contract name from a package name
fn extract_contract_name(package_name: &str) -> String {
    // For simple names like "erc20", capitalize everything
    if !package_name.contains('-') {
        return package_name.to_uppercase();
    }

    // For compound names like "erc20-token", extract and capitalize the relevant part
    let parts: Vec<&str> = package_name.split('-').collect();
    if parts.len() <= 1 {
        return package_name.to_uppercase();
    }

    // Typically the contract name would be after the first dash
    // e.g., "uniswap-v2-pair" -> "Pair"
    let contract_part = parts[1];

    // For special cases like "erc20" or "erc721", uppercase entirely
    if contract_part.to_lowercase().starts_with("erc") {
        return contract_part.to_uppercase();
    }

    // Otherwise, capitalize first letter
    let capitalized = contract_part
        .chars()
        .next()
        .unwrap_or('C')
        .to_uppercase()
        .collect::<String>()
        + &contract_part[1..];
    capitalized
}
