use ink::env::{DefaultEnvironment};
use ink_e2e::subxt::tx::Signer;
use ink_e2e::{build_message, PolkadotConfig};
use openbrush::contracts::access_control::accesscontrol_external::AccessControl;
use openbrush::traits::AccountId;
use scale::Decode;
use scale::Encode;

use lotto::config::Config;
use lotto::raffle_manager;
use lotto::*;

use lotto::raffle_manager::rafflemanager_external::RaffleManager;
use lotto_registration_manager_contract::{lotto_registration_manager_contract, *};

use phat_rollup_anchor_ink::traits::meta_transaction::metatransaction_external::MetaTransaction;
use phat_rollup_anchor_ink::traits::rollup_anchor::rollupanchor_external::RollupAnchor;

use phat_rollup_anchor_ink::traits::rollup_anchor::*;
use lotto::raffle_manager::Winners;

type E2EResult<T> = std::result::Result<T, Box<dyn std::error::Error>>;

async fn alice_instantiates_raffle_manager(
    client: &mut ink_e2e::Client<PolkadotConfig, DefaultEnvironment>,
) -> AccountId {
    let lotto_constructor = lotto_registration_manager_contract::ContractRef::new();
    let lotto_contract_id = client
        .instantiate(
            "lotto_registration_manager_contract",
            &ink_e2e::alice(),
            lotto_constructor,
            0,
            None,
        )
        .await
        .expect("instantiate failed")
        .account_id;

    lotto_contract_id
}

async fn alice_configures_raffle_manager(
    client: &mut ink_e2e::Client<PolkadotConfig, DefaultEnvironment>,
    contract_id: &AccountId,
    config: Config,
    registration_contracts: Vec<RegistrationContractId>,
) {
    let set_config =
        build_message::<lotto_registration_manager_contract::ContractRef>(contract_id.clone())
            .call(|contract| contract.set_config(config));
    client
        .call(&ink_e2e::alice(), set_config, 0, None)
        .await
        .expect("set config failed");

    let set_registration_contracts =
        build_message::<lotto_registration_manager_contract::ContractRef>(contract_id.clone())
            .call(|contract| contract.set_registration_contracts(registration_contracts.clone()));

    client
        .call(&ink_e2e::alice(), set_registration_contracts, 0, None)
        .await
        .expect("set registration contracts failed");

    let min_number_salts = registration_contracts.len() as u8;
    let set_min_number_salts =
        build_message::<lotto_registration_manager_contract::ContractRef>(contract_id.clone())
            .call(|contract| contract.set_min_number_salts(min_number_salts));

    client
        .call(&ink_e2e::alice(), set_min_number_salts, 0, None)
        .await
        .expect("set minimum number of salts failed");
}

async fn alice_grants_bob_as_attestor(
    client: &mut ink_e2e::Client<PolkadotConfig, DefaultEnvironment>,
    contract_id: &AccountId,
) {
    // bob is granted as attestor
    let bob_address = ink::primitives::AccountId::from(ink_e2e::bob().public_key().0);
    let grant_role =
        build_message::<lotto_registration_manager_contract::ContractRef>(contract_id.clone())
            .call(|contract| contract.grant_role(ATTESTOR_ROLE, Some(bob_address)));
    client
        .call(&ink_e2e::alice(), grant_role, 0, None)
        .await
        .expect("grant bob as attestor failed");
}

async fn alice_starts_raffle(
    client: &mut ink_e2e::Client<PolkadotConfig, DefaultEnvironment>,
    contract_id: &AccountId,
    previous_draw_number: DrawNumber,
) {
    let start_raffle =
        build_message::<lotto_registration_manager_contract::ContractRef>(contract_id.clone())
            .call(|contract| contract.start(Some(previous_draw_number)));
    client
        .call(&ink_e2e::alice(), start_raffle, 0, None)
        .await
        .expect("start raffle failed");

    assert_eq!(
        previous_draw_number,
        get_draw_number(client, contract_id).await
    );
    assert_eq!(
        raffle_manager::Status::Started,
        get_manager_status(client, contract_id).await
    );
}

async fn attestor_sends_config_propagated(
    client: &mut ink_e2e::Client<PolkadotConfig, DefaultEnvironment>,
    contract_id: &AccountId,
    registration_contracts: Vec<RegistrationContractId>,
    queue_head: u32,
) {
    let config_hash: [u8;32] = hex::decode("1af688b7e4ccbd51529a15d28753270a04adf361d4eb1cbd9553ef19d353c656")
        .expect("hex decode failed")
        .try_into()
        .expect("incorrect length");
    let payload =
        LottoManagerResponseMessage::ConfigPropagated(registration_contracts.clone(), config_hash.into());

    let actions = vec![
        HandleActionInput::Reply(payload.encode()),
        HandleActionInput::SetQueueHead(queue_head),
    ];
    let rollup_cond_eq =
        build_message::<lotto_registration_manager_contract::ContractRef>(contract_id.clone())
            .call(|contract| contract.rollup_cond_eq(vec![], vec![], actions.clone()));

    /*
           let result = client.call_dry_run(&ink_e2e::bob(), &rollup_cond_eq, 0, None).await;
           assert_eq!(
               result.debug_message(),
               "only attestor should be able to send messages"
           );
    */

    let result = client
        .call(&ink_e2e::bob(), rollup_cond_eq, 0, None)
        .await
        .expect("send config propagated failed");
    // two events : MessageProcessedTo and RaffleDone
    assert!(result.contains_event("Contracts", "ContractEmitted"));
}

async fn attestor_sends_all_registrations_open(
    client: &mut ink_e2e::Client<PolkadotConfig, DefaultEnvironment>,
    contract_id: &AccountId,
    draw_number: DrawNumber,
    registration_contracts: Vec<RegistrationContractId>,
    queue_head: u32,
) {
    let payload =
        LottoManagerResponseMessage::RegistrationsOpen(draw_number, registration_contracts.clone());

    let actions = vec![
        HandleActionInput::Reply(payload.encode()),
        HandleActionInput::SetQueueHead(queue_head),
    ];
    let rollup_cond_eq =
        build_message::<lotto_registration_manager_contract::ContractRef>(contract_id.clone())
            .call(|contract| contract.rollup_cond_eq(vec![], vec![], actions.clone()));

    let _result = client
        .call(&ink_e2e::bob(), rollup_cond_eq, 0, None)
        .await
        .expect("send config propagated failed");
    // two events : MessageProcessedTo and RaffleDone
    //assert!(result.contains_event("Contracts", "ContractEmitted"));
}

async fn alice_close_registrations(
    client: &mut ink_e2e::Client<PolkadotConfig, DefaultEnvironment>,
    contract_id: &AccountId,
) {
    let stop_raffle =
        build_message::<lotto_registration_manager_contract::ContractRef>(contract_id.clone())
            .call(|contract| contract.close_registrations());
    client
        .call(&ink_e2e::alice(), stop_raffle, 0, None)
        .await
        .expect("stop raffle failed");
}

async fn attestor_sends_all_registrations_closed(
    client: &mut ink_e2e::Client<PolkadotConfig, DefaultEnvironment>,
    contract_id: &AccountId,
    draw_number: DrawNumber,
    registration_contracts: Vec<RegistrationContractId>,
    queue_head: u32,
) {
    let payload = LottoManagerResponseMessage::RegistrationsClosed(
        draw_number,
        registration_contracts.clone(),
    );

    let actions = vec![
        HandleActionInput::Reply(payload.encode()),
        HandleActionInput::SetQueueHead(queue_head),
    ];
    let rollup_cond_eq =
        build_message::<lotto_registration_manager_contract::ContractRef>(contract_id.clone())
            .call(|contract| contract.rollup_cond_eq(vec![], vec![], actions.clone()));

    let result = client
        .call(&ink_e2e::bob(), rollup_cond_eq, 0, None)
        .await
        .expect("send registration closed failed");
    // two events : MessageProcessedTo and RaffleDone
    assert!(result.contains_event("Contracts", "ContractEmitted"));
}

async fn attestor_sends_salts(
    client: &mut ink_e2e::Client<PolkadotConfig, DefaultEnvironment>,
    contract_id: &AccountId,
    draw_number: DrawNumber,
    contract_salts : Vec<(RegistrationContractId, Salt)>,
    queue_head: u32,
) {
    let payload = LottoManagerResponseMessage::SaltGenerated(
        draw_number,
        contract_salts.clone(),
    );

    let actions = vec![
        HandleActionInput::Reply(payload.encode()),
        HandleActionInput::SetQueueHead(queue_head),
    ];
    let rollup_cond_eq =
        build_message::<lotto_registration_manager_contract::ContractRef>(contract_id.clone())
            .call(|contract| contract.rollup_cond_eq(vec![], vec![], actions.clone()));

    let result = client
        .call(&ink_e2e::bob(), rollup_cond_eq, 0, None)
        .await
        .expect("save salts failed");
    // two events : MessageProcessedTo and RaffleDone
    assert!(result.contains_event("Contracts", "ContractEmitted"));
}


async fn attestor_sends_winning_numbers(
    client: &mut ink_e2e::Client<PolkadotConfig, DefaultEnvironment>,
    contract_id: &AccountId,
    draw_number: DrawNumber,
    numbers: Vec<Number>,
    config_salt_hash: [u8; 32],
    queue_head: u32,
) {

    let payload =
        LottoManagerResponseMessage::WinningNumbers(draw_number, numbers.clone(), config_salt_hash.into());

    let actions = vec![
        HandleActionInput::Reply(payload.encode()),
        HandleActionInput::SetQueueHead(queue_head),
    ];
    let rollup_cond_eq =
        build_message::<lotto_registration_manager_contract::ContractRef>(contract_id.clone())
            .call(|contract| contract.rollup_cond_eq(vec![], vec![], actions.clone()));

    let result = client
        .call(&ink_e2e::bob(), rollup_cond_eq, 0, None)
        .await
        .      expect("send result failed");
    // two events : MessageProcessedTo and RaffleDone
    assert!(result.contains_event("Contracts", "ContractEmitted"));
}

async fn attestor_sends_winners(
    client: &mut ink_e2e::Client<PolkadotConfig, DefaultEnvironment>,
    contract_id: &AccountId,
    draw_number: DrawNumber,
    winners: Winners,
    numbers_hash: [u8; 32],
    queue_head: u32,
) {
    let payload = LottoManagerResponseMessage::Winners(draw_number, winners.0.clone(), winners.1.clone(), numbers_hash.into());

    let actions = vec![
        HandleActionInput::Reply(payload.encode()),
        HandleActionInput::SetQueueHead(queue_head),
    ];
    let rollup_cond_eq =
        build_message::<lotto_registration_manager_contract::ContractRef>(contract_id.clone())
            .call(|contract| contract.rollup_cond_eq(vec![], vec![], actions.clone()));

    let result = client
        .call(&ink_e2e::bob(), rollup_cond_eq, 0, None)
        .await
        .expect("send winners failed");
    // two events : MessageProcessedTo and RaffleDone
    assert!(result.contains_event("Contracts", "ContractEmitted"));
}

async fn attestor_sends_results_propagated(
    client: &mut ink_e2e::Client<PolkadotConfig, DefaultEnvironment>,
    contract_id: &AccountId,
    draw_number: DrawNumber,
    registration_contracts: Vec<RegistrationContractId>,
    numbers_hash: [u8; 32],
    queue_head: u32,
) {
    let payload = LottoManagerResponseMessage::ResultsPropagated(
        draw_number,
        registration_contracts.clone(),
        numbers_hash.into(),
    );

    let actions = vec![
        HandleActionInput::Reply(payload.encode()),
        HandleActionInput::SetQueueHead(queue_head),
    ];
    let rollup_cond_eq =
        build_message::<lotto_registration_manager_contract::ContractRef>(contract_id.clone())
            .call(|contract| contract.rollup_cond_eq(vec![], vec![], actions.clone()));

    /*
    let result = client
        .call_dry_run(&ink_e2e::bob(), &rollup_cond_eq, 0, None)
        .await;
    assert_eq!(
        result.debug_message(),
        "only attestor should be able to send messages"
    );
     */
    let result = client
        .call(&ink_e2e::bob(), rollup_cond_eq, 0, None)
        .await
        .expect("send results propagated failed");
    // two events : MessageProcessedTo and RaffleDone
    assert!(result.contains_event("Contracts", "ContractEmitted"));
}

async fn get_draw_number(
    client: &mut ink_e2e::Client<PolkadotConfig, DefaultEnvironment>,
    contract_id: &AccountId,
) -> DrawNumber {
    let get_draw_number =
        build_message::<lotto_registration_manager_contract::ContractRef>(contract_id.clone())
            .call(|contract| contract.get_draw_number());

    client
        .call_dry_run(&ink_e2e::alice(), &get_draw_number, 0, None)
        .await
        .return_value()
        .expect("fail to get the draw number")
}

async fn get_manager_status(
    client: &mut ink_e2e::Client<PolkadotConfig, DefaultEnvironment>,
    contract_id: &AccountId,
) -> raffle_manager::Status {
    let get_status =
        build_message::<lotto_registration_manager_contract::ContractRef>(contract_id.clone())
            .call(|contract| contract.get_status());

    client
        .call_dry_run(&ink_e2e::alice(), &get_status, 0, None)
        .await
        .return_value()
        .expect("fail to get the status")
}

async fn can_close_registrations(
    client: &mut ink_e2e::Client<PolkadotConfig, DefaultEnvironment>,
    contract_id: &AccountId,
) -> bool {
    let can_close_registrations =
        build_message::<lotto_registration_manager_contract::ContractRef>(contract_id.clone())
            .call(|contract| contract.can_close_registrations());

    client
        .call_dry_run(&ink_e2e::alice(), &can_close_registrations, 0, None)
        .await
        .return_value()
}

async fn has_pending_message(
    client: &mut ink_e2e::Client<PolkadotConfig, DefaultEnvironment>,
    contract_id: &AccountId,
) -> bool {
    let has_pending_message =
        build_message::<lotto_registration_manager_contract::ContractRef>(contract_id.clone())
            .call(|contract| contract.has_pending_message());

    client
        .call_dry_run(&ink_e2e::alice(), &has_pending_message, 0, None)
        .await
        .return_value()
}

async fn get_results(
    client: &mut ink_e2e::Client<PolkadotConfig, DefaultEnvironment>,
    contract_id: &AccountId,
    draw_number: DrawNumber,
) -> Option<Vec<Number>> {
    let get_results =
        build_message::<lotto_registration_manager_contract::ContractRef>(contract_id.clone())
            .call(|contract| contract.get_results(draw_number));

    let result = client
        .call_dry_run(&ink_e2e::alice(), &get_results, 0, None)
        .await;

    result.return_value()
}

async fn get_winners(
    client: &mut ink_e2e::Client<PolkadotConfig, DefaultEnvironment>,
    contract_id: &AccountId,
    draw_number: DrawNumber,
) -> Option<Winners> {
    let get_winners =
        build_message::<lotto_registration_manager_contract::ContractRef>(contract_id.clone())
            .call(|contract| contract.get_winners(draw_number));

    let result = client
        .call_dry_run(&ink_e2e::alice(), &get_winners, 0, None)
        .await;

    result.return_value()
}

async fn get_messages_in_queue(
    client: &mut ink_e2e::Client<PolkadotConfig, DefaultEnvironment>,
    contract_id: &AccountId,
) -> Vec<LottoManagerRequestMessage> {
    const QUEUE_PREFIX: &[u8] = b"q/";
    const QUEUE_HEAD_KEY: &[u8] = b"_head";
    const QUEUE_TAIL_KEY: &[u8] = b"_tail";

    let get_queue_head =
        build_message::<lotto_registration_manager_contract::ContractRef>(contract_id.clone())
            .call(|contract| contract.get_value([QUEUE_PREFIX, QUEUE_HEAD_KEY].concat()));

    let result = client
        .call_dry_run(&ink_e2e::alice(), &get_queue_head, 0, None)
        .await;

    let queue_head: Option<Vec<u8>> = result.return_value();
    let queue_head = match queue_head {
        Some(v) => u32::decode(&mut v.as_slice()).ok().unwrap_or_default(),
        None => 0,
    };

    let get_queue_tail =
        build_message::<lotto_registration_manager_contract::ContractRef>(contract_id.clone())
            .call(|contract| contract.get_value([QUEUE_PREFIX, QUEUE_TAIL_KEY].concat()));

    let result = client
        .call_dry_run(&ink_e2e::alice(), &get_queue_tail, 0, None)
        .await;

    let queue_tail: Option<Vec<u8>> = result.return_value();
    let queue_tail = match queue_tail {
        Some(v) => u32::decode(&mut v.as_slice()).ok().unwrap_or_default(),
        None => 0,
    };

    let mut messages = Vec::new();
    for i in queue_head..queue_tail {
        let get_message =
            build_message::<lotto_registration_manager_contract::ContractRef>(contract_id.clone())
                .call(|contract| contract.get_value([QUEUE_PREFIX, &i.encode()].concat()));

        let result = client
            .call_dry_run(&ink_e2e::alice(), &get_message, 0, None)
            .await;

        let message: Option<Vec<u8>> = result.return_value();
        if let Some(v) = message {
            if let Some(m) = LottoManagerRequestMessage::decode(&mut v.as_slice()).ok() {
                messages.push(m);
            }
        };
    }
    messages
}

#[ink_e2e::test(additional_contracts = "contracts/raffle_manager/Cargo.toml")]
async fn test_raffles(mut client: ink_e2e::Client<C, E>) -> E2EResult<()> {
    // given
    let contract_id = alice_instantiates_raffle_manager(&mut client).await;

    let registration_contracts = vec![101, 102, 103];

    let config = Config {
        nb_numbers: 4,
        min_number: 1,
        max_number: 50,
    };

    // configure the raffle
    alice_configures_raffle_manager(
        &mut client,
        &contract_id,
        config.clone(),
        registration_contracts.clone(),
    )
    .await;

    // bob is granted as attestor
    alice_grants_bob_as_attestor(&mut client, &contract_id).await;

    assert_eq!(0, get_draw_number(&mut client, &contract_id).await);
    assert_eq!(
        raffle_manager::Status::NotStarted,
        get_manager_status(&mut client, &contract_id).await
    );

    // check the message queue
    assert_eq!(false, has_pending_message(&mut client, &contract_id).await);

    // start the raffle
    alice_starts_raffle(&mut client, &contract_id, 10).await;

    // check the message queue
    assert_eq!(true, has_pending_message(&mut client, &contract_id).await);
    let messages = get_messages_in_queue(&mut client, &contract_id).await;
    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0],
        LottoManagerRequestMessage::PropagateConfig(config.clone(), vec![101, 102, 103])
    );

    let mut queue_head = 1;

    // propagate the config
    attestor_sends_config_propagated(&mut client, &contract_id, vec![], queue_head).await;
    queue_head += 1;

    // the registrations are not open because all contracts are not synched
    assert_eq!(10, get_draw_number(&mut client, &contract_id).await);
    assert_eq!(
        raffle_manager::Status::Started,
        get_manager_status(&mut client, &contract_id).await
    );

    // check the message in the queue
    let messages = get_messages_in_queue(&mut client, &contract_id).await;
    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0],
        LottoManagerRequestMessage::PropagateConfig(config.clone(), vec![101, 102, 103])
    );

    // propagate the missing config
    attestor_sends_config_propagated(&mut client, &contract_id, vec![101, 103], queue_head).await;
    queue_head += 1;

    // the registrations are not open because all contracts are not synched
    assert_eq!(10, get_draw_number(&mut client, &contract_id).await);
    assert_eq!(
        raffle_manager::Status::Started,
        get_manager_status(&mut client, &contract_id).await
    );

    // check the message in the queue
    let messages = get_messages_in_queue(&mut client, &contract_id).await;
    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0],
        LottoManagerRequestMessage::PropagateConfig(config.clone(), vec![102])
    );

    // propagate the missing config
    attestor_sends_config_propagated(&mut client, &contract_id, vec![102], queue_head).await;
    queue_head += 1;

    // the registrations are now open
    let draw_number = get_draw_number(&mut client, &contract_id).await;
    assert_eq!(draw_number, 11);
    assert_eq!(
        raffle_manager::Status::RegistrationsOpen,
        get_manager_status(&mut client, &contract_id).await
    );

    // check the messages in the queue
    let messages = get_messages_in_queue(&mut client, &contract_id).await;
    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0],
        LottoManagerRequestMessage::OpenRegistrations(draw_number, vec![101, 102, 103])
    );

    // propagate registrations are open
    attestor_sends_all_registrations_open(
        &mut client,
        &contract_id,
        draw_number,
        vec![],
        queue_head,
    )
    .await;
    queue_head += 1;

    // all contracts are not synched
    // check the messages in the queue
    let messages = get_messages_in_queue(&mut client, &contract_id).await;
    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0],
        LottoManagerRequestMessage::OpenRegistrations(draw_number, vec![101, 102, 103])
    );

    // propagate all registrations are open
    attestor_sends_all_registrations_open(
        &mut client,
        &contract_id,
        draw_number,
        vec![101, 102, 103],
        queue_head,
    )
    .await;
    queue_head += 1;

    // all contracts are synched
    // check the messages in the queue
    assert_eq!(false, has_pending_message(&mut client, &contract_id).await);
    let messages = get_messages_in_queue(&mut client, &contract_id).await;
    assert_eq!(messages.len(), 0);

    assert_eq!(
        raffle_manager::Status::RegistrationsOpen,
        get_manager_status(&mut client, &contract_id).await
    );

    // stop the registrations
    assert_eq!(
        true,
        can_close_registrations(&mut client, &contract_id).await
    );
    alice_close_registrations(&mut client, &contract_id).await;
    assert_eq!(
        raffle_manager::Status::RegistrationsClosed,
        get_manager_status(&mut client, &contract_id).await
    );

    // propagate registrations are closed
    attestor_sends_all_registrations_closed(
        &mut client,
        &contract_id,
        draw_number,
        vec![103],
        queue_head,
    )
    .await;
    queue_head += 1;

    // all contracts are not synched
    // check the message in the queue
    assert_eq!(true, has_pending_message(&mut client, &contract_id).await);
    let messages = get_messages_in_queue(&mut client, &contract_id).await;
    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0],
        LottoManagerRequestMessage::CloseRegistrations(draw_number, vec![101, 102])
    );

    // propagate registrations are closed
    attestor_sends_all_registrations_closed(
        &mut client,
        &contract_id,
        draw_number,
        vec![101, 102],
        queue_head,
    )
    .await;
    queue_head += 1;

    // all contracts are synched, generate the salts
    // check the message in the queue
    let messages = get_messages_in_queue(&mut client, &contract_id).await;
    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0],
        LottoManagerRequestMessage::GenerateSalt(draw_number, vec![101, 102, 103])
    );
    // send the salts
    attestor_sends_salts(
        &mut client,
        &contract_id,
        draw_number,
        vec![(103, [3u8;32].to_vec())],
        queue_head,
    ).await;
    queue_head += 1;

    // all contracts are not synched
    // check the message in the queue
    assert_eq!(true, has_pending_message(&mut client, &contract_id).await);
    let messages = get_messages_in_queue(&mut client, &contract_id).await;
    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0],
        LottoManagerRequestMessage::GenerateSalt(draw_number, vec![101, 102])
    );
    // send the salts
    attestor_sends_salts(
        &mut client,
        &contract_id,
        draw_number,
        vec![(101, [1u8;32].to_vec()), (102, [2u8;32].to_vec())],
        queue_head,
    ).await;
    queue_head += 1;

    // all contracts are synched, send the results
    // check the message in the queue
    let generated_salt: [u8;32] = [101, 183, 131, 128, 194, 210, 6, 186, 135, 158, 6, 247, 69, 144, 120, 98, 45, 169, 95, 8, 91, 222, 225, 175, 72, 14, 187, 148, 7, 210, 251, 70];
    let messages = get_messages_in_queue(&mut client, &contract_id).await;
    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0],
        LottoManagerRequestMessage::DrawNumbers(draw_number, config, generated_salt.to_vec())
    );

    let config_salt_hash: [u8;32] = hex::decode("94e1fa775bc259340a60dda2a2f10e911b6343e6ab0932726c738097c8fc3521")
        .expect("hex decode failed")
        .try_into()
        .expect("incorrect length");

    let numbers: Vec<Number> = vec![5, 40, 8, 2];
    let numbers_hash: [u8;32] = hex::decode("0c70b0cb9b2d87768d1efacd6ca6a89be08a4c8c70855b54455f7f46caeeb155")
        .expect("hex decode failed")
        .try_into()
        .expect("incorrect length");

    // send the winning numbers
    attestor_sends_winning_numbers(
        &mut client,
        &contract_id,
        draw_number,
        numbers.clone(),
        config_salt_hash,
        queue_head,
    )
    .await;
    queue_head += 1;

    assert_eq!(
        raffle_manager::Status::WaitingWinner,
        get_manager_status(&mut client, &contract_id).await
    );

    // check the message in the queue
    let messages = get_messages_in_queue(&mut client, &contract_id).await;
    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0],
        LottoManagerRequestMessage::CheckWinners(draw_number, numbers.clone())
    );

    // send no winner
    let winners: Winners = (vec![], vec![]);
    attestor_sends_winners(
        &mut client,
        &contract_id,
        draw_number,
        winners,
        numbers_hash.clone(),
        queue_head,
    )
    .await;
    queue_head += 1;

    // check the status
    assert_eq!(
        raffle_manager::Status::DrawFinished,
        get_manager_status(&mut client, &contract_id).await
    );

    // check the results
    assert_eq!(
        Some(numbers.clone()),
        get_results(&mut client, &contract_id, draw_number).await
    );

    // check the winners
    assert_eq!(
        None,
        get_winners(&mut client, &contract_id, draw_number).await
    );

    // check the message in the queue
    let messages = get_messages_in_queue(&mut client, &contract_id).await;
    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0],
        LottoManagerRequestMessage::PropagateResults(
            draw_number,
            numbers.clone(),
            false,
            vec![101, 102, 103]
        )
    );

    // propagate the results
    attestor_sends_results_propagated(&mut client, &contract_id, draw_number, vec![], numbers_hash.clone(), queue_head)
        .await;
    queue_head += 1;

    // all contracts are not synched
    // check the message in the queue
    let messages = get_messages_in_queue(&mut client, &contract_id).await;
    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0],
        LottoManagerRequestMessage::PropagateResults(
            draw_number,
            numbers.clone(),
            false,
            vec![101, 102, 103]
        )
    );

    // propagate the results
    attestor_sends_results_propagated(
        &mut client,
        &contract_id,
        draw_number,
        vec![101, 102, 103],
        numbers_hash.clone(),
        queue_head,
    )
    .await;
    queue_head += 1;

    // all contracts are synched
    // new draw number
    let draw_number = get_draw_number(&mut client, &contract_id).await;
    assert_eq!(draw_number, 12);
    assert_eq!(
        raffle_manager::Status::RegistrationsOpen,
        get_manager_status(&mut client, &contract_id).await
    );

    // check the message in the queue
    // the registrations are opened again
    let messages = get_messages_in_queue(&mut client, &contract_id).await;
    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0],
        LottoManagerRequestMessage::OpenRegistrations(draw_number, vec![101, 102, 103])
    );

    // propagate all registrations are open
    attestor_sends_all_registrations_open(
        &mut client,
        &contract_id,
        draw_number,
        vec![101, 102, 103],
        queue_head,
    )
        .await;
    queue_head += 1;

    // stop the registrations
    assert_eq!(
        true,
        can_close_registrations(&mut client, &contract_id).await
    );
    alice_close_registrations(&mut client, &contract_id).await;

    // propagate all registrations are closed
    attestor_sends_all_registrations_closed(
        &mut client,
        &contract_id,
        draw_number,
        vec![101, 102, 103],
        queue_head,
    )
        .await;
    queue_head += 1;

    // send the salts
    attestor_sends_salts(
        &mut client,
        &contract_id,
        draw_number,
        vec![(101, [1u8;32].to_vec()), (102, [2u8;32].to_vec()), (103, [3u8;32].to_vec())],
        queue_head,
    ).await;
    queue_head += 1;

    // all contracts are synched, send the results
    // check the message in the queue
    let generated_salt: [u8;32] =  [94, 193, 212, 179, 22, 80, 18, 236, 194, 56, 99, 20, 16, 125, 123, 20, 14, 26, 212, 42, 96, 187, 51, 110, 129, 113, 120, 162, 223, 50, 36, 79];
    let messages = get_messages_in_queue(&mut client, &contract_id).await;
    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0],
        LottoManagerRequestMessage::DrawNumbers(draw_number, config, generated_salt.to_vec())
    );

    let config_salt_hash: [u8;32] = hex::decode("c6aac4e20883f260241bbae6963be7ae78d9cc0136f0a2409aa40e0fdef11cb1")
        .expect("hex decode failed")
        .try_into()
        .expect("incorrect length");

    let numbers: Vec<Number> = vec![15, 20, 1, 31];
    let numbers_hash: [u8;32] = hex::decode("2a8b8764a606b81095017886e6e46482bf2f248969279ea3c063265b060794ae")
        .expect("hex decode failed")
        .try_into()
        .expect("incorrect length");

    // send the winning numbers
    attestor_sends_winning_numbers(
        &mut client,
        &contract_id,
        draw_number,
        numbers.clone(),
        config_salt_hash,
        queue_head,
    )
        .await;
    queue_head += 1;

    // send a winner
    let dave_address = ink_e2e::dave().public_key().0;
    let winners: Winners = (vec![dave_address], vec![]);
    attestor_sends_winners(
        &mut client,
        &contract_id,
        draw_number,
        winners,
        numbers_hash.clone(),
        queue_head,
    )
        .await;
    queue_head += 1;

    // check the status
    assert_eq!(
        raffle_manager::Status::DrawFinished,
        get_manager_status(&mut client, &contract_id).await
    );

    // check the results
    assert_eq!(
        Some(numbers.clone()),
        get_results(&mut client, &contract_id, draw_number).await
    );

    // check the winners
    assert_eq!(
        Some((vec![dave_address], vec![])),
        get_winners(&mut client, &contract_id, draw_number).await
    );

    // propagate the results
    attestor_sends_results_propagated(
        &mut client,
        &contract_id,
        draw_number,
        vec![101, 102, 103],
        numbers_hash.clone(),
        queue_head,
    )
        .await;

    // all contracts are synched
    // There is a winner the lotto is stopped

    let draw_number = get_draw_number(&mut client, &contract_id).await;
    assert_eq!(draw_number, 12);
    assert_eq!(
        raffle_manager::Status::DrawFinished,
        get_manager_status(&mut client, &contract_id).await
    );

    // check no message in the queue
    let messages = get_messages_in_queue(&mut client, &contract_id).await;
    assert_eq!(messages.len(), 0);

    Ok(())
}

#[ink_e2e::test(additional_contracts = "contracts/raffle_manager/Cargo.toml")]
async fn test_bad_attestor(mut client: ink_e2e::Client<C, E>) -> E2EResult<()> {
    // given
    let contract_id = alice_instantiates_raffle_manager(&mut client).await;

    // bob is not granted as attestor => it should not be able to send a message
    let rollup_cond_eq =
        build_message::<lotto_registration_manager_contract::ContractRef>(contract_id.clone())
            .call(|contract| contract.rollup_cond_eq(vec![], vec![], vec![]));
    let result = client.call(&ink_e2e::bob(), rollup_cond_eq, 0, None).await;
    assert!(
        result.is_err(),
        "only attestor should be able to send messages"
    );

    // bob is granted as attestor
    alice_grants_bob_as_attestor(&mut client, &contract_id).await;

    // then bob is able to send a message
    let rollup_cond_eq =
        build_message::<lotto_registration_manager_contract::ContractRef>(contract_id.clone())
            .call(|contract| contract.rollup_cond_eq(vec![], vec![], vec![]));
    let result = client
        .call(&ink_e2e::bob(), rollup_cond_eq, 0, None)
        .await
        .expect("rollup cond eq failed");
    // no event
    assert!(!result.contains_event("Contracts", "ContractEmitted"));

    Ok(())
}

#[ink_e2e::test(additional_contracts = "contracts/raffle_manager/Cargo.toml")]
async fn test_bad_messages(mut client: ink_e2e::Client<C, E>) -> E2EResult<()> {
    // given
    let contract_id = alice_instantiates_raffle_manager(&mut client).await;

    // bob is granted as attestor
    alice_grants_bob_as_attestor(&mut client, &contract_id).await;

    let actions = vec![HandleActionInput::Reply(58u128.encode())];
    let rollup_cond_eq =
        build_message::<lotto_registration_manager_contract::ContractRef>(contract_id.clone())
            .call(|contract| contract.rollup_cond_eq(vec![], vec![], actions.clone()));
    let result = client.call(&ink_e2e::bob(), rollup_cond_eq, 0, None).await;
    assert!(
        result.is_err(),
        "we should not be able to proceed bad messages"
    );

    Ok(())
}

///
/// Test the meta transactions
/// Alice is the owner
/// Bob is the attestor
/// Charlie is the sender (ie the payer)
///
#[ink_e2e::test(additional_contracts = "contracts/raffle_manager/Cargo.toml")]
async fn test_meta_tx_rollup_cond_eq(mut client: ink_e2e::Client<C, E>) -> E2EResult<()> {
    let contract_id = alice_instantiates_raffle_manager(&mut client).await;

    // Bob is the attestor
    // use the ecsda account because we are not able to verify the sr25519 signature
    let from = ink::primitives::AccountId::from(
        Signer::<PolkadotConfig>::account_id(&subxt_signer::ecdsa::dev::bob()).0,
    );

    // add the role => it should succeed
    let grant_role =
        build_message::<lotto_registration_manager_contract::ContractRef>(contract_id.clone())
            .call(|contract| contract.grant_role(ATTESTOR_ROLE, Some(from)));
    client
        .call(&ink_e2e::alice(), grant_role, 0, None)
        .await
        .expect("grant the attestor failed");

    // prepare the meta transaction
    let data = RollupCondEqMethodParams::encode(&(vec![], vec![], vec![]));
    let prepare_meta_tx =
        build_message::<lotto_registration_manager_contract::ContractRef>(contract_id.clone())
            .call(|contract| contract.prepare(from, data.clone()));
    let result = client
        .call(&ink_e2e::bob(), prepare_meta_tx, 0, None)
        .await
        .expect("We should be able to prepare the meta tx");

    let (request, _hash) = result
        .return_value()
        .expect("Expected value when preparing meta tx");

    assert_eq!(0, request.nonce);
    assert_eq!(from, request.from);
    assert_eq!(contract_id, request.to);
    assert_eq!(&data, &request.data);

    // Bob signs the message
    let keypair = subxt_signer::ecdsa::dev::bob();
    let signature = keypair.sign(&scale::Encode::encode(&request)).0;

    // do the meta tx: charlie sends the message
    let meta_tx_rollup_cond_eq =
        build_message::<lotto_registration_manager_contract::ContractRef>(contract_id.clone())
            .call(|contract| contract.meta_tx_rollup_cond_eq(request.clone(), signature));
    client
        .call(&ink_e2e::charlie(), meta_tx_rollup_cond_eq, 0, None)
        .await
        .expect("meta tx rollup cond eq should not failed");

    // do it again => it must fail
    let meta_tx_rollup_cond_eq =
        build_message::<lotto_registration_manager_contract::ContractRef>(contract_id.clone())
            .call(|contract| contract.meta_tx_rollup_cond_eq(request.clone(), signature));
    let result = client
        .call(&ink_e2e::charlie(), meta_tx_rollup_cond_eq, 0, None)
        .await;
    assert!(
        result.is_err(),
        "This message should not be proceed because the nonce is obsolete"
    );

    Ok(())
}
