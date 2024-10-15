extern crate alloc;

use crate::error::RaffleDrawError::{self, *};
use crate::types::*;
//use alloc::boxed::Box;
use alloc::vec::Vec;
use phat_offchain_rollup::clients::ink::{Action, InkRollupClient};
use scale::Encode;

use pink_extension::ResultExt;
//use pink_web3::keys::pink::KeyPair;
use crate::raffle_manager_contract::{
    LottoManagerRequestMessage, LottoManagerResponseMessage, RaffleManagerContract,
    RaffleManagerStatus,
};
use crate::raffle_registration_contract::{
    RaffleRegistrationContract, RaffleRegistrationStatus, RequestForAction,
};

pub struct WasmContract {
    config: WasmContractConfig,
}

impl WasmContract {
    pub fn new(config: Option<WasmContractConfig>) -> Result<Self, RaffleDrawError> {
        let config = config.ok_or(WasmContractNotConfigured)?;
        Ok(Self { config })
    }

    pub fn connect(config: &WasmContractConfig) -> Result<InkRollupClient, RaffleDrawError> {
        let result = InkRollupClient::new(
            &config.rpc,
            config.pallet_id,
            config.call_id,
            &config.contract_id,
        )
        .log_err("failed to create rollup client");

        match result {
            Ok(client) => Ok(client),
            Err(e) => {
                pink_extension::error!("Error : {:?}", e);
                ink::env::debug_println!("Error : {:?}", e);
                Err(FailedToCreateClient)
            }
        }
    }

    pub fn maybe_submit_tx(
        client: InkRollupClient,
        attest_key: &[u8; 32],
        sender_key: Option<&[u8; 32]>,
    ) -> Result<Option<Vec<u8>>, RaffleDrawError> {
        let maybe_submittable = client
            .commit()
            .log_err("failed to commit")
            .map_err(|_| FailedToCommitTx)?;

        if let Some(submittable) = maybe_submittable {
            let tx_id = if let Some(sender_key) = sender_key {
                // Prefer to meta-tx
                submittable
                    .submit_meta_tx(attest_key, sender_key)
                    .log_err("failed to submit rollup meta-tx")?
            } else {
                // Fallback to account-based authentication
                submittable
                    .submit(attest_key)
                    .log_err("failed to submit rollup tx")?
            };
            return Ok(Some(tx_id));
        }
        Ok(None)
    }
}

impl RaffleRegistrationContract for WasmContract {

    fn do_action(
        &self,
        expected_draw_number: DrawNumber,
        expected_status: RaffleRegistrationStatus,
        action: RequestForAction,
        attest_key: &[u8; 32],
    ) -> Result<bool, RaffleDrawError> {
        // connect to the contract
        let mut client = Self::connect(&self.config)?;

        let draw_number = get_draw_number(&mut client)?
            .ok_or(DrawNumberUnknown)?;
        let status = get_status(&mut client)?
            .ok_or(StatusUnknown)?;

        if draw_number == expected_draw_number && status == expected_status {
            // the contract is already synchronized
            return Ok(true);
        }

        // synchronize the contract =>  Attach an action to the tx
        client.action(Action::Reply(scale::Encode::encode(&action)));
        // submit the transaction
        Self::maybe_submit_tx(client, attest_key, self.config.sender_key.as_ref())?;

        Ok(false)
    }
}

impl RaffleManagerContract for WasmContract {
    fn get_raffle_manager_status(&self) -> Option<RaffleManagerStatus> {
        // use kv store
        None // TODO
    }
/*
    fn get_request(&self) -> Result<Option<LottoManagerRequestMessage>, RaffleDrawError> {
        self.pop()?
    }

    fn send_response(
        &mut self,
        response: LottoManagerResponseMessage,
        attest_key: &[u8],
    ) -> Result<Option<Vec<u8>>, RaffleDrawError> {
        // Attach an action to the tx by:
        self.action(Action::Reply(response.encode()));

        Self::maybe_submit_tx(self, attest_key, self.config.sender_key.as_ref())?
    }

 */
}

const DRAW_NUMBER: u32 = ink::selector_id!("DRAW_NUMBER");
const STATUS: u32 = ink::selector_id!("STATUS");

fn get_draw_number(
    client: &mut InkRollupClient,
) -> Result<Option<DrawNumber>, RaffleDrawError> {
    client
        .get(&DRAW_NUMBER)
        .log_err("Draw number unknown in kv store")
        .map_err(|_| DrawNumberUnknown)
}

fn get_status(
    client: &mut InkRollupClient,
) -> Result<Option<RaffleRegistrationStatus>, RaffleDrawError> {
    client
        .get(&STATUS)
        .log_err("Status unknown in kv store")
        .map_err(|_| StatusUnknown)
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::raffle_manager_contract::{LottoManagerRequestMessage, LottoManagerResponseMessage};
    use scale::{Decode, Encode};

    #[ink::test]
    fn encode_response_numbers() {
        let _ = env_logger::try_init();
        pink_extension_runtime::mock_ext::mock_all_ext();

        //Request received for raffle 6 - draw 4 numbers between 1 and 50
        // Numbers: [4, 49, 41, 16]

        let draw_number = 6;
        let numbers = vec![4, 49, 41, 16];
        let hash = [0; 32];

        let response = LottoManagerResponseMessage::WinningNumbers(draw_number, numbers, hash);
        let encoded_response = response.encode();
        ink::env::debug_println!("Reply response numbers: {encoded_response:02x?}");

        let winners = vec![];
        let response = LottoManagerResponseMessage::Winners(draw_number, winners, hash);
        let encoded_response = response.encode();
        ink::env::debug_println!("Reply response winners: {encoded_response:02x?}");
    }

    #[ink::test]
    fn encode_keys() {
        const QUEUE_PREFIX: &[u8] = b"q/";

        const QUEUE_HEAD_KEY: &[u8] = b"_head";
        let head_key = [QUEUE_PREFIX, QUEUE_HEAD_KEY].concat();
        ink::env::debug_println!("queue head key: {head_key:02x?}");

        const QUEUE_TAIL_KEY: &[u8] = b"_tail";
        let tail_key = [QUEUE_PREFIX, QUEUE_TAIL_KEY].concat();
        ink::env::debug_println!("queue tail key: {tail_key:02x?}");

        let id: u32 = 11;
        let key = [QUEUE_PREFIX, &id.encode()].concat();
        ink::env::debug_println!("queue key: {key:02x?}");
    }

    #[ink::test]
    fn decode_message() {
        let encoded_message: Vec<u8> =
            hex::decode("0600000001100400310029001000").expect("hex decode failed");
        let message = LottoManagerRequestMessage::decode(&mut encoded_message.as_slice())?;
        ink::env::debug_println!("message: {message:?}");

        let encoded_message: Vec<u8> =
            hex::decode("07000000000401003200").expect("hex decode failed");
        let message = LottoManagerRequestMessage::decode(&mut encoded_message.as_slice())?;
        ink::env::debug_println!("message: {message:?}");
    }
}
