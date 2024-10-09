#![cfg_attr(not(feature = "std"), no_std, no_main)]

extern crate alloc;
extern crate core;

#[ink::contract(env = pink_extension::PinkEnvironment)]
mod lotto_draw_multichain {
    use alloc::boxed::Box;
    use alloc::vec;
    use alloc::vec::Vec;
    use ink::prelude::string::String;
    use ink::storage::Mapping;
    use lotto_draw_logic::draw::Draw;
    use lotto_draw_logic::error::RaffleDrawError;
    use lotto_draw_logic::evm_contract::EvmContract;
    use lotto_draw_logic::indexer::Indexer;
    use lotto_draw_logic::raffle_manager_contract::{
        LottoManagerRequestMessage, LottoManagerResponseMessage,
    };
    use lotto_draw_logic::raffle_registration_contract::{
        RaffleRegistrationContract, RaffleRegistrationStatus, RequestForAction,
    };
    use lotto_draw_logic::types::*;
    use lotto_draw_logic::wasm_contract::WasmContract;
    use phat_offchain_rollup::clients::ink::Action;
    use pink_extension::chain_extension::signing;
    use pink_extension::{error, info, ResultExt};
    use scale::{Decode, Encode};

    #[ink(storage)]
    pub struct Lotto {
        owner: AccountId,
        /// config for raffle manager contract
        raffle_manager: Option<ContractConfig>,
        /// config for raffle registrations contracts
        raffle_registrations: Mapping<RegistrationContractId, ContractConfig>,
        /// indexer endpoint
        indexer_url: Option<String>,
        /// Key for signing the rollup tx.
        attest_key: [u8; 32],
    }

    #[derive(Encode, Decode, Debug, PartialEq, Eq)]
    #[repr(u8)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub enum ContractError {
        RaffleDrawError(RaffleDrawError),
        BadOrigin,
        ClientNotConfigured,
        InvalidKeyLength,
        InvalidAddressLength,
        NoRequestInQueue,
        FailedToCallRollup,
        // error when verify the numbers
        InvalidContractId,
        CurrentRaffleUnknown,
        UnauthorizedRaffle,
        UnknownDrawNumber,
        UnknownRegistrationStatus,
        MissingRegistrationContract,
        EvmRaffleManagerNotImplemented,
    }

    type Result<T> = core::result::Result<T, ContractError>;

    impl From<phat_offchain_rollup::Error> for ContractError {
        fn from(error: phat_offchain_rollup::Error) -> Self {
            error!("error in the rollup: {:?}", error);
            ContractError::FailedToCallRollup
        }
    }

    impl From<RaffleDrawError> for ContractError {
        fn from(error: RaffleDrawError) -> Self {
            ContractError::RaffleDrawError(error)
        }
    }

    impl Lotto {
        #[ink(constructor)]
        pub fn default() -> Self {
            const NONCE: &[u8] = b"lotto";
            let private_key = signing::derive_sr25519_key(NONCE);
            Self {
                owner: Self::env().caller(),
                attest_key: private_key[..32].try_into().expect("Invalid Key Length"),
                raffle_manager: None,
                raffle_registrations: Mapping::default(),
                indexer_url: None,
            }
        }

        /// Gets the owner of the contract
        #[ink(message)]
        pub fn owner(&self) -> AccountId {
            self.owner
        }

        /// Gets the attestor address used by this rollup
        #[ink(message)]
        pub fn get_attest_address(&self) -> Vec<u8> {
            signing::get_public_key(&self.attest_key, signing::SigType::Sr25519)
        }

        /// Gets the ecdsa address used by this rollup in the meta transaction
        #[ink(message)]
        pub fn get_attest_ecdsa_address(&self) -> Vec<u8> {
            use ink::env::hash;
            let input = signing::get_public_key(&self.attest_key, signing::SigType::Ecdsa);
            let mut output = <hash::Blake2x256 as hash::HashOutput>::Type::default();
            ink::env::hash_bytes::<hash::Blake2x256>(&input, &mut output);
            output.to_vec()
        }

        /// Gets the sender address used by this rollup (in case of meta-transaction)
        #[ink(message)]
        pub fn get_sender_address_raffle_manager(&self) -> Option<Vec<u8>> {
            match self.raffle_manager.as_ref() {
                Some(config) => {
                    let sender_key = match config {
                        ContractConfig::Wasm(c) => c.sender_key.as_ref(),
                        ContractConfig::Evm(c) => c.sender_key.as_ref(),
                    };
                    sender_key.map(|key| signing::get_public_key(key, signing::SigType::Sr25519))
                }
                None => None,
            }
        }

        /// Gets the sender address used by this rollup (in case of meta-transaction)
        #[ink(message)]
        pub fn get_sender_address_raffle_registration(
            &self,
            contract_id: RegistrationContractId,
        ) -> Option<Vec<u8>> {
            match self.raffle_registrations.get(contract_id).as_ref() {
                Some(config) => {
                    let sender_key = match config {
                        ContractConfig::Wasm(c) => c.sender_key.as_ref(),
                        ContractConfig::Evm(c) => c.sender_key.as_ref(),
                    };
                    sender_key.map(|key| signing::get_public_key(key, signing::SigType::Sr25519))
                }
                None => None,
            }
        }

        /// Gets the config of the target consumer contract
        #[ink(message)]
        pub fn get_config_raffle_manager(&self) -> Option<ContractConfig> {
            self.raffle_manager.clone()
        }

        /// Gets the config of the target consumer contract
        #[ink(message)]
        pub fn get_config_raffle_registrations(
            &self,
            contract_id: RegistrationContractId,
        ) -> Option<ContractConfig> {
            self.raffle_registrations.get(contract_id).clone()
        }

        /// Configures the target consumer contract (admin only)
        #[ink(message)]
        pub fn set_config_raffle_manager(&mut self, config: Option<ContractConfig>) -> Result<()> {
            self.ensure_owner()?;
            self.raffle_manager = config;
            Ok(())
        }

        #[ink(message)]
        pub fn set_config_raffle_registrations(
            &mut self,
            contract_id: RegistrationContractId,
            config: Option<ContractConfig>,
        ) -> Result<()> {
            self.ensure_owner()?;
            match config {
                None => {
                    self.raffle_registrations.remove(contract_id);
                }
                Some(c) => {
                    self.raffle_registrations.insert(contract_id, &c);
                }
            }
            Ok(())
        }

        /*
                /// Gets the sender address used by this rollup (in case of meta-transaction)
                #[ink(message)]
                pub fn get_primary_sender_address(&self) -> Option<Vec<u8>> {
                    if let Some(Some(sender_key)) = self
                        .primary_consumer
                        .as_ref()
                        .map(|c| c.sender_key.as_ref())
                    {
                        let sender_key = signing::get_public_key(sender_key, signing::SigType::Sr25519);
                        Some(sender_key)
                    } else {
                        None
                    }
                }

                /// Gets the sender address used by this rollup (in case of meta-transaction)
                #[ink(message)]
                pub fn get_secondary_sender_address(&self, key: u8) -> Option<Vec<u8>> {
                    if let Some(Some(sender_key)) = self.secondary_consumers.get(key).map(|c| c.sender_key)
                    {
                        let sender_key = signing::get_public_key(&sender_key, signing::SigType::Sr25519);
                        Some(sender_key)
                    } else {
                        None
                    }
                }

                 /// Gets the config of the target consumer contract
        #[ink(message)]
        pub fn get_config_raffle_manager(&self) -> Option<(String, u8, u8, WasmContractId)> {
            self.primary_consumer
                .as_ref()
                .map(|c| (c.rpc.clone(), c.pallet_id, c.call_id, c.contract_id))
        }

        /// Gets the config of the target consumer contract
        #[ink(message)]
        pub fn get_secondary_consumer(&self, key: u8) -> Option<(String, EvmContractId)> {
            self.secondary_consumers
                .get(key)
                .map(|c| (c.rpc.clone(), c.contract_id))
        }

        /// Configures the target consumer contract (admin only)
        #[ink(message)]
        pub fn set_primary_consumer(
            &mut self,
            rpc: String,
            pallet_id: u8,
            call_id: u8,
            contract_id: Vec<u8>,
            sender_key: Option<Vec<u8>>,
        ) -> Result<()> {
            self.ensure_owner()?;
            self.primary_consumer = Some(WasmContractConfig {
                rpc,
                pallet_id,
                call_id,
                contract_id: contract_id
                    .try_into()
                    .or(Err(ContractError::InvalidAddressLength))?,
                sender_key: match sender_key {
                    Some(key) => Some(key.try_into().or(Err(ContractError::InvalidKeyLength))?),
                    None => None,
                },
            });
            Ok(())
        }

        #[ink(message)]
        pub fn set_secondary_consumer(
            &mut self,
            key: u8,
            config: Option<EvmContractConfig>,
        ) -> Result<()> {
            self.ensure_owner()?;
            match config {
                None => {
                    if let Some(index) =
                        self.secondary_consumers_keys.iter().position(|k| *k == key)
                    {
                        self.secondary_consumers.remove(key);
                        self.secondary_consumers_keys.remove(index);
                    }
                }
                Some(c) => {
                    self.secondary_consumers.insert(key, &c);
                    if !self
                        .secondary_consumers_keys
                        .iter()
                        .any(|k| *k == key)
                    {
                        self.secondary_consumers_keys.push(key);
                    }
                }
            }
            Ok(())
        }

         */

        /// Gets the config to target the indexer
        #[ink(message)]
        pub fn get_indexer_url(&self) -> Option<String> {
            self.indexer_url.clone()
        }

        /// Configures the indexer (admin only)
        #[ink(message)]
        pub fn config_indexer(&mut self, indexer_url: String) -> Result<()> {
            self.ensure_owner()?;
            self.indexer_url = Some(indexer_url);
            Ok(())
        }

        /// Transfers the ownership of the contract (admin only)
        #[ink(message)]
        pub fn transfer_ownership(&mut self, new_owner: AccountId) -> Result<()> {
            self.ensure_owner()?;
            self.owner = new_owner;
            Ok(())
        }

        /// Processes a request by a rollup transaction
        #[ink(message)]
        pub fn answer_request(&self) -> Result<Option<Vec<u8>>> {
            let config = self.ensure_client_configured()?;
            /*
                        let mut client: Box<dyn RaffleManagerContract> = match config {
                            ContractConfig::Wasm(config) => {
                                WasmContract::new(Some(config.clone())).map(Box::new)?
                            }
                            ContractConfig::Evm(config) => return Err(ContractError::EvmRaffleManagerNotImplemented),
                        };
            */
            let (mut client, sender_key) = match config {
                ContractConfig::Wasm(config) => (WasmContract::connect(config)?, config.sender_key),
                ContractConfig::Evm(_config) => {
                    return Err(ContractError::EvmRaffleManagerNotImplemented)
                }
            };

            // Get a request if presents
            let request: LottoManagerRequestMessage = client
                .pop()
                .log_err("answer_request: failed to read queue")?
                .ok_or(ContractError::NoRequestInQueue)?;

            ink::env::debug_println!("Received request: {request:02x?}");

            let response = self.handle_request(request)?;
            // Attach an action to the tx by:
            client.action(Action::Reply(response.encode()));

            let tx = WasmContract::maybe_submit_tx(client, &self.attest_key, sender_key.as_ref())?;
            Ok(tx)
        }

        fn handle_request(
            &self,
            message: LottoManagerRequestMessage,
        ) -> Result<LottoManagerResponseMessage> {
            let response = match message {
                LottoManagerRequestMessage::PropagateConfig(config, ref contract_ids) => {
                    let synchronized_contracts =
                        self.inner_do_action(RequestForAction::SetConfig(config), contract_ids)?;
                    let hash = [0; 32]; // TODO compute hash
                    LottoManagerResponseMessage::ConfigPropagated(synchronized_contracts, hash)
                }
                LottoManagerRequestMessage::OpenRegistrations(draw_number, ref contract_ids) => {
                    let synchronized_contracts = self.inner_do_action(
                        RequestForAction::OpenRegistrations(draw_number),
                        contract_ids,
                    )?;
                    LottoManagerResponseMessage::RegistrationsOpen(
                        draw_number,
                        synchronized_contracts,
                    )
                }
                LottoManagerRequestMessage::CloseRegistrations(draw_number, ref contract_ids) => {
                    let synchronized_contracts = self.inner_do_action(
                        RequestForAction::CloseRegistrations(draw_number),
                        contract_ids,
                    )?;
                    LottoManagerResponseMessage::RegistrationsClosed(
                        draw_number,
                        synchronized_contracts,
                    )
                }
                LottoManagerRequestMessage::DrawNumbers(
                    draw_number,
                    nb_numbers,
                    smallest_number,
                    biggest_number,
                ) => {
                    let numbers = self.inner_get_numbers(
                        draw_number,
                        nb_numbers,
                        smallest_number,
                        biggest_number,
                    )?;
                    let hash = [0; 32]; // TODO compute hash
                    LottoManagerResponseMessage::WinningNumbers(draw_number, numbers, hash)
                }
                LottoManagerRequestMessage::CheckWinners(draw_number, ref numbers) => {
                    let indexer = Indexer::new(self.get_indexer_url())?;
                    let winners = indexer.query_winners(draw_number, numbers)?;
                    /*.map(
                        |(substrate_addresses, evm_addresses)| {
                            LottoManagerResponseMessage::Winners(substrate_addresses, evm_addresses)
                        },
                    )?
                    */
                    let hash = [0; 32]; // TODO compute hash
                                        // TODO manage evm addresses
                    LottoManagerResponseMessage::Winners(draw_number, winners.0, hash)
                }
                LottoManagerRequestMessage::PropagateResults(
                    draw_number,
                    ref _numbers,
                    ref _winners,
                    ref contract_ids,
                ) => {
                    let synchronized_contracts = self.inner_do_action(
                        RequestForAction::SetResults(draw_number, vec![], vec![]),
                        contract_ids,
                    )?;
                    let hash = [0; 32]; // TODO compute hash
                    LottoManagerResponseMessage::ResultsPropagated(
                        draw_number,
                        synchronized_contracts,
                        hash,
                    )
                }
            };

            Ok(response)
        }

        fn inner_do_action(
            &self,
            request: RequestForAction,
            contract_ids: &[RegistrationContractId],
        ) -> Result<Vec<RegistrationContractId>> {
            let mut synchronized_contracts = Vec::new();

            let (expected_draw_number, expected_status) = match request {
                RequestForAction::SetConfig(_) => (0, RaffleRegistrationStatus::NotStarted),
                RequestForAction::OpenRegistrations(draw_number) => {
                    (draw_number, RaffleRegistrationStatus::RegistrationOpen)
                }
                RequestForAction::CloseRegistrations(draw_number) => {
                    (draw_number, RaffleRegistrationStatus::RegistrationClosed)
                }
                RequestForAction::SetResults(draw_number, _, _) => {
                    (draw_number, RaffleRegistrationStatus::ResultsReceived)
                }
            };

            for contract_id in contract_ids {
                let contract_config = self
                    .raffle_registrations
                    .get(contract_id)
                    .ok_or(ContractError::MissingRegistrationContract)?;
                let contract: Box<dyn RaffleRegistrationContract> = match contract_config {
                    ContractConfig::Wasm(config) => {
                        WasmContract::new(Some(config)).map(Box::new)?
                    }
                    ContractConfig::Evm(config) => EvmContract::new(Some(config)).map(Box::new)?,
                };

                let draw_number = contract
                    .get_draw_number()
                    .ok_or(ContractError::UnknownDrawNumber)?;
                let status = contract
                    .get_status()
                    .ok_or(ContractError::UnknownRegistrationStatus)?;
                if draw_number == expected_draw_number && status == expected_status {
                    // the contract is already synchronized
                    synchronized_contracts.push(*contract_id);
                } else {
                    // synchronize the contract
                    contract.do_action(request.clone(), &self.attest_key)?;
                }
            }
            Ok(synchronized_contracts)
        }

        /// Verify if the winning numbers for a raffle are valid (only for past raffles)
        ///
        #[ink(message)]
        pub fn verify_numbers(
            &self,
            draw_number: DrawNumber,
            hashes: Vec<lotto_draw_logic::types::Hash>,
            nb_numbers: u8,
            smallest_number: Number,
            biggest_number: Number,
            numbers: Vec<Number>,
        ) -> Result<bool> {
            /*
                       let config = self.ensure_client_configured()?;

                       // check if the target contract is correct
                       if contract_id != config.contract_id {
                           return Err(ContractError::InvalidContractId);
                       }

                       let mut client = WasmContract::connect(config)?;
                       const LAST_RAFFLE_FOR_VERIF: u32 = ink::selector_id!("LAST_RAFFLE_FOR_VERIF");

                       let last_raffle: DrawNumber = client
                           .get(&LAST_RAFFLE_FOR_VERIF)
                           .log_err("verify numbers: last raffle unknown")?
                           .ok_or(ContractError::CurrentRaffleUnknown)?;

                       // verify the winning numbers only for the past raffles
                       if draw_number > last_raffle {
                           return Err(ContractError::UnauthorizedRaffle);
                       }
            */

            let draw = Draw::new(nb_numbers, smallest_number, biggest_number)?;
            let result = draw.verify_numbers(draw_number, hashes, numbers)?;
            Ok(result)
        }

        fn inner_get_numbers(
            &self,
            draw_number: DrawNumber,
            nb_numbers: u8,
            smallest_number: Number,
            biggest_number: Number,
        ) -> Result<Vec<Number>> {
            info!(
                "Draw number {draw_number} - Request received for draw {nb_numbers} numbers between {smallest_number} and {biggest_number}"
            );

            let indexer = Indexer::new(self.get_indexer_url())?;
            let hashes = indexer.query_hashes(draw_number)?;

            let draw = Draw::new(nb_numbers, smallest_number, biggest_number)?;
            let result = draw.get_numbers(draw_number, hashes)?;
            Ok(result)
        }

        /*
               fn inner_complete_all_raffles(
                   &self,
                   draw_number: DrawNumber,
               ) -> Result<(bool, Vec<lotto_draw_logic::types::Hash>)> {
                   info!("Synchronize raffle {raffle_id} - complete all raffles");

                   let indexer = Indexer::new(self.get_indexer_url())?;
                   let hashes = indexer.query_hashes(draw_number)?;

                   let expected_nb_hashes = self
                       .secondary_consumers_keys
                       .len()
                       .checked_add(1)
                       .ok_or(AddOverFlow)?;
                   if hashes.len() == expected_nb_hashes {
                       // we already have all hashes, it means all raffles are completed
                       return Ok((true, hashes));
                   }

                   for (_i, key) in self.secondary_consumers_keys.iter().enumerate() {
                       // TODO complete the contract raffle only if we don't have the hash

                       // get the config linked to this contract
                       let config = self.secondary_consumers.get(key);
                       // complete the raffle
                       let contract = EvmContract::new(config)?;
                       contract.complete_raffle(raffle_id, &self.attest_key)?;
                   }

                   // we have to wait
                   Ok((false, hashes))
               }
        */

        /// Returns BadOrigin error if the caller is not the owner
        fn ensure_owner(&self) -> Result<()> {
            if self.env().caller() == self.owner {
                Ok(())
            } else {
                Err(ContractError::BadOrigin)
            }
        }

        /// Returns the config reference or raise the error `ClientNotConfigured`
        fn ensure_client_configured(&self) -> Result<&ContractConfig> {
            self.raffle_manager
                .as_ref()
                .ok_or(ContractError::ClientNotConfigured)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        struct EnvVars {
            /// The RPC endpoint of the target blockchain
            rpc: String,
            pallet_id: u8,
            call_id: u8,
            /// The rollup anchor address on the target blockchain
            contract_id: WasmContractId,
            /// When we want to manually set the attestor key for signing the message (only dev purpose)
            //attest_key: Vec<u8>,
            /// When we want to use meta tx
            sender_key: Option<Vec<u8>>,
        }

        fn get_env(key: &str) -> String {
            std::env::var(key).expect("env not found")
        }

        fn config() -> EnvVars {
            dotenvy::dotenv().ok();
            let rpc = get_env("RPC");
            let pallet_id: u8 = get_env("PALLET_ID").parse().expect("u8 expected");
            let call_id: u8 = get_env("CALL_ID").parse().expect("u8 expected");
            let contract_id: WasmContractId = hex::decode(get_env("CONTRACT_ID"))
                .expect("hex decode failed")
                .try_into()
                .expect("incorrect length");
            //let attest_key = hex::decode(get_env("ATTEST_KEY")).expect("hex decode failed");
            let sender_key = std::env::var("SENDER_KEY")
                .map(|s| hex::decode(s).expect("hex decode failed"))
                .ok();

            EnvVars {
                rpc: rpc.to_string(),
                pallet_id,
                call_id,
                contract_id: contract_id.into(),
                //attest_key,
                sender_key,
            }
        }

        fn init_contract() -> Lotto {
            let EnvVars {
                rpc,
                pallet_id,
                call_id,
                contract_id,
                //attest_key,
                sender_key,
            } = config();

            let mut lotto = Lotto::default();
            lotto
                .set_primary_consumer(rpc, pallet_id, call_id, contract_id.into(), sender_key)
                .unwrap();

            lotto
                .config_indexer("https://query.substrate.fi/lotto-subquery-shibuya".to_string())
                .unwrap();
            //lotto.set_attest_key(Some(attest_key)).unwrap();

            lotto
        }

        #[ink::test]
        #[ignore = "The target contract must be deployed on the Substrate node and a random number request must be submitted"]
        fn answer_request() {
            let _ = env_logger::try_init();
            pink_extension_runtime::mock_ext::mock_all_ext();

            let lotto = init_contract();

            let r = lotto.answer_request().expect("failed to answer request");
            ink::env::debug_println!("answer request: {r:?}");
        }
    }
}
