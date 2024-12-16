extern crate alloc;

use crate::error::RaffleDrawError;
use crate::types::{DrawNumber, Number, RaffleConfig, RegistrationContractId};
use alloc::vec::Vec;

#[derive(scale::Encode, scale::Decode, Eq, PartialEq, Clone, Copy, Debug)]
pub enum RaffleRegistrationStatus {
    NotStarted,
    Started,
    RegistrationsOpen,
    RegistrationsClosed,
    SaltGenerated,
    ResultsReceived,
}


/// Message sent by the offchain rollup to the Raffle Registration Contracts
#[derive(scale::Encode, scale::Decode, Debug, Clone)]
pub enum RequestForAction {
    /// update the config, set the registration contract id for this contract and start the workflow
    SetConfigAndStart(RaffleConfig, RegistrationContractId),
    /// open the registrations for the given draw number
    OpenRegistrations(DrawNumber),
    /// close the registrations for the given draw number
    CloseRegistrations(DrawNumber),
    /// generate the salt used by VRF
    GenerateSalt(DrawNumber),
    /// set the results (winning numbers + true or false if we have a winner) for the given draw number
    SetResults(DrawNumber, Vec<Number>, bool),
}

pub trait RaffleRegistrationContract {
    fn do_action(
        &self,
        target_draw_number: Option<DrawNumber>,
        target_status: Option<RaffleRegistrationStatus>,
        action: RequestForAction,
        attest_key: &[u8; 32],
    ) -> Result<(bool, Option<Vec<u8>>), RaffleDrawError>;
}
