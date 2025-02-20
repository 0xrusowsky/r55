#![no_std]
#![no_main]

use core::default::Default;

use contract_derive::{contract, payable, storage, Event, Error};
use eth_riscv_runtime::types::*;

use alloy_core::primitives::{address, Address, U256, Bytes};

extern crate alloc;
use alloc::string::String;

// -- EVENTS -------------------------------------------------------------------
#[derive(Event)]
pub struct Transfer {
    #[indexed]
    pub from: Address,
    #[indexed]
    pub to: Address,
    #[indexed]
    pub id: U256,
}

#[derive(Event)]
pub struct Approval {
    #[indexed]
    pub owner: Address,
    #[indexed]
    pub spender: Address,
    #[indexed]
    pub id: U256,
}

#[derive(Event)]
pub struct ApprovalForAll {
    #[indexed]
    pub owner: Address,
    #[indexed]
    pub operator: Address,
    pub approved: bool,
}

#[derive(Event)]
pub struct OwnershipTransferred {
    #[indexed]
    pub from: Address,
    #[indexed]
    pub to: Address,
}

// -- ERRORS -------------------------------------------------------------------
#[derive(Error)]
pub enum ERC721Error {
    AlreadyMinted,
    NotMinted,
    OnlyOwner,
    Unauthorized,
    WrongFrom,
    ZeroAddress,
}

// -- CONTRACT -----------------------------------------------------------------
#[storage]
pub struct ERC721 {
    total_supply: Slot<U256>,
    owner_of: Mapping<U256, Slot<Address>>,
    balance_of: Mapping<Address, Slot<U256>>,
    approval_of: Mapping<U256, Slot<Address>>,
    is_operator: Mapping<Address, Mapping<Address, Slot<bool>>>,
    owner: Slot<Address>,
    // TODO: handle string storage
    // name: String, 
    // symbol: String,
}

#[contract]
impl ERC721 {
    // -- CONSTRUCTOR ----------------------------------------------------------
    pub fn new(owner: Address) -> Self {
        // Init the contract
        let mut erc721 = ERC721::default();

        // Store the owner
        erc721.owner.write(owner);

        // Return the initialized contract
        erc721
    }

    // -- STATE MODIFYING FUNCTIONS --------------------------------------------
    #[payable]
    pub fn mint(&mut self, to: Address, id: U256) -> Result<bool, ERC721Error> {
        // Perform sanity checks
        if to == Address::ZERO { return Err(ERC721Error::ZeroAddress) };
        if msg_sender() != self.owner.read() { return Err(ERC721Error::OnlyOwner) }; 
        if self.owner_of.read(id) != Address::ZERO { return Err(ERC721Error::AlreadyMinted) };

        // Update state
        self.owner_of.write(id, to);
        self.balance_of.write(to, self.balance_of.read(to) + U256::from(1));
        self.total_supply.write(self.total_supply.read() + U256::from(1));
        
        // Emit event + return
        log::emit(Transfer::new(Address::ZERO, to, id));
        Ok(true)
    }

    pub fn approve(&mut self, spender: Address, id: U256) -> Result<bool, ERC721Error> {
        let owner = self.owner_of.read(id);
        
        // Perform authorization check
        if msg_sender() != owner && !self.is_operator.read(owner).read(msg_sender()) {
            return Err(ERC721Error::Unauthorized);
        }

        // Update state
        self.approval_of.write(id, spender);

        // Emit event + return
        log::emit(Approval::new(owner, spender, id));
        Ok(true)
    }

    pub fn set_approval_for_all(&mut self, operator: Address, approved: bool) -> Result<bool, ERC721Error> {
        // Update state
        self.is_operator
            .read(msg_sender())
            .write(operator, approved);

        // Emit event + return
        log::emit(ApprovalForAll::new(msg_sender(), operator, approved));
        Ok(true)
    }

    pub fn transfer_from(&mut self, from: Address, to: Address, id: U256) -> Result<bool, ERC721Error> {
        // Perform sanity checks
        if from != self.owner_of.read(id) { return Err(ERC721Error::WrongFrom) };
        if to == Address::ZERO { return Err(ERC721Error::ZeroAddress) };

        // Check authorization
        let sender = msg_sender();
        if sender != from 
            && !self.is_operator.read(from).read(sender)
            && sender != self.approval_of.read(id) {
            return Err(ERC721Error::Unauthorized);
        }

        // Update state
        self.owner_of.write(id, to);
        self.balance_of.write(from, self.balance_of.read(from) - U256::from(1));
        self.balance_of.write(to, self.balance_of.read(to) + U256::from(1));
        self.approval_of.write(id, Address::ZERO);

        // Emit event + return
        log::emit(Transfer::new(from, to, id));
        Ok(true)
    }

    pub fn transfer_ownership(&mut self, new_owner: Address) -> Result<bool, ERC721Error> {
        // Perform safety check 
        let from = msg_sender();
        if from != self.owner.read() { return Err(ERC721Error::OnlyOwner) }; 

        // Update state
        self.owner.write(new_owner);

        // Emit event + return 
        log::emit(OwnershipTransferred::new(from, new_owner));
        Ok(true)
    }

    // -- READ-ONLY FUNCTIONS --------------------------------------------------
    pub fn owner(&self) -> Address {
        self.owner.read()
    }

    pub fn owner_of(&self, id: U256) -> Result<Address, ERC721Error> {
        let owner = self.owner_of.read(id);
        if owner == Address::ZERO {
            return Err(ERC721Error::NotMinted);
        }
        Ok(owner)
    }

    pub fn balance_of(&self, owner: Address) -> Result<U256, ERC721Error> {
        if owner == Address::ZERO {
            return Err(ERC721Error::ZeroAddress);
        }
        Ok(self.balance_of.read(owner))
    }

    pub fn get_approved(&self, id: U256) -> Address {
        self.approval_of.read(id)
    }

    pub fn is_approved_for_all(&self, owner: Address, operator: Address) -> bool {
        self.is_operator.read(owner).read(operator)
    }

    pub fn total_supply(&self) -> U256 {
        self.total_supply.read()
    }
}

