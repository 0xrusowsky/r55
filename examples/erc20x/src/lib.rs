#![no_std]
#![no_main]

use core::default::Default;

use alloy_core::{hex, primitives::{Address, Bytes, U256}};
use contract_derive::{contract, CustomError, show_streams};

extern crate alloc;

use erc20::{IERC20, ERC20Error};

#[derive(Default)]
pub struct ERC20x;

#[contract]
impl ERC20x {
    pub fn x_balance_of(&self, owner: Address, target: Address) -> U256 {
        let token = IERC20::new(target);
        token.balance_of(owner)
    }

    pub fn x_mint(&self,
        owner: Address,
        amount: U256,
        target: Address
    ) -> Result<(), ERC20Error> {
        let token = IERC20::new(target);

        // easy to leverage rust's powerful enums like `Result<T, E>`
        if let Err(e) = token.mint(owner, amount) {
            match e {
                ERC20Error::InsufficientFunds(_req, available) => {
                    token.mint(owner, available)?
                },
                ERC20Error::InsufficientAllowance(_req, available) => {
                    token.mint(owner, available)?
                },
                other => return Err(other)
            }
        };

        Ok(())
    }
}
