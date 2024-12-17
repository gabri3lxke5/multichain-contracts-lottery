extern crate alloc;

use crate::types::{AccountId20, AccountId32, DrawNumber, Hash, Number, RaffleConfig, RegistrationContractId, Salt};
use alloc::vec::Vec;

#[derive(scale::Encode, scale::Decode, Debug)]
pub enum RaffleManagerStatus {
    NotStarted,
    Started,
    RegistrationsOpen,
    RegistrationsClosed,
    WaitingSalt,
    WaitingResult,
    WaitingWinner,
    DrawFinished,
}

/// Message to synchronize the contracts, to request the lotto draw and get the list of winners.
/// message pushed in the queue by this contract and read by the offchain rollup
#[derive(scale::Encode, scale::Decode, Eq, PartialEq, Clone, Debug)]
pub enum LottoManagerRequestMessage {
    /// request to propagate the config to all given contracts
    PropagateConfig(RaffleConfig, Vec<RegistrationContractId>),
    /// request to open the registrations to all given contracts
    OpenRegistrations(DrawNumber, Vec<RegistrationContractId>),
    /// request to close the registrations to all given contracts
    CloseRegistrations(DrawNumber, Vec<RegistrationContractId>),
    /// request to generate a salt by all given contracts
    GenerateSalt(DrawNumber, Vec<RegistrationContractId>),
    /// request to draw the numbers based on the config and the given salt
    DrawNumbers(DrawNumber, RaffleConfig, Salt),
    /// request to check if there is a winner for the given numbers
    CheckWinners(DrawNumber, Vec<Number>),
    /// request to propagate the results to all given contracts
    PropagateResults(
        DrawNumber,
        Vec<Number>,
        bool,
        Vec<RegistrationContractId>,
    ),
}

/// Offchain rollup response
#[derive(scale::Encode, scale::Decode)]
pub enum LottoManagerResponseMessage {
    /// The config is propagated to the given contract ids.
    /// arg2: list of contracts where the config is propagated
    /// Arg2 : Hash of config
    ConfigPropagated(Vec<RegistrationContractId>, Hash),
    /// The registration is open for the given contract ids.
    /// arg1: draw number
    /// arg2: list of contracts where the registration is open
    RegistrationsOpen(DrawNumber, Vec<RegistrationContractId>),
    /// The registration is closed for the given contract ids.
    /// arg1: draw number
    /// arg2: list of contracts where the registration is closed
    RegistrationsClosed(DrawNumber, Vec<RegistrationContractId>),
    /// The salt is generated for the given contract ids.
    /// arg1: draw number
    /// arg2: list of contracts where the salt is generated
    SaltGenerated(DrawNumber, Vec<(RegistrationContractId, Salt)>),
    /// Return the winning numbers
    /// arg1: draw number
    /// arg2: winning numbers
    /// arg3: hash of salt used for vrf
    WinningNumbers(DrawNumber, Vec<Number>, Hash),
    /// Return the list of winners
    /// arg1: draw number
    /// arg2: winners substrate
    /// arg3: winners evm
    /// arg4: hash of winning numbers
    Winners(DrawNumber, Vec<AccountId32>, Vec<AccountId20>, Hash),
    /// The results are propagated to the given contract ids.
    /// arg1: draw number
    /// arg2: list of contracts where the results are propagated
    /// arg3: hash of results
    ResultsPropagated(DrawNumber, Vec<RegistrationContractId>, Hash),
    /// Request to close the registrations
    CloseRegistrations(),
}
