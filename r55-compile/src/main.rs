mod compile;
mod deployable;
mod generate;

use compile::{find_r55_contract_projects, sort_generated_contracts};
use generate::generate_temporary_crates;

use std::{fs, path::Path};
use tracing::{debug, info};

fn main() -> eyre::Result<()> {
    // Initialize logging
    let tracing_sub = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_target(false)
        .finish();
    tracing::subscriber::set_global_default(tracing_sub)?;

    // Setup output directory
    let project_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let output_dir = project_root.join("r55-output-bytecode");
    fs::create_dir_all(&output_dir)?;

    // Setup temporary directory for generated crates
    let temp_dir = project_root.join("target").join("r55-generated");
    fs::create_dir_all(&temp_dir)?;

    // Find all R55 example units in examples directory
    let examples_dir = project_root.join("examples");
    let projects = find_r55_contract_projects(&examples_dir)?;

    // Log discovered examples and their contracts
    info!("Found {} R55 project:", projects.len());
    for (i, example) in projects.iter().enumerate() {
        info!(
            "  {}. {} with {} contracts:",
            i + 1,
            example.name,
            example.targets.len()
        );
        for target in &example.targets {
            info!("     - {}", target.ident);
        }
    }

    // Generate temporary crates for all contracts
    let generated_contracts = generate_temporary_crates(&projects, &temp_dir, project_root)?;

    // Sort contracts in the correct compilation order
    let sorted_contracts = sort_generated_contracts(generated_contracts)?;

    debug!("Compilation order:");
    for (i, contract) in sorted_contracts.iter().enumerate() {
        debug!("  {}. {}", i + 1, contract.name);
    }

    // Compile each contract in order
    for contract in sorted_contracts {
        info!("Generating deployable.rs for contract: {}", contract.name);

        // Generate `deployable.rs` in the working crate
        deployable::generate_deployable(&contract, true)?;
        // Generate `deployable.rs` in the temporary crate
        deployable::generate_deployable(&contract, false)?;

        info!("Compiling contract: {}", contract.name);
        // Compile deployment code and save in the file
        let deploy_bytecode = contract.compile()?;
        let deploy_path = output_dir.join(format!("{}.bin", contract.name));
        fs::write(deploy_path, deploy_bytecode)?;
    }

    Ok(())
}
