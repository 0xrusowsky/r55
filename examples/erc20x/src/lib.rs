#![no_std]
#![no_main]

use core::default::Default;

use alloy_core::primitives::{Address, U256};
use contract_derive::{contract, CustomError, show_streams};

extern crate alloc;

use erc20::{IERC20, ERC20Error};

#[derive(Default)]
pub struct ERC20x;

#[derive(CustomError)]
pub enum ERC20xError {
    CallFailed
}

#[contract]
impl ERC20x {
    pub fn x_balance_of(&self, owner: Address, target: Address) -> U256 {
        let token = IERC20::new(target);
        token.balance_of(owner)
    }

    pub fn x_mint(&self, owner: Address, amount: U256, target: Address) -> Result<(), ERC20xError> {
        let token = IERC20::new(target);

        // easy to leverage rust's powerful enums like `Result<T, E>`
        token.mint(owner, amount).map_err(|_| ERC20xError::CallFailed)
    }

    pub fn x_mint_with_fallback(
        &self,
        owner: Address,
        amount: U256,
        target: Address
    ) -> Result<(), ERC20xError> {
        let token = IERC20::new(target);

        // easy to leverage rust's powerful enums like `Result<T, E>`
        token.mint(owner, amount).map_err(|_| ERC20xError::CallFailed)
    }

}
