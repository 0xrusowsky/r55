#![no_std]

extern crate alloc;
use alloy_core::primitives::{Address, Bytes, U32};
use alloy_sol_types::{SolType, SolValue};
use ext_alloc::vec::Vec;
use core::arch::asm;
use eth_riscv_syscalls::Syscall;
use super::{CallCtx, MutableCtx};

pub trait Deployable {
    type Interface;
    type ConstructorArgs: SolValue
        + core::convert::From<<<Self::ConstructorArgs as SolValue>::SolType as SolType>::RustType>;

    /// Creates an interface instance from a contract address
    fn interface(addr: Address) -> Self::Interface;

    /// Returns the contract bytecode
    fn bytecode() -> &'static [u8];

    /// Deploys a new contract instance
    fn deploy(args: Self::ConstructorArgs) -> Self::Interface {
        let bytecode = Self::bytecode();
        let encoded_args = args.abi_encode();

        // Craft R55 initcode: [0xFF][codesize][bytecode][constructor_args]
        let codesize = U32::from(bytecode.len());

        let mut init_code = Vec::new();
        init_code.push(0xff);
        init_code.extend_from_slice(&Bytes::from(codesize.to_be_bytes_vec()));
        init_code.extend_from_slice(bytecode);
        init_code.extend_from_slice(&encoded_args);

        let offset = init_code.as_ptr() as u64;
        let size = init_code.len() as u64;
        // TODO: think of an ergonomic API to handle deployments with values
        let addr = create(0, offset, size);

        Self::interface(addr)
    }
}

pub fn create(value: u64, data_offset: u64, data_size: u64) -> Address {
    let (first, second, third): (u64, u64, u64);
    unsafe {
        asm!(
            "ecall",
            in("a0") value, in("a1") data_offset, in("a2") data_size,
            lateout("a0") first, lateout("a1") second, lateout("a2") third,
            in("t0") u8::from(Syscall::Create)
        );
    }
    let mut bytes = [0u8; 20];
    bytes[0..8].copy_from_slice(&first.to_be_bytes());
    bytes[8..16].copy_from_slice(&second.to_be_bytes());
    bytes[16..20].copy_from_slice(&third.to_be_bytes()[..4]);
    Address::from_slice(&bytes)
}
