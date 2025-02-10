pub mod exec;

mod error;
mod gas;

pub mod test_utils;

use alloy_primitives::Bytes;
use std::fs::File;
use std::io::Read;
use std::process::Command;
use tracing::{error, info};

fn compile_runtime(path: &str) -> eyre::Result<Vec<u8>> {
    info!("Compiling runtime: {}", path);
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

    let path = format!(
        "{}/target/riscv64imac-unknown-none-elf/release/runtime",
        path
    );
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(e) => {
            eyre::bail!("Failed to open file: {}", e);
        }
    };

    // Read the file contents into a vector.
    let mut bytecode = Vec::new();
    if let Err(e) = file.read_to_end(&mut bytecode) {
        eyre::bail!("Failed to read file: {}", e);
    }

    Ok(bytecode)
}

pub fn compile_deploy(path: &str) -> eyre::Result<Vec<u8>> {
    compile_runtime(path)?;
    info!("Compiling deploy: {}", path);
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

    let path = format!(
        "{}/target/riscv64imac-unknown-none-elf/release/deploy",
        path
    );
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(e) => {
            eyre::bail!("Failed to open file: {}", e);
        }
    };

    // Read the file contents into a vector.
    let mut bytecode = Vec::new();
    if let Err(e) = file.read_to_end(&mut bytecode) {
        eyre::bail!("Failed to read file: {}", e);
    }

    Ok(bytecode)
}

pub fn compile_with_prefix<F>(compile_fn: F, path: &str) -> eyre::Result<Bytes>
where
    F: FnOnce(&str) -> eyre::Result<Vec<u8>>,
{
    let bytecode = compile_fn(path)?;
    let mut prefixed_bytecode = vec![0xff]; // Add the 0xff prefix
    prefixed_bytecode.extend_from_slice(&bytecode);
    Ok(Bytes::from(prefixed_bytecode))
}

#[cfg(test)]
mod tests {
    use crate::exec::{deploy_contract, run_tx};
    use crate::{compile_deploy, compile_with_prefix, test_utils::*};

    use alloy_core::hex::{self, ToHexExt};
    use alloy_core::primitives::address;
    use alloy_primitives::B256;
    use alloy_sol_types::SolValue;

    const ERC20_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/erc20");

    #[test]
    fn test_runtime() {
        initialize_logger();

        let mut db = InMemoryDB::default();

        // Setup addresses
        let alice: Address = address!("000000000000000000000000000000000000000A");
        let bob: Address = address!("000000000000000000000000000000000000000B");
        let carol: Address = address!("000000000000000000000000000000000000000C");

        // Add balance to Alice's account for gas fees
        add_balance_to_db(&mut db, alice, 1e18 as u64);

        // Compile + deploy contract
        let bytecode = compile_with_prefix(compile_deploy, ERC20_PATH).unwrap();
        let constructor = alice.abi_encode();
        let erc20 = deploy_contract(&mut db, bytecode, Some(constructor)).unwrap();

        // Define fn selectors
        let selector_owner = get_selector_from_sig("owner");
        let selector_total_supply = get_selector_from_sig("total_supply");
        let selector_balance = get_selector_from_sig("balance_of");
        let selector_mint = get_selector_from_sig("mint");
        let selector_transfer = get_selector_from_sig("transfer");
        let selector_approve = get_selector_from_sig("approve");
        let selector_allowance = get_selector_from_sig("allowance");

        // Check that Alice is the contract owner
        let owner_result = run_tx(&mut db, &erc20, selector_owner.to_vec())
            .expect("Error executing tx")
            .output;

        assert_eq!(
            B256::from_slice(owner_result.as_slice()),
            alice.into_word(),
            "Incorrect owner"
        );

        // Mint 42 tokens to Alice
        let value_mint = U256::from(42e18);
        let mut calldata_mint = (alice, value_mint).abi_encode();
        let mut complete_mint_calldata = selector_mint.to_vec();
        complete_mint_calldata.append(&mut calldata_mint);
        let mint_result = run_tx(&mut db, &erc20, complete_mint_calldata).unwrap();

        assert!(mint_result.status, "Mint transaction failed");

        // Check total supply
        let total_supply_result = run_tx(&mut db, &erc20, selector_total_supply.to_vec())
            .expect("Error executing tx")
            .output;

        assert_eq!(
            U256::from_be_bytes::<32>(total_supply_result.as_slice().try_into().unwrap()),
            value_mint,
            "Incorrect total supply"
        );

        // Check Alice's balance
        let mut calldata_alice_balance = alice.abi_encode();
        let mut complete_calldata_alice_balance = selector_balance.to_vec();
        complete_calldata_alice_balance.append(&mut calldata_alice_balance);
        let alice_balance_result = run_tx(&mut db, &erc20, complete_calldata_alice_balance.clone())
            .expect("Error executing tx")
            .output;

        assert_eq!(
            U256::from_be_bytes::<32>(alice_balance_result.as_slice().try_into().unwrap()),
            value_mint,
            "Incorrect balance"
        );

        // Transfer 21 tokens from Alice to Bob
        let value_transfer = U256::from(21e18);
        let mut calldata_transfer = (bob, value_transfer).abi_encode();
        let mut complete_calldata_transfer = selector_transfer.to_vec();
        complete_calldata_transfer.append(&mut calldata_transfer);
        let transfer_result = run_tx(&mut db, &erc20, complete_calldata_transfer.clone()).unwrap();
        assert!(transfer_result.status, "Transfer transaction failed");

        // Check Alice's balance
        let alice_balance_result = run_tx(&mut db, &erc20, complete_calldata_alice_balance.clone())
            .expect("Error executing tx")
            .output;

        assert_eq!(
            U256::from_be_bytes::<32>(alice_balance_result.as_slice().try_into().unwrap()),
            value_mint - value_transfer,
            "Incorrect balance"
        );

        // Check Bob's balance
        let mut calldata_bob_balance = bob.abi_encode();
        let mut complete_calldata_bob_balance = selector_balance.to_vec();
        complete_calldata_bob_balance.append(&mut calldata_bob_balance);
        let bob_balance_result = run_tx(&mut db, &erc20, complete_calldata_bob_balance.clone())
            .expect("Error executing tx")
            .output;

        assert_eq!(
            U256::from_be_bytes::<32>(bob_balance_result.as_slice().try_into().unwrap()),
            value_transfer,
            "Incorrect balance"
        );

        // Approve Carol to spend 10 tokens from Alice
        let value_approve = U256::from(10e18);
        let mut calldata_approve = (carol, value_approve).abi_encode();
        let mut complete_calldata_approve = selector_approve.to_vec();
        complete_calldata_approve.append(&mut calldata_approve);
        let approve_result = run_tx(&mut db, &erc20, complete_calldata_approve.clone()).unwrap();
        assert!(approve_result.status, "Approve transaction failed");

        // Check Carol's allowance
        let mut calldata_allowance = (alice, carol).abi_encode();
        let mut complete_calldata_allowance = selector_allowance.to_vec();
        complete_calldata_allowance.append(&mut calldata_allowance);
        let carol_allowance_result = run_tx(&mut db, &erc20, complete_calldata_allowance.clone())
            .expect("Error executing tx")
            .output;

        assert_eq!(
            U256::from_be_bytes::<32>(carol_allowance_result.as_slice().try_into().unwrap()),
            value_approve,
            "Incorrect balance"
        );
    }

    #[test]
    fn test_transfer_logs() {
        initialize_logger();

        let mut db = InMemoryDB::default();

        // Setup addresses
        let alice: Address = address!("000000000000000000000000000000000000000A");
        let bob: Address = address!("000000000000000000000000000000000000000B");

        // Add balance to Alice's account for gas fees
        add_balance_to_db(&mut db, alice, 1e18 as u64);

        // Compile + deploy contract
        let bytecode = compile_with_prefix(compile_deploy, ERC20_PATH).unwrap();
        let constructor = alice.abi_encode();
        let erc20 = deploy_contract(&mut db, bytecode, Some(constructor)).unwrap();

        // Mint tokens to Alice
        let selector_mint = get_selector_from_sig("mint");
        let mut calldata_mint = (alice, 100u64).abi_encode();
        let mut complete_mint_calldata = selector_mint.to_vec();
        complete_mint_calldata.append(&mut calldata_mint);

        let mint_result = run_tx(&mut db, &erc20, complete_mint_calldata).unwrap();
        assert!(mint_result.status, "Mint transaction failed");

        // Transfer tokens from Alice to Bob
        let selector_transfer = get_selector_from_sig("transfer");
        let mut calldata_transfer = (bob, 50u64).abi_encode();
        let mut complete_transfer_calldata = selector_transfer.to_vec();
        complete_transfer_calldata.append(&mut calldata_transfer);

        let transfer_result = run_tx(&mut db, &erc20, complete_transfer_calldata).unwrap();

        // Assert the transfer log
        assert!(
            !transfer_result.logs.is_empty(),
            "No logs found in transfer transaction"
        );
        let log = &transfer_result.logs[0];
        let topics = log.data.topics();

        // Expected event hash for Transfer event
        let expected_event_hash = keccak256("Transfer(address,address,uint256)");
        assert_eq!(
            hex::encode(topics[0]),
            hex::encode(expected_event_hash),
            "Incorrect event hash"
        );

        // Assert "from" address in log
        assert_eq!(
            hex::encode(&topics[1][12..]),
            alice.encode_hex(),
            "Incorrect 'from' address in transfer log"
        );

        // Assert "to" address in log
        assert_eq!(
            hex::encode(&topics[2][12..]),
            bob.encode_hex(),
            "Incorrect 'to' address in transfer log"
        );

        // Assert transfer amount
        let amount = U256::from_be_slice(log.data.data[..32].try_into().unwrap());
        assert_eq!(
            amount,
            U256::from(50),
            "Incorrect transfer amount in transfer log"
        );
    }

    #[test]
    fn test_storage_layout() {
        initialize_logger();

        let mut db = InMemoryDB::default();

        // Setup addresses
        let alice: Address = address!("000000000000000000000000000000000000000A");
        let bob: Address = address!("000000000000000000000000000000000000000B");
        let carol: Address = address!("000000000000000000000000000000000000000C");

        // Add balance to Alice's account for gas fees
        add_balance_to_db(&mut db, alice, 1e18 as u64);

        // Compile + deploy contract
        let bytecode = compile_with_prefix(compile_deploy, ERC20_PATH).unwrap();
        let constructor = alice.abi_encode();
        let erc20 = deploy_contract(&mut db, bytecode, Some(constructor)).unwrap();

        // Mint tokens to Alice
        let mint_alice = U256::from(10e18);
        let selector_mint = get_selector_from_sig("mint");
        let mut calldata_mint = (alice, mint_alice).abi_encode();
        let mut complete_mint_calldata = selector_mint.to_vec();
        complete_mint_calldata.append(&mut calldata_mint);

        let mint_result = run_tx(&mut db, &erc20, complete_mint_calldata).unwrap();
        assert!(mint_result.status, "Mint transaction failed");

        // Mint tokens to Bob
        let mint_bob = U256::from(20e18);
        let mut calldata_mint = (bob, mint_bob).abi_encode();
        let mut complete_mint_calldata = selector_mint.to_vec();
        complete_mint_calldata.append(&mut calldata_mint);

        let mint_result = run_tx(&mut db, &erc20, complete_mint_calldata).unwrap();
        assert!(mint_result.status, "Mint transaction failed");

        // Approve Carol to spend 10 tokens from Alice
        let allowance_carol = U256::from(5e18);
        let selector_approve = get_selector_from_sig("approve");
        let mut calldata_approve = (carol, allowance_carol).abi_encode();
        let mut complete_calldata_approve = selector_approve.to_vec();
        complete_calldata_approve.append(&mut calldata_approve);
        let approve_result = run_tx(&mut db, &erc20, complete_calldata_approve).unwrap();
        assert!(approve_result.status, "Approve transaction failed");

        // EXPECTED STORAGE LAYOUT:
        //
        // pub struct ERC20 {
        //     total_supply: Slot<U256>,                                Slot: 0
        //     balances: Mapping<Address, U256>,                        Slot: keccak256(address, 1)
        //     allowances: Mapping<Address, Mapping<Address, U256>>,    Slot: keccak256(address, keccak256(address, 2))
        //     owner: Slot<Address>,                                    Slot: 3
        // }

        // Assert `total_supply` is set to track the correct slot
        let expected_slot = U256::from(0);
        assert_eq!(
            mint_alice + mint_bob,
            read_db_slot(&mut db, erc20, expected_slot)
        );

        let balances_id = U256::from(1);
        // Assert `balances[alice]` is set to track the correct slot
        let expected_slot = get_mapping_slot(alice.abi_encode(), balances_id);
        assert_eq!(mint_alice, read_db_slot(&mut db, erc20, expected_slot));

        // Assert `balances[bob]` is set to track the correct slot
        let expected_slot = get_mapping_slot(bob.abi_encode(), balances_id);
        assert_eq!(mint_bob, read_db_slot(&mut db, erc20, expected_slot));

        let allowances_id = U256::from(2);
        // Assert `allowance[alice][carol]` is set to track the correct slot
        let id = get_mapping_slot(alice.abi_encode(), allowances_id);
        let expected_slot = get_mapping_slot(carol.abi_encode(), id);
        assert_eq!(allowance_carol, read_db_slot(&mut db, erc20, expected_slot));

        // Assert `owner` is set to track the correct slot
        let expected_slot = U256::from(3);
        assert_eq!(
            read_db_slot(&mut db, erc20, expected_slot),
            alice.into_word().into(),
        );
    }
}
