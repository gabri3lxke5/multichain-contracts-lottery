use crate::error::{RaffleError, RaffleError::*};
use crate::{AccountId20, AccountId32, DrawNumber, Number, RegistrationContractId, Salt};
use ink::prelude::vec::Vec;
use ink::storage::Mapping;
use openbrush::traits::Storage;
use phat_rollup_anchor_ink::traits::rollup_anchor::RollupAnchor;
use scale::{Decode, Encode};

const STATUS: u32 = ink::selector_id!("STATUS");
const DRAW_NUMBER: u32 = ink::selector_id!("DRAW_NUMBER");

pub type Winners = (Vec<AccountId32>, Vec<AccountId20>);

#[derive(Default, Debug)]
#[openbrush::storage_item]
pub struct Data {
    registration_contracts: Vec<RegistrationContractId>,
    registration_contracts_status: Mapping<RegistrationContractId, Status>,
    salts: Mapping<DrawNumber, Vec<(RegistrationContractId, Salt)>>,
    generated_salt: Mapping<DrawNumber, Salt>,
    results: Mapping<DrawNumber, Vec<Number>>,
    winners: Mapping<DrawNumber, Winners>,
    min_number_salts: u8,
}

#[derive(Default, Debug, Eq, PartialEq, Copy, Clone, scale::Encode, scale::Decode)]
#[cfg_attr(
    feature = "std",
    derive(scale_info::TypeInfo, ink::storage::traits::StorageLayout)
)]
pub enum Status {
    #[default]
    NotStarted,
    Started,
    RegistrationsOpen,
    RegistrationsClosed,
    WaitingSalt,
    WaitingResult,
    WaitingWinner,
    DrawFinished,
}

#[openbrush::trait_definition]
pub trait RaffleManager: Storage<Data> + RollupAnchor {

    /// Set the registration contracts
    fn set_registration_contracts(
        &mut self,
        registration_contracts: Vec<RegistrationContractId>,
    ) -> Result<(), RaffleError> {
        // check the status
        self.check_registration_contracts_status(Status::NotStarted)?;

        // update the new contract
        self.data::<Data>().registration_contracts = registration_contracts.clone();
        // add the default status for this added contract
        for registration_contract in &registration_contracts {
            self.data::<Data>()
                .registration_contracts_status
                .insert(registration_contract, &Status::NotStarted);
        }

        Ok(())
    }

    #[ink(message)]
    fn get_min_number_salts(&self) -> u8 {
        self.data::<Data>().min_number_salts
    }

    /// Set the minimum number of salts
    fn set_min_number_salts(
        &mut self,
        min_number_salts: u8,
    ) -> Result<(), RaffleError> {
        // check the status
        self.check_registration_contracts_status(Status::NotStarted)?;

        // update the storage
        self.data::<Data>().min_number_salts = min_number_salts;

        Ok(())
    }

    /// start
    fn start(&mut self, previous_draw_number: DrawNumber) -> Result<(), RaffleError> {
        // check the status
        self.check_registration_contracts_status(Status::NotStarted)?;

        self.set_draw_number(previous_draw_number);
        self.set_status(Status::Started);

        Ok(())
    }

    /// Open the registrations
    fn open_registrations(&mut self) -> Result<DrawNumber, RaffleError> {
        // check the status
        let status = self.get_status()?;
        if status != Status::Started && status != Status::DrawFinished {
            return Err(IncorrectStatus);
        }
        // check the status
        self.check_registration_contracts_status(status)?;

        // increment the draw number
        let new_draw_number = self.get_draw_number()?
            .checked_add(1)
            .ok_or(AddOverFlow)?;
    
        self.set_draw_number(new_draw_number);
        self.set_status(Status::RegistrationsOpen);

        Ok(new_draw_number)
    }

    /// Return true if the registrations can be closed
    fn can_close_registrations(&self) -> bool {
        self.check_registration_contracts_status(Status::RegistrationsOpen)
            .is_ok()
    }

    /// Close the registrations
    fn close_registrations(&mut self) -> Result<DrawNumber, RaffleError> {
        // check the status
        self.check_registration_contracts_status(Status::RegistrationsOpen)?;

        // update the status
        self.set_status(Status::RegistrationsClosed);
        self.get_draw_number()
    }

    /// Try to generate the salt
    /// Return the salt or the list of missing contracts
    fn try_to_generate_salt(&mut self) -> Result<(Option<Salt>, Vec<RegistrationContractId>), RaffleError> {
        // check and update the status
        match self.get_status()? {
            Status::RegistrationsClosed => self.set_status(Status::WaitingSalt),
            Status::WaitingSalt => {},
            _ => return Err(IncorrectStatus),
        };

        let draw_number = self.get_draw_number()?;

        // manage the case when we don't want salts generated by registration contracts
        if self.data::<Data>().min_number_salts == 0 {
            // default salt used for test purpose
            let salts = Vec::new();
            //let default_salt = [0u8; 32];
            //salts.push(&default_salt.to_vec());
            // generate the salt
            let generated_salt = self.generate_salt(draw_number, salts.as_slice())?;
            // update the status
            self.set_status(Status::WaitingResult);
            return Ok((Some(generated_salt), Vec::new()));
        }

        // get the salts generated by registration contracts
        let contracts_salts = self.data::<Data>().salts.get(draw_number).unwrap_or_default();

        // test if we received enough salts
        let min_number_salts = self.data::<Data>().min_number_salts as usize;
        if contracts_salts.len() >= min_number_salts  {
            // we already receive enough salt to generate the final salt
            // collect all salt
            let salts : Vec<_> = contracts_salts.iter().map(|(_contract, salt)| salt).collect();
            // generate the salt
            let generated_salt = self.generate_salt(draw_number, &salts[..min_number_salts])?;
            // update the status
            self.set_status(Status::WaitingResult);
            return Ok((Some(generated_salt), Vec::new()));
        }
        // we didn't receive enough salt
        let mut missing_contracts = Vec::new();

        for i in 0..self.data::<Data>().registration_contracts.len() {
            let contract_id = self.data::<Data>().registration_contracts[i];
            let contract_status = self
                .data::<Data>()
                .registration_contracts_status
                .get(contract_id);
            if contract_status != Some(Status::WaitingSalt) {
                missing_contracts.push(contract_id);
            }
        }
        Ok((None, missing_contracts))
    }

    fn generate_salt(
        &mut self,
        draw_number: DrawNumber,
        salts: &[&Salt]
    ) -> Result<Salt, RaffleError> {

        use ink::env::hash;

        let mut input_salts: Vec<u8> = Vec::new();
        for salt in salts.iter() {
            input_salts.extend_from_slice(salt);
        }

        let mut output_salt = <hash::Blake2x256 as hash::HashOutput>::Type::default();
        ink::env::hash_bytes::<hash::Blake2x256>(&input_salts, &mut output_salt);

        match self.data::<Data>().generated_salt.get(draw_number) {
            Some(_) => return Err(ExistingSalt),
            None => self.data::<Data>().generated_salt.insert(draw_number, &output_salt.to_vec()),
        };

        Ok(output_salt.to_vec())
    }

    /// Save the salts for given registration contracts
    fn save_salts(
        &mut self,
        draw_number: DrawNumber,
        contracts_salts: Vec<(RegistrationContractId, Salt)>,
    ) -> Result<(), RaffleError> {
        // check the status
        if self.get_status()? != Status::WaitingSalt {
            return Err(IncorrectStatus);
        }
        // check the draw number
        if self.get_draw_number()? != draw_number {
            return Err(IncorrectDrawNumber);
        }

        for (contract_id, salt) in contracts_salts.iter() {
            match self.data::<Data>().registration_contracts_status.get(contract_id) {
                Some(Status::RegistrationsClosed) => {
                    // update the status
                    self.data::<Data>().registration_contracts_status.insert(contract_id, &Status::WaitingSalt);
                    // add the hash
                    let mut registered_contracts_salts = self.data::<Data>().salts.get(draw_number).unwrap_or_default();
                    registered_contracts_salts.push((*contract_id, salt.to_vec()));
                    self.data::<Data>().salts.insert(draw_number, &registered_contracts_salts);
                },
                _ => return Err(IncorrectStatus)
            }
        }

        Ok(())
    }

    /// Save the status for given registration contracts
    /// return the contracts not synchronized yet
    fn check_registration_contracts_status(&self, status: Status) -> Result<(), RaffleError> {
        // check the status in the manager
        if self.get_status()? != status {
            return Err(IncorrectStatus);
        }

        // check the status in all  registration contracts
        for i in 0..self.data::<Data>().registration_contracts.len() {
            let contract_id = self.data::<Data>().registration_contracts[i];
            let contract_status = self
                .data::<Data>()
                .registration_contracts_status
                .get(contract_id);
            if contract_status != Some(status) {
                return Err(IncorrectStatus);
            }
        }

        Ok(())
    }

    /// Save the status for given registration contracts
    /// return the contracts not synchronized yet
    fn save_registration_contracts_status(
        &mut self,
        draw_number: DrawNumber,
        status: Status,
        registration_contracts: Vec<RegistrationContractId>,
    ) -> Result<Vec<RegistrationContractId>, RaffleError> {
        // check the status
        if self.get_status()? != status {
            return Err(IncorrectStatus);
        }
        // check the draw number
        if self.get_draw_number()? != draw_number {
            return Err(IncorrectDrawNumber);
        }

        for registration_contract in &registration_contracts {
            self.data::<Data>()
                .registration_contracts_status
                .insert(registration_contract, &status);
        }

        // contract not synchronized yet
        let mut not_synchronized_contracts = Vec::new();

        for i in 0..self.data::<Data>().registration_contracts.len() {
            let contract_id = self.data::<Data>().registration_contracts[i];
            let contract_status = self
                .data::<Data>()
                .registration_contracts_status
                .get(contract_id);
            if contract_status != Some(status) {
                not_synchronized_contracts.push(contract_id);
            }
        }

        Ok(not_synchronized_contracts)
    }

    #[ink(message)]
    fn get_draw_number(&self) -> Result<DrawNumber, RaffleError> {
        match RollupAnchor::get_value(self, DRAW_NUMBER.encode()) {
            Some(v) => DrawNumber::decode(&mut v.as_slice()).map_err(|_| FailedToDecode),
            _ => Ok(0),
        }
    }

    fn set_draw_number(&mut self, draw_number: DrawNumber) {
        RollupAnchor::set_value(self, &DRAW_NUMBER.encode(), Some(&draw_number.encode()));
    }

    #[ink(message)]
    fn get_status(&self) -> Result<Status, RaffleError> {
        match RollupAnchor::get_value(self, STATUS.encode()) {
            Some(v) => Status::decode(&mut v.as_slice()).map_err(|_| FailedToDecode),
            _ => Ok(Status::NotStarted),
        }
    }

    fn set_status(&mut self, status: Status) {
        RollupAnchor::set_value(self, &STATUS.encode(), Some(&status.encode()));
    }

    #[ink(message)]
    fn get_registration_contracts(&self) -> Vec<RegistrationContractId> {
        self.data::<Data>().registration_contracts.clone()
    }

    #[ink(message)]
    fn get_registration_contract_status(
        &self,
        registration_contract: RegistrationContractId,
    ) -> Option<Status> {
        self.data::<Data>()
            .registration_contracts_status
            .get(registration_contract)
    }

    #[ink(message)]
    fn get_generated_salt(&self, draw_number: DrawNumber) -> Option<Salt> {
        self.data::<Data>().generated_salt.get(draw_number)
    }

    #[ink(message)]
    fn get_results(&self, draw_number: DrawNumber) -> Option<Vec<Number>> {
        self.data::<Data>().results.get(draw_number)
    }

    #[ink(message)]
    fn get_winners(&self, draw_number: DrawNumber) -> Option<Winners> {
        self.data::<Data>().winners.get(draw_number)
    }

    /// save the results for the current raffle.
    fn set_results(
        &mut self,
        draw_number: DrawNumber,
        results: Vec<Number>,
    ) -> Result<(), RaffleError> {
        // check the raffle number
        if self.get_draw_number()? != draw_number {
            return Err(IncorrectDrawNumber);
        }

        // check the status
        let status = self.get_status()?;
        //if status != Status::RegistrationsClosed && status != Status::WaitingSalt {
        if status != Status::WaitingResult {
            return Err(IncorrectStatus);
        }

        match self.data::<Data>().results.get(draw_number) {
            Some(_) => Err(ExistingResults),
            None => {
                // save the results
                self.data::<Data>().results.insert(draw_number, &results);
                // update the status
                self.set_status(Status::WaitingWinner);
                Ok(())
            }
        }
    }

    /// check if the saved results are the same as the ones given in parameter
    fn ensure_same_results(
        &mut self,
        draw_number: DrawNumber,
        numbers: &[Number],
    ) -> Result<(), RaffleError> {
        // get the correct results for the given raffle
        let result = self
            .data::<Data>()
            .results
            .get(draw_number)
            .ok_or(DifferentResults)?;

        if result.len() != numbers.len() {
            return Err(DifferentResults);
        }

        for i in 0..numbers.len() {
            if numbers[i] != result[i] {
                return Err(DifferentResults);
            }
        }

        Ok(())
    }

    /// save the winners for the current raffle.
    fn set_winners(
        &mut self,
        draw_number: DrawNumber,
        winners: Winners,
    ) -> Result<(), RaffleError> {
        // check the raffle number
        if self.get_draw_number()? != draw_number {
            return Err(IncorrectDrawNumber);
        }

        // check the status
        if self.get_status()? != Status::WaitingWinner {
            return Err(IncorrectStatus);
        }

        match self.data::<Data>().winners.get(draw_number) {
            Some(_) => Err(ExistingWinners),
            None => {
                // save the result
                if !winners.0.is_empty() || !winners.1.is_empty() {
                    self.data::<Data>().winners.insert(draw_number, &winners);
                }
                // update the status
                self.set_status(Status::DrawFinished);
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_contract::lotto_contract::Contract;

    #[ink::test]
    fn test_add_registrations_contract() {
        let mut contract = Contract::new();

        contract
            .set_registration_contracts(vec![100, 101, 102])
            .expect("Fail to add registrations contract");

        assert_eq!(contract.get_status(), Ok(Status::NotStarted));
        assert_eq!(contract.get_draw_number(), Ok(0));
        assert_eq!(contract.get_registration_contracts(), vec![100, 101, 102]);
        assert_eq!(
            contract.get_registration_contract_status(100),
            Some(Status::NotStarted)
        );
        assert_eq!(
            contract.get_registration_contract_status(101),
            Some(Status::NotStarted)
        );
        assert_eq!(
            contract.get_registration_contract_status(102),
            Some(Status::NotStarted)
        );
        assert_eq!(contract.get_registration_contract_status(103), None);
    }

    #[ink::test]
    fn test_start() {
        let mut contract = Contract::new();

        assert_eq!(contract.get_status(), Ok(Status::NotStarted));
        assert_eq!(contract.get_draw_number(), Ok(0));

        contract.start(1).expect("Fail to start");

        assert_eq!(contract.get_status(), Ok(Status::Started));
        assert_eq!(contract.get_draw_number(), Ok(1));

        // we cannot add registration contract when it started
        assert_eq!(contract.set_registration_contracts(vec![1]), Err(IncorrectStatus));
    }

    #[ink::test]
    fn test_open_registrations() {
        let mut contract = Contract::new();

        assert_eq!(contract.open_registrations(), Err(IncorrectStatus));

        contract.start(0).expect("Fail to start");

        contract
            .open_registrations()
            .expect("Fail to open the registrations");
        assert_eq!(contract.get_status(), Ok(Status::RegistrationsOpen));
        assert_eq!(contract.get_draw_number(), Ok(1));

        assert_eq!(contract.open_registrations(), Err(IncorrectStatus));
    }

    #[ink::test]
    fn test_close_registrations() {
        let mut contract = Contract::new();

        assert_eq!(false, contract.can_close_registrations());
        assert_eq!(contract.close_registrations(), Err(IncorrectStatus));

        contract.start(0).expect("Fail to start");

        assert_eq!(false, contract.can_close_registrations());
        assert_eq!(contract.close_registrations(), Err(IncorrectStatus));

        contract
            .open_registrations()
            .expect("Fail to open the registrations");

        assert_eq!(true, contract.can_close_registrations());
        contract
            .close_registrations()
            .expect("Fail to close the registrations");
        assert_eq!(contract.get_status(), Ok(Status::RegistrationsClosed));
        assert_eq!(contract.get_draw_number(), Ok(1));
    }


    #[ink::test]
    fn test_try_to_generate_salt() {
        let mut contract = Contract::new();

        contract.start(0).expect("Fail to start");
        contract
            .open_registrations()
            .expect("Fail to open the registrations");

        assert_eq!(contract.try_to_generate_salt(), Err(IncorrectStatus));

        contract
            .close_registrations()
            .expect("Fail to close the registrations");

        assert_eq!(contract.get_status(), Ok(Status::RegistrationsClosed));
        assert_eq!(contract.get_draw_number(), Ok(1));

        // test purpose
        let contract_salt = contract
            .try_to_generate_salt()
            .expect("Fail to generate salt");
        assert!(contract_salt.0.is_some(), "Salt not generated");
        assert_eq!(contract_salt.1, vec![]);

        assert_eq!(contract.get_status(), Ok(Status::WaitingResult));
        assert_eq!(contract.get_draw_number(), Ok(1));

    }


    #[ink::test]
    fn test_generate_salt() {
        let mut contract = Contract::new();

        let salt_1 : Salt = [1u8; 32].to_vec();
        let salt_2 : Salt = [2u8; 32].to_vec();
        let salt_3 : Salt = [3u8; 32].to_vec();
        let salts = [&salt_1, &salt_2, &salt_3];

        let generated_salt = contract.generate_salt(1, &salts)
            .expect("Fail to generate salt");

        let expected_salt : Salt = [94, 193, 212, 179, 22, 80, 18, 236, 194, 56, 99, 20, 16, 125, 123, 20, 14, 26, 212, 42, 96, 187, 51, 110, 129, 113, 120, 162, 223, 50, 36, 79].to_vec();
        assert_eq!(expected_salt, generated_salt);

    }

    #[ink::test]
    fn test_set_results() {
        let mut contract = Contract::new();

        contract.start(0).expect("Fail to start");
        contract
            .open_registrations()
            .expect("Fail to open the registrations");

        assert_eq!(contract.set_results(1, vec![]), Err(IncorrectStatus));

        contract
            .close_registrations()
            .expect("Fail to close the registrations");

        assert_eq!(contract.set_results(0, vec![]), Err(IncorrectDrawNumber));
        assert_eq!(contract.set_results(2, vec![]), Err(IncorrectDrawNumber));

        assert_eq!(contract.get_status(), Ok(Status::RegistrationsClosed));
        assert_eq!(contract.get_draw_number(), Ok(1));
        contract
            .try_to_generate_salt()
            .expect("Fail to generate salt");
        contract
            .set_results(1, vec![1, 2, 3, 4])
            .expect("Fail to save the results");
        assert_eq!(contract.get_status(), Ok(Status::WaitingWinner));
        assert_eq!(contract.get_draw_number(), Ok(1));

        assert_eq!(contract.get_results(1), Some(vec![1, 2, 3, 4]));
        assert_eq!(contract.get_results(0), None);
        assert_eq!(contract.get_results(2), None);
    }

    #[ink::test]
    fn test_ensure_same_results() {
        let mut contract = Contract::new();

        contract.start(0).expect("Fail to start");
        contract
            .open_registrations()
            .expect("Fail to open the registrations");
        contract
            .close_registrations()
            .expect("Fail to close the registrations");
        contract
            .try_to_generate_salt()
            .expect("Fail to generate the salt");
        contract
            .set_results(1, vec![1, 2, 3, 4])
            .expect("Fail to save the results");

        contract
            .ensure_same_results(1, vec![1, 2, 3, 4].as_slice())
            .expect("Check same results failed");
        assert_eq!(
            contract.ensure_same_results(1, vec![1, 2, 3, 5].as_slice()),
            Err(DifferentResults)
        );
        assert_eq!(
            contract.ensure_same_results(2, vec![1, 2, 3, 4].as_slice()),
            Err(DifferentResults)
        );
    }

    #[ink::test]
    fn test_set_winners() {
        let mut contract = Contract::new();

        contract.start(0).expect("Fail to start");
        contract
            .open_registrations()
            .expect("Fail to open the registrations");

        assert_eq!(contract.set_winners(1, (vec![], vec![])), Err(IncorrectStatus));

        contract
            .close_registrations()
            .expect("Fail to close the registrations");

        contract
            .try_to_generate_salt()
            .expect("Fail to generate salt");

        assert_eq!(contract.set_winners(1, (vec![], vec![])), Err(IncorrectStatus));

        contract
            .set_results(1, vec![1, 2, 3, 4])
            .expect("Fail to save the results");

        assert_eq!(contract.set_winners(0, (vec![], vec![])), Err(IncorrectDrawNumber));
        assert_eq!(contract.set_winners(2, (vec![], vec![])), Err(IncorrectDrawNumber));

        assert_eq!(contract.get_status(), Ok(Status::WaitingWinner));
        assert_eq!(contract.get_draw_number(), Ok(1));
        contract
            .set_winners(1, (vec![], vec![]))
            .expect("Fail to save the winners");

        assert_eq!(contract.get_status(), Ok(Status::DrawFinished));
        assert_eq!(contract.get_draw_number(), Ok(1));

        assert_eq!(contract.get_winners(1), None);
        assert_eq!(contract.get_results(0), None);
        assert_eq!(contract.get_results(2), None);
    }


    #[ink::test]
    fn test_set_winners_substrate() {
        let mut contract = Contract::new();

        contract.start(0).expect("Fail to start");
        contract
            .open_registrations()
            .expect("Fail to open the registrations");
        contract
            .close_registrations()
            .expect("Fail to close the registrations");
        contract
            .try_to_generate_salt()
            .expect("Fail to generate salt");
        contract
            .set_results(1, vec![1, 2, 3, 4])
            .expect("Fail to save the results");

        let address_substrate_1 = [1;32];
        let address_substrate_2 = [2;32];

        contract
            .set_winners(1, (vec![address_substrate_1, address_substrate_2], vec![]))
            .expect("Fail to save the winners");

        assert_eq!(contract.get_status(), Ok(Status::DrawFinished));
        assert_eq!(contract.get_draw_number(), Ok(1));
        assert_eq!(contract.get_winners(1), Some((vec![address_substrate_1, address_substrate_2], vec![])));
    }


    #[ink::test]
    fn test_set_winners_evm() {
        let mut contract = Contract::new();

        contract.start(0).expect("Fail to start");
        contract
            .open_registrations()
            .expect("Fail to open the registrations");
        contract
            .close_registrations()
            .expect("Fail to close the registrations");
        contract
            .try_to_generate_salt()
            .expect("Fail to generate salt");
        contract
            .set_results(1, vec![1, 2, 3, 4])
            .expect("Fail to save the results");

        let address_evm_1 = [1;20];

        contract
            .set_winners(1, (vec![], vec![address_evm_1]))
            .expect("Fail to save the winners");

        assert_eq!(contract.get_status(), Ok(Status::DrawFinished));
        assert_eq!(contract.get_draw_number(), Ok(1));
        assert_eq!(contract.get_winners(1), Some((vec![], vec![address_evm_1])));
    }


    #[ink::test]
    fn test_reopen_after_results_and_winners() {
        let mut contract = Contract::new();
        contract.start(0).expect("Fail to start");
        contract
            .open_registrations()
            .expect("Fail to open the registrations");
        contract
            .close_registrations()
            .expect("Fail to close the registrations");
        contract
            .try_to_generate_salt()
            .expect("Fail to generate salt");
        contract
            .set_results(1, vec![1, 2, 3, 4])
            .expect("Fail to save the results");
        contract
            .set_winners(1, (vec![], vec![]))
            .expect("Fail to save the winners");

        contract
            .open_registrations()
            .expect("Fail to open the registrations");
        assert_eq!(contract.get_status(), Ok(Status::RegistrationsOpen));
        assert_eq!(contract.get_draw_number(), Ok(2));
    }

    #[ink::test]
    fn test_registration_contracts_status() {
        let mut contract = Contract::new();

        contract
            .set_registration_contracts(vec![100, 101, 102])
            .expect("Fail to add registrations contract");

        contract
            .check_registration_contracts_status(Status::NotStarted)
            .expect("Check status failed");

        // start
        contract.start(0).expect("Fail to start");

        assert_eq!(contract.get_status(), Ok(Status::Started));
        assert_eq!(
            contract.get_registration_contract_status(100),
            Some(Status::NotStarted)
        );
        assert_eq!(
            contract.get_registration_contract_status(101),
            Some(Status::NotStarted)
        );
        assert_eq!(
            contract.get_registration_contract_status(102),
            Some(Status::NotStarted)
        );

        assert_eq!(
            contract.check_registration_contracts_status(Status::NotStarted),
            Err(IncorrectStatus)
        );
        assert_eq!(
            contract.check_registration_contracts_status(Status::Started),
            Err(IncorrectStatus)
        );

        assert_eq!(contract.get_draw_number(), Ok(0));
        // cannot save the draw number doesn't match
        assert_eq!(
            contract.save_registration_contracts_status(1, Status::Started, vec![100, 102]),
            Err(IncorrectDrawNumber)
        );

        contract
            .save_registration_contracts_status(0, Status::Started, vec![100, 102])
            .expect("Save status failed");
        assert_eq!(contract.get_status(), Ok(Status::Started));
        assert_eq!(
            contract.get_registration_contract_status(100),
            Some(Status::Started)
        );
        assert_eq!(
            contract.get_registration_contract_status(101),
            Some(Status::NotStarted)
        );
        assert_eq!(
            contract.get_registration_contract_status(102),
            Some(Status::Started)
        );

        assert_eq!(
            contract.check_registration_contracts_status(Status::NotStarted),
            Err(IncorrectStatus)
        );
        assert_eq!(
            contract.check_registration_contracts_status(Status::Started),
            Err(IncorrectStatus)
        );

        contract
            .save_registration_contracts_status(0, Status::Started, vec![101])
            .expect("Save status failed");

        contract
            .check_registration_contracts_status(Status::Started)
            .expect("Check status failed");

        // open the registrations
        contract
            .open_registrations()
            .expect("Fail to open the Registrations");
        assert_eq!(
            contract.check_registration_contracts_status(Status::RegistrationsOpen),
            Err(IncorrectStatus)
        );

        contract
            .save_registration_contracts_status(1, Status::RegistrationsOpen, vec![100, 101, 102])
            .expect("Save status failed");
        contract
            .check_registration_contracts_status(Status::RegistrationsOpen)
            .expect("Check status failed");

        // cannot save an incorrect
        assert_eq!(
            contract.save_registration_contracts_status(
                1,
                Status::RegistrationsClosed,
                vec![100, 101, 102]
            ),
            Err(IncorrectStatus)
        );

        // close the registrations
        contract
            .close_registrations()
            .expect("Fail to close the Registrations");
        assert_eq!(
            contract.check_registration_contracts_status(Status::RegistrationsClosed),
            Err(IncorrectStatus)
        );

        contract
            .save_registration_contracts_status(1, Status::RegistrationsClosed, vec![100, 101, 102])
            .expect("Save status failed");
        contract
            .check_registration_contracts_status(Status::RegistrationsClosed)
            .expect("Check status failed");
    }

    #[ink::test]
    fn test_full() {
        let mut contract = Contract::new();

        contract
            .set_registration_contracts(vec![100, 101, 102])
            .expect("Fail to add registrations contract");
        contract
            .set_min_number_salts(2)
            .expect("Fail to set the minimum number of salts");

        // start
        contract.start(0).expect("Fail to start");
        contract
            .save_registration_contracts_status(0, Status::Started, vec![100, 101, 102])
            .expect("Save status failed");
        // first draw
        contract
            .open_registrations()
            .expect("Fail to open the Registrations");
        contract
            .save_registration_contracts_status(1, Status::RegistrationsOpen, vec![100, 101, 102])
            .expect("Save status failed");
        contract
            .close_registrations()
            .expect("Fail to open the Registrations");
        contract
            .save_registration_contracts_status(1, Status::RegistrationsClosed, vec![100, 101, 102])
            .expect("Save status failed");
        let salt = contract
            .try_to_generate_salt()
            .expect("Fail to generate salt").0;
        assert_eq!(salt, None);
        contract.save_salts(1, vec![(100, [0u8;32].to_vec()), (101, [1u8;32].to_vec()), (102, [2u8;32].to_vec())])
            .expect("Fail to save the salts");
        let salt = contract
            .try_to_generate_salt()
            .expect("Fail to generate salt").0;
        assert!(salt.is_some(), "Salt is not generated");
        contract
            .set_results(1, vec![1, 2, 3, 4])
            .expect("Fail to save the results");
        contract
            .set_winners(1, (vec![], vec![]))
            .expect("Fail to save the winners");
        contract
            .save_registration_contracts_status(1, Status::DrawFinished, vec![100, 101, 102])
            .expect("Save status failed");
        // second draw
        contract
            .open_registrations()
            .expect("Fail to open the Registrations");
        contract
            .save_registration_contracts_status(2, Status::RegistrationsOpen, vec![100, 101, 102])
            .expect("Save status failed");
        contract
            .close_registrations()
            .expect("Fail to open the Registrations");
        contract
            .save_registration_contracts_status(2, Status::RegistrationsClosed, vec![100, 101, 102])
            .expect("Save status failed");
        let contract_salt = contract
            .try_to_generate_salt()
            .expect("Fail to generate salt");
        assert_eq!(contract_salt.0, None);
        assert_eq!(contract_salt.1, vec![100, 101, 102]);

        assert_eq!(contract.get_status(), Ok(Status::WaitingSalt));

        contract.save_salts(2, vec![(101, [1u8;32].to_vec())])
            .expect("Fail to save the salts");
        let contract_salt = contract
            .try_to_generate_salt()
            .expect("Fail to generate salt");
        assert_eq!(contract_salt.0, None);
        assert_eq!(contract_salt.1, vec![100, 102]);

        assert_eq!(contract.get_status(), Ok(Status::WaitingSalt));

        contract.save_salts(2, vec![(102, [2u8;32].to_vec())])
            .expect("Fail to save the salts");
        let contract_salt = contract
            .try_to_generate_salt()
            .expect("Fail to generate salt");
        assert!(contract_salt.0.is_some(), "Salt not generated");
        assert_eq!(contract_salt.1, vec![]);

        assert_eq!(contract.get_status(), Ok(Status::WaitingResult));

        contract
            .set_results(2, vec![10, 35, 8, 10])
            .expect("Fail to save the results");
        contract
            .set_winners(2, (vec![], vec![]))
            .expect("Fail to save the winners");
        contract
            .save_registration_contracts_status(2, Status::DrawFinished, vec![100, 101, 102])
            .expect("Save status failed");
    }
}
