#![cfg_attr(not(feature = "std"), no_std, no_main)]

pub type RegistrationContractId = u128;
pub type DrawNumber = u32;
pub type Number = u16;
pub type Salt = ink::prelude::vec::Vec<u8>;
pub type AccountId32 = [u8; 32];
pub type AccountId20 = [u8; 20];

pub mod config;
pub mod error;
pub mod raffle_manager;
pub mod raffle_registration;

#[cfg(test)]
mod test_contract;
