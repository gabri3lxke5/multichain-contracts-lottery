#![cfg_attr(not(feature = "std"), no_std, no_main)]

#[openbrush::implementation(Ownable, AccessControl, Upgradeable)]
#[openbrush::contract]
pub mod lotto_registration_contract {
    use ink::prelude::vec::Vec;
    use lotto::{
        config, config::*, error::*, raffle_registration::*, DrawNumber, Number,
        RegistrationContractId,
    };
    use openbrush::contracts::access_control::*;
    use openbrush::contracts::ownable::*;
    use openbrush::{modifiers, traits::Storage};
    use phat_rollup_anchor_ink::traits::{
        meta_transaction, meta_transaction::*, rollup_anchor, rollup_anchor::*,
    };

    /// Event emitted when the config is updated
    #[ink(event)]
    pub struct ConfigUpdated {
        config: Config,
    }

    /// Event emitted when the workflow starts
    #[ink(event)]
    pub struct Started {
        #[ink(topic)]
        registration_contract_id: RegistrationContractId,
    }

    /// Event emitted when the registrations are open
    #[ink(event)]
    pub struct RegistrationsOpen {
        #[ink(topic)]
        registration_contract_id: RegistrationContractId,
        #[ink(topic)]
        draw_number: DrawNumber,
    }

    /// Event emitted when the registrations are closed
    #[ink(event)]
    pub struct RegistrationsClosed {
        #[ink(topic)]
        registration_contract_id: RegistrationContractId,
        #[ink(topic)]
        draw_number: DrawNumber,
    }

    /// Event emitted when the salt is generated
    #[ink(event)]
    pub struct SaltGenerated {
        #[ink(topic)]
        registration_contract_id: RegistrationContractId,
        #[ink(topic)]
        draw_number: DrawNumber,
    }

    /// Event emitted when the results are received
    #[ink(event)]
    pub struct ResultsReceived {
        #[ink(topic)]
        registration_contract_id: RegistrationContractId,
        #[ink(topic)]
        draw_number: DrawNumber,
        numbers: Vec<Number>,
        has_winner: bool,
    }

    /// Event emitted when the participation is registered
    #[ink(event)]
    pub struct ParticipationRegistered {
        #[ink(topic)]
        registration_contract_id: RegistrationContractId,
        #[ink(topic)]
        draw_number: DrawNumber,
        #[ink(topic)]
        participant: AccountId,
        numbers: Vec<Number>,
    }

    /// Errors occurred in the contract
    #[derive(Debug, Eq, PartialEq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub enum ContractError {
        AccessControlError(AccessControlError),
        RaffleError(RaffleError),
        RollupAnchorError(RollupAnchorError),
        TransferError,
    }

    /// convertor from AccessControlError to ContractError
    impl From<AccessControlError> for ContractError {
        fn from(error: AccessControlError) -> Self {
            ContractError::AccessControlError(error)
        }
    }

    /// convertor from RaffleError to ContractError
    impl From<RaffleError> for ContractError {
        fn from(error: RaffleError) -> Self {
            ContractError::RaffleError(error)
        }
    }

    /// convertor from RaffleError to ContractError
    impl From<RollupAnchorError> for ContractError {
        fn from(error: RollupAnchorError) -> Self {
            ContractError::RollupAnchorError(error)
        }
    }

    /// convertor from ContractError to RollupAnchorError
    impl From<ContractError> for RollupAnchorError {
        fn from(error: ContractError) -> Self {
            ink::env::debug_println!("Error: {:?}", error);
            RollupAnchorError::UnsupportedAction
        }
    }

    /// Message to request for action
    /// Message sent by the offchain rollup to the Ink! smart contract
    #[derive(scale::Encode, scale::Decode)]
    pub enum RequestForAction {
        /// update the config, set the registration contract id for this contract and start the workflow
        SetConfigAndStart(Config, RegistrationContractId),
        /// open the registrations for the given draw number
        OpenRegistrations(DrawNumber),
        /// close the registrations for the given draw number
        CloseRegistrations(DrawNumber),
        /// generate the salt used by VRF
        GenerateSalt(DrawNumber),
        /// set the results (winning numbers + true or false if we have a winner) for the given draw number
        SetResults(DrawNumber, Vec<Number>, bool),
    }

    // Contract storage
    #[ink(storage)]
    #[derive(Default, Storage)]
    pub struct Contract {
        #[storage_field]
        ownable: ownable::Data,
        #[storage_field]
        access: access_control::Data,
        #[storage_field]
        rollup_anchor: rollup_anchor::Data,
        #[storage_field]
        meta_transaction: meta_transaction::Data,
        #[storage_field]
        config: config::Data,
        registration_contract_id: RegistrationContractId,
    }

    impl RaffleConfig for Contract {}
    impl Raffle for Contract {}

    impl RollupAnchor for Contract {}
    impl MetaTransaction for Contract {}

    impl Contract {
        #[ink(constructor)]
        pub fn new() -> Self {
            let mut instance = Self::default();
            let caller = instance.env().caller();
            // set the owner of this contract
            ownable::Internal::_init_with_owner(&mut instance, caller);
            // set the admin of this contract
            access_control::Internal::_init_with_admin(&mut instance, Some(caller));
            instance
        }

        #[ink(message)]
        pub fn participate(&mut self, numbers: Vec<Number>) -> Result<(), ContractError> {
            // check if the numbers are correct
            RaffleConfig::check_numbers(self, &numbers)?;
            // check if the user can participate (raffle is open)
            Raffle::check_can_participate(self)?;
            // save the participation with an event
            let participant = Self::env().caller();
            let registration_contract_id = self.registration_contract_id;
            let draw_number = Raffle::get_draw_number(self)?;
            self.env().emit_event(ParticipationRegistered {
                registration_contract_id,
                draw_number,
                participant,
                numbers,
            });
            Ok(())
        }

        #[ink(message)]
        pub fn participate_batch(
            &mut self,
            numbers: Vec<Vec<Number>>,
        ) -> Result<(), ContractError> {
            // check if the numbers are correct
            for n in numbers {
                self.participate(n)?;
            }

            Ok(())
        }

        #[ink(message)]
        pub fn get_registration_contract_id(&self) -> RegistrationContractId {
            self.registration_contract_id
        }

        fn inner_set_config_and_start(
            &mut self,
            config: Config,
            registration_contract_id: RegistrationContractId,
        ) -> Result<(), ContractError> {
            // check the status, we can set the config only when the raffle is not started yet
            let status = Raffle::get_status(self)?;
            if status != Status::NotStarted {
                return Err(ContractError::RaffleError(RaffleError::IncorrectStatus));
            }
            // set the registration contract id
            self.registration_contract_id = registration_contract_id;

            // update the config
            RaffleConfig::set_config(self, config)?;

            // emit the event
            self.env().emit_event(ConfigUpdated {
                config,
            });

            // start the workflow
            Raffle::start(self)?;

            // emit the event
            self.env().emit_event(Started {
                registration_contract_id,
            });

            Ok(())
        }

        fn inner_open_registrations(
            &mut self,
            draw_number: DrawNumber,
        ) -> Result<(), ContractError> {
            // Open the registrations
            Raffle::open_registrations(self, draw_number)?;

            // emit the event
            let registration_contract_id = self.registration_contract_id;
            self.env().emit_event(RegistrationsOpen {
                registration_contract_id,
                draw_number,
            });

            Ok(())
        }

        fn inner_close_registrations(
            &mut self,
            draw_number: DrawNumber,
        ) -> Result<(), ContractError> {
            // Close the registrations
            Raffle::close_registrations(self, draw_number)?;

            // emit the event
            let registration_contract_id = self.registration_contract_id;
            self.env().emit_event(RegistrationsClosed {
                registration_contract_id,
                draw_number,
            });

            Ok(())
        }

        fn inner_generate_salt(
            &mut self,
            draw_number: DrawNumber,
        ) -> Result<(), ContractError> {
            // Generate the salt
            Raffle::generate_salt(self, draw_number)?;

            // emit the event
            let registration_contract_id = self.registration_contract_id;
            self.env().emit_event(SaltGenerated {
                registration_contract_id,
                draw_number,
            });

            Ok(())
        }

        fn inner_set_results(
            &mut self,
            draw_number: DrawNumber,
            numbers: Vec<Number>,
            has_winner: bool,
        ) -> Result<(), ContractError> {
            // check if the numbers satisfies the config
            RaffleConfig::check_numbers(self, &numbers)?;

            // save the results
            Raffle::save_results(self, draw_number, numbers.clone(), has_winner)?;

            // emmit the event
            let registration_contract_id = self.registration_contract_id;
            self.env().emit_event(ResultsReceived {
                registration_contract_id,
                draw_number,
                numbers: numbers.clone(),
                has_winner,
            });

            Ok(())
        }

        #[ink(message)]
        #[modifiers(only_role(DEFAULT_ADMIN_ROLE))]
        pub fn register_attestor(
            &mut self,
            account_id: AccountId,
        ) -> Result<(), AccessControlError> {
            AccessControl::grant_role(self, ATTESTOR_ROLE, Some(account_id))?;
            Ok(())
        }

        #[ink(message)]
        pub fn get_attestor_role(&self) -> RoleType {
            ATTESTOR_ROLE
        }

        #[ink(message)]
        #[modifiers(only_role(DEFAULT_ADMIN_ROLE))]
        pub fn terminate_me(&mut self) -> Result<(), ContractError> {
            self.env().terminate_contract(self.env().caller());
        }

        #[ink(message)]
        #[openbrush::modifiers(only_role(DEFAULT_ADMIN_ROLE))]
        pub fn withdraw(&mut self, value: Balance) -> Result<(), ContractError> {
            let caller = Self::env().caller();
            self.env()
                .transfer(caller, value)
                .map_err(|_| ContractError::TransferError)?;
            Ok(())
        }
    }

    impl rollup_anchor::MessageHandler for Contract {
        fn on_message_received(&mut self, action: Vec<u8>) -> Result<(), RollupAnchorError> {
            // parse the response
            let request: RequestForAction = scale::Decode::decode(&mut &action[..])
                .or(Err(RollupAnchorError::FailedToDecode))?;

            match request {
                RequestForAction::SetConfigAndStart(config, registration_contract_id) => {
                    self.inner_set_config_and_start(config, registration_contract_id)?;
                }
                RequestForAction::OpenRegistrations(draw_number) => {
                    self.inner_open_registrations(draw_number)?;
                }
                RequestForAction::CloseRegistrations(draw_number) => {
                    self.inner_close_registrations(draw_number)?;
                }
                RequestForAction::GenerateSalt(draw_number) => {
                    self.inner_generate_salt(draw_number)?;
                }
                RequestForAction::SetResults(draw_number, numbers, has_winner) => {
                    self.inner_set_results(draw_number, numbers, has_winner)?
                }
            }

            Ok(())
        }
    }

    impl rollup_anchor::EventBroadcaster for Contract {
        fn emit_event_message_queued(&self, _id: u32, _data: Vec<u8>) {
            // nothing because the message queue is not used in this contract
        }
        fn emit_event_message_processed_to(&self, _id: u32) {
            // nothing because an event is already emitted in the different methods
        }
    }

    impl meta_transaction::EventBroadcaster for Contract {
        fn emit_event_meta_tx_decoded(&self) {
            // do nothing, we don't care
        }
    }
}
