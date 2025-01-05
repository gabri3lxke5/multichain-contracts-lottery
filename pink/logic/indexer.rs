extern crate alloc;
extern crate core;

use crate::error::RaffleDrawError::{self, *};
use crate::types::*;
use alloc::vec::Vec;
use ink::prelude::{format, string::String};
use pink_extension::{debug, error, http_post, info};
use scale::Encode;
use serde::Deserialize;
use serde_json_core;
use sp_core::crypto::Ss58Codec;

/// DTO use for serializing and deserializing the json when querying the winners
#[derive(Deserialize, Encode, Clone, Debug, PartialEq)]
pub struct IndexerParticipationsResponse<'a> {
    #[serde(borrow)]
    data: IndexerParticipationsResponseData<'a>,
}

#[derive(Deserialize, Encode, Clone, Debug, PartialEq)]
struct IndexerParticipationsResponseData<'a> {
    #[serde(borrow)]
    participations: Participations<'a>,
}

#[derive(Deserialize, Encode, Clone, Debug, PartialEq)]
struct Participations<'a> {
    #[serde(borrow)]
    nodes: Vec<ParticipationNode<'a>>,
}

#[derive(Deserialize, Encode, Clone, Debug, PartialEq)]
#[allow(non_snake_case)]
struct ParticipationNode<'a> {
    accountId: &'a str,
}

/// DTO use for serializing and deserializing the json when querying the hashes
#[derive(Deserialize, Encode, Clone, Debug, PartialEq)]
pub struct IndexerRafflesResponse<'a> {
    #[serde(borrow)]
    data: IndexerRafflesResponseData<'a>,
}

#[derive(Deserialize, Encode, Clone, Debug, PartialEq)]
struct IndexerRafflesResponseData<'a> {
    #[serde(borrow)]
    raffles: RaffleNodes<'a>,
}

#[derive(Deserialize, Encode, Clone, Debug, PartialEq)]
struct RaffleNodes<'a> {
    #[serde(borrow)]
    nodes: Vec<RaffleNode<'a>>,
}

#[derive(Deserialize, Encode, Clone, Debug, PartialEq)]
#[allow(non_snake_case)]
struct RaffleNode<'a> {
    salt: &'a str,
}

pub struct Indexer {
    endpoint: String,
}

impl Indexer {
    pub fn new(url: Option<String>) -> Result<Self, RaffleDrawError> {
        let endpoint = url.ok_or(IndexerNotConfigured)?;
        Ok(Self { endpoint })
    }

    pub fn query_winners(
        &self,
        draw_number: DrawNumber,
        numbers: &Vec<Number>,
    ) -> Result<(Vec<AccountId32>, Vec<AccountId20>), RaffleDrawError> {
        info!(
                "Request received to get the winners for raffle id {draw_number} and numbers {numbers:?} "
            );

        if numbers.is_empty() {
            return Err(NoNumber);
        }

        // build the headers
        let headers = alloc::vec![
            ("Content-Type".into(), "application/json".into()),
            ("Accept".into(), "application/json".into())
        ];
        // build the filter
        let mut filter = format!(
            r#"filter:{{and:[{{drawNumber:{{equalTo:\"{}\"}}}}"#,
            draw_number
        );
        for n in numbers {
            let f = format!(r#",{{numbers:{{contains:\"{}\"}}}}"#, n);
            filter.push_str(&f);
        }
        filter.push_str("]}");

        // build the body
        let body = format!(
            r#"{{"query" : "{{participations({}){{ nodes {{ accountId }} }} }}"}}"#,
            filter
        );

        debug!("body: {body}");

        // query the indexer
        let resp = http_post!(self.endpoint.clone(), body, headers);

        // check the result
        if resp.status_code != 200 {
            ink::env::debug_println!("status code {}", resp.status_code);
            return Err(HttpRequestFailed);
        }

        // parse the result
        let result: IndexerParticipationsResponse =
            serde_json_core::from_slice(resp.body.as_slice())
                .or(Err(InvalidResponseBody))?
                .0;

        // add the winners
        let mut winners_substrate = Vec::new();
        let mut winners_evm = Vec::new();
        for w in result.data.participations.nodes.iter() {
            // build the accountId from the string address
            match w.accountId.len() {
                48 => { // accountId 32
                    let account_id = sp_core::crypto::AccountId32::from_ss58check(w.accountId)
                        .or(Err(InvalidSs58Address))?;
                    let address_hex: AccountId32 = scale::Encode::encode(&account_id)
                        .try_into()
                        .or(Err(InvalidKeyLength))?;
                    winners_substrate.push(address_hex);
                }
                42 => { // accountId 20
                    // remove the prefix 0x
                    let without_0x = w.accountId.get(2..).ok_or(InvalidKeyLength)?;
                    let address_hex: AccountId20 = hex::decode(without_0x)
                        .expect("hex decode failed")
                        .try_into()
                        .or(Err(InvalidKeyLength))?;
                    winners_evm.push(address_hex);
                }
                _ => {
                    error!("Not Supported address: {0:?}", w.accountId);
                    return Err(InvalidKeyLength);
                }
            }
        }

        info!("Winners Substrate: {winners_substrate:02x?} - EVM: {winners_evm:02x?} ");

        Ok((winners_substrate, winners_evm))
    }

    pub fn query_salt(&self, draw_number: DrawNumber, registration_contract_id: RegistrationContractId) -> Result<Salt, RaffleDrawError> {
        info!("Query salt for raffle {draw_number} and contract {registration_contract_id}");

        // build the headers
        let headers = alloc::vec![
            ("Content-Type".into(), "application/json".into()),
            ("Accept".into(), "application/json".into())
        ];
        // build the filter
        let filter = format!(
            r#"filter:{{and:[{{drawNumber:{{equalTo:\"{draw_number}\"}}}},{{registrationContractId:{{equalTo:\"{registration_contract_id}\"}}}}]}}"#,
        );

        // build the body
        let body = format!(
            r#"{{"query" : "{{raffles({}){{ nodes {{ salt }} }} }}"}}"#,
            filter
        );

        debug!("body: {body}");

        // query the indexer
        let resp = http_post!(self.endpoint.clone(), body, headers);

        // check the result
        if resp.status_code != 200 {
            ink::env::debug_println!("status code {}", resp.status_code);
            return Err(HttpRequestFailed);
        }

        // parse the result
        let result: IndexerRafflesResponse = serde_json_core::from_slice(resp.body.as_slice())
            .or(Err(InvalidResponseBody))?
            .0;

        if let Some(node) = result.data.raffles.nodes.iter().next() {
            // remove the prefix 0x
            let without_0x = node.salt.get(2..).ok_or(InvalidResponseBody)?;
            let salt = hex::decode(without_0x).expect("hex decode failed");
            Ok(salt)
        } else {
            Err(NoSalt)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_indexer() -> Indexer {
        Indexer {
            endpoint: "https://query.substrate.fi/lotto-multichain-subquery-testnet".to_string(),
        }
    }

    #[ink::test]
    fn test_get_salt() {
        pink_extension_runtime::mock_ext::mock_all_ext();

        let draw_num = 1;
        let contract_id = 10;

        let indexer = new_indexer();
        let salt = indexer.query_salt(draw_num, contract_id).unwrap();
        ink::env::debug_println!("salt: {salt:?}");
    }

    #[ink::test]
    fn test_get_winner_substrate() {
        pink_extension_runtime::mock_ext::mock_all_ext();

        let draw_num = 1;
        let numbers = vec![9, 14, 25, 37];

        let indexer = new_indexer();
        let winners = indexer.query_winners(draw_num, &numbers).unwrap();
        assert_eq!(1, winners.0.len());
        assert_eq!(0, winners.1.len());
    }

    #[ink::test]
    fn test_get_winner_evm() {
        pink_extension_runtime::mock_ext::mock_all_ext();

        let draw_num = 3;
        let numbers = vec![43, 27, 50, 2];

        let indexer = new_indexer();
        let winners = indexer.query_winners(draw_num, &numbers).unwrap();
        assert_eq!(0, winners.0.len());
        assert_eq!(1, winners.1.len());
    }

    #[ink::test]
    fn test_no_winner() {
        pink_extension_runtime::mock_ext::mock_all_ext();

        let draw_num = 0;
        let numbers = vec![150, 1, 44, 2800];

        let indexer = new_indexer();
        let winners = indexer.query_winners(draw_num, &numbers).unwrap();
        assert_eq!(0, winners.0.len());
        assert_eq!(0, winners.1.len());
    }

    #[ink::test]
    fn test_no_number() {
        pink_extension_runtime::mock_ext::mock_all_ext();

        let draw_num = 0;
        let numbers = vec![];

        let indexer = new_indexer();
        let result = indexer.query_winners(draw_num, &numbers);
        assert_eq!(Err(NoNumber), result);
    }
}
