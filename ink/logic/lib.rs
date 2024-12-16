#![cfg_attr(not(feature = "std"), no_std, no_main)]
use ink::prelude::vec::Vec;

pub type RegistrationContractId = u128;
pub type DrawNumber = u32;
pub type Number = u16;
pub type Salt = Vec<u8>;

pub mod config;
pub mod error;
pub mod raffle_manager;
pub mod raffle_registration;

#[cfg(test)]
mod test_contract;
