#![cfg(test)]
use super::*;
use soroban_sdk::testutils::{Address as _, Events as _, Ledger as _, MockAuth, MockAuthInvoke};
use soroban_sdk::IntoVal;
use soroban_sdk::{vec, BytesN, Symbol, TryFromVal};

#[test]
fn test_initialize_and_threshold() {
    let env = Env::default();
    let contract_id = env.register(StellarSentinel, ());
    let client = StellarSentinelClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &75);
    assert_eq!(client.get_threshold(), 75);
    assert!(!client.is_agent(&admin));
}

#[test]
fn administrator_transfer_requires_both_parties_and_preserves_state() {
    let env = Env::default();
    let contract_id = env.register(StellarSentinel, ());
    let client = StellarSentinelClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let next_admin = Address::generate(&env);
    let agent = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &75);
    client.authorize_agent(&admin, &agent);
    let wrong_admin = Address::generate(&env);
    assert!(client
        .try_transfer_admin(&wrong_admin, &next_admin)
        .is_err());
    assert_eq!(client.get_threshold(), 75);
    assert!(client.is_agent(&agent));

    let transfer = MockAuthInvoke {
        contract: &contract_id,
        fn_name: "transfer_admin",
        args: (&admin, &next_admin).into_val(&env),
        sub_invokes: &[],
    };
    env.mock_auths(&[
        MockAuth {
            address: &admin,
            invoke: &transfer,
        },
        MockAuth {
            address: &next_admin,
            invoke: &transfer,
        },
    ]);
    client.transfer_admin(&admin, &next_admin);

    env.mock_all_auths();
    assert!(client.is_agent(&agent));
    assert_eq!(client.get_threshold(), 75);
    assert!(client.try_set_threshold(&admin, &80).is_err());
    client.set_threshold(&next_admin, &80);
    assert_eq!(client.get_threshold(), 80);
}

#[test]
fn administrator_transfer_fails_without_new_admin_acceptance() {
    let env = Env::default();
    let contract_id = env.register(StellarSentinel, ());
    let client = StellarSentinelClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let next_admin = Address::generate(&env);
    let agent = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &75);
    client.authorize_agent(&admin, &agent);

    let transfer = MockAuthInvoke {
        contract: &contract_id,
        fn_name: "transfer_admin",
        args: (&admin, &next_admin).into_val(&env),
        sub_invokes: &[],
    };
    env.mock_auths(&[MockAuth {
        address: &admin,
        invoke: &transfer,
    }]);
    assert!(client.try_transfer_admin(&admin, &next_admin).is_err());
    env.mock_all_auths();
    assert!(client.is_agent(&agent));
    assert_eq!(client.get_threshold(), 75);
    assert!(client.try_set_threshold(&next_admin, &80).is_err());
    client.set_threshold(&admin, &70);
    assert_eq!(client.get_threshold(), 70);
}

#[test]
#[should_panic(expected = "already initialized")]
fn initialization_is_one_time() {
    let env = Env::default();
    let contract_id = env.register(StellarSentinel, ());
    let client = StellarSentinelClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &50);
    client.initialize(&admin, &50);
}

#[test]
#[should_panic(expected = "threshold must be between 0 and 100")]
fn initialization_rejects_out_of_range_threshold() {
    let env = Env::default();
    let contract_id = env.register(StellarSentinel, ());
    let client = StellarSentinelClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &101);
}

#[test]
fn authorized_agent_can_flag_at_or_above_threshold() {
    let env = Env::default();
    let contract_id = env.register(StellarSentinel, ());
    let client = StellarSentinelClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let agent = Address::generate(&env);
    let subject = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &75);
    client.authorize_agent(&admin, &agent);
    client.flag_anomaly(&agent, &subject, &75);
    client.flag_anomaly(&agent, &subject, &90);
    assert_eq!(
        client.get_latest_flag(&subject),
        Some(FlagRecord {
            agent,
            score: 90,
            ledger: env.ledger().sequence(),
            timestamp: env.ledger().timestamp(),
        })
    );
}

#[test]
fn admin_can_change_threshold_and_revoke_agents() {
    let env = Env::default();
    let contract_id = env.register(StellarSentinel, ());
    let client = StellarSentinelClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let agent = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &75);
    client.authorize_agent(&admin, &agent);
    client.set_threshold(&admin, &80);
    assert_eq!(client.get_threshold(), 80);
    client.revoke_agent(&admin, &agent);
    assert!(!client.is_agent(&agent));
}

#[test]
fn admin_changes_emit_typed_configuration_events() {
    let env = Env::default();
    let contract_id = env.register(StellarSentinel, ());
    let client = StellarSentinelClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let agent = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &75);
    client.authorize_agent(&admin, &agent);
    let events = env.events().all();
    assert_eq!(events.len(), 1);
    let (event_contract, topics, value) = events.get(0).unwrap();
    assert_eq!(event_contract, contract_id);
    assert_eq!(
        Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap(),
        symbol_short!("agent_add")
    );
    assert_eq!(
        Address::try_from_val(&env, &topics.get(1).unwrap()).unwrap(),
        admin
    );
    assert_eq!(
        Address::try_from_val(&env, &topics.get(2).unwrap()).unwrap(),
        agent
    );
    assert!(bool::try_from_val(&env, &value).unwrap());

    client.revoke_agent(&admin, &agent);
    let events = env.events().all();
    assert_eq!(events.len(), 1);
    let (_, topics, value) = events.get(0).unwrap();
    assert_eq!(
        Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap(),
        symbol_short!("agent_del")
    );
    assert_eq!(
        Address::try_from_val(&env, &topics.get(1).unwrap()).unwrap(),
        admin
    );
    assert_eq!(
        Address::try_from_val(&env, &topics.get(2).unwrap()).unwrap(),
        agent
    );
    assert!(!bool::try_from_val(&env, &value).unwrap());

    client.set_threshold(&admin, &80);
    let events = env.events().all();
    assert_eq!(events.len(), 1);
    let (_, topics, value) = events.get(0).unwrap();
    assert_eq!(
        Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap(),
        symbol_short!("threshold")
    );
    assert_eq!(
        Address::try_from_val(&env, &topics.get(1).unwrap()).unwrap(),
        admin
    );
    assert_eq!(<(u32, u32)>::try_from_val(&env, &value).unwrap(), (75, 80));
}

#[test]
fn failed_admin_changes_do_not_emit_events_or_change_state() {
    let env = Env::default();
    let contract_id = env.register(StellarSentinel, ());
    let client = StellarSentinelClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let unauthorized = Address::generate(&env);
    let agent = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &75);
    assert!(client.try_authorize_agent(&unauthorized, &agent).is_err());
    assert!(!client.is_agent(&agent));
    assert!(env.events().all().is_empty());

    assert!(client.try_set_threshold(&admin, &101).is_err());
    assert_eq!(client.get_threshold(), 75);
    assert!(env.events().all().is_empty());
}

#[test]
fn flagged_event_schema_remains_unchanged() {
    let env = Env::default();
    let contract_id = env.register(StellarSentinel, ());
    let client = StellarSentinelClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let agent = Address::generate(&env);
    let subject = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &75);
    client.authorize_agent(&admin, &agent);
    client.flag_anomaly(&agent, &subject, &90);

    let events = env.events().all();
    assert_eq!(events.len(), 1);
    let (_, topics, value) = events.get(0).unwrap();
    assert_eq!(topics.len(), 3);
    assert_eq!(
        Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap(),
        symbol_short!("flagged")
    );
    assert_eq!(
        Address::try_from_val(&env, &topics.get(1).unwrap()).unwrap(),
        agent
    );
    assert_eq!(
        Address::try_from_val(&env, &topics.get(2).unwrap()).unwrap(),
        subject
    );
    assert_eq!(u32::try_from_val(&env, &value).unwrap(), 90);
}

#[test]
fn latest_flag_is_empty_before_first_flag() {
    let env = Env::default();
    let contract_id = env.register(StellarSentinel, ());
    let client = StellarSentinelClient::new(&env, &contract_id);
    let subject = Address::generate(&env);
    assert_eq!(client.get_latest_flag(&subject), None);
}

#[test]
#[should_panic(expected = "score below risk threshold")]
fn agent_cannot_flag_below_threshold() {
    let env = Env::default();
    let contract_id = env.register(StellarSentinel, ());
    let client = StellarSentinelClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let agent = Address::generate(&env);
    let subject = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &75);
    client.authorize_agent(&admin, &agent);
    client.flag_anomaly(&agent, &subject, &74);
}

#[test]
#[should_panic(expected = "not an authorized agent")]
fn unauthorized_address_cannot_flag() {
    let env = Env::default();
    let contract_id = env.register(StellarSentinel, ());
    let client = StellarSentinelClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let agent = Address::generate(&env);
    let subject = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &75);
    client.flag_anomaly(&agent, &subject, &90);
}

#[test]
#[should_panic(expected = "not an authorized agent")]
fn revoked_agent_cannot_flag() {
    let env = Env::default();
    let contract_id = env.register(StellarSentinel, ());
    let client = StellarSentinelClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let agent = Address::generate(&env);
    let subject = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &75);
    client.authorize_agent(&admin, &agent);
    client.revoke_agent(&admin, &agent);
    client.flag_anomaly(&agent, &subject, &90);
}

#[test]
#[should_panic(expected = "score must be between 0 and 100")]
fn agent_cannot_submit_score_above_100() {
    let env = Env::default();
    let contract_id = env.register(StellarSentinel, ());
    let client = StellarSentinelClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let agent = Address::generate(&env);
    let subject = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &75);
    client.authorize_agent(&admin, &agent);
    client.flag_anomaly(&agent, &subject, &101);
}

#[test]
fn agent_registry_is_idempotent_and_removes_revoked_agents() {
    let env = Env::default();
    let contract_id = env.register(StellarSentinel, ());
    let client = StellarSentinelClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let agent = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &70);
    client.authorize_agent(&admin, &agent);
    client.authorize_agent(&admin, &agent);
    let agents = client.get_agents(&admin);
    assert_eq!(agents.len(), 1);
    assert_eq!(agents.get(0), Some(agent.clone()));

    client.revoke_agent(&admin, &agent);
    assert_eq!(client.get_agents(&admin).len(), 0);
}

#[test]
#[should_panic(expected = "agent capacity reached")]
fn agent_registry_rejects_more_than_its_maximum() {
    let env = Env::default();
    let contract_id = env.register(StellarSentinel, ());
    let client = StellarSentinelClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &70);

    for _ in 0..=HARD_AGENT_CAPACITY {
        client.authorize_agent(&admin, &Address::generate(&env));
    }
}


#[test]
fn pause_stops_flags_without_blocking_reads_and_unpause_recovers() {
    let env = Env::default();
    let contract_id = env.register(StellarSentinel, ());
    let client = StellarSentinelClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let agent = Address::generate(&env);
    let subject = Address::generate(&env);
    env.mock_all_auths();

    assert!(!client.is_paused());
    client.initialize(&admin, &70);
    client.authorize_agent(&admin, &agent);
    client.flag_anomaly(&agent, &subject, &80);
    let original = client.get_latest_flag(&subject);

    let stranger = Address::generate(&env);
    assert!(client.try_pause(&stranger).is_err());
    assert!(!client.is_paused());

    client.pause(&admin);
    assert!(client.is_paused());
    assert_eq!(client.get_threshold(), 70);
    assert_eq!(client.get_latest_flag(&subject), original);
    assert!(client.try_flag_anomaly(&agent, &subject, &90).is_err());
    assert_eq!(client.get_latest_flag(&subject), original);
    assert!(env.events().all().is_empty());

    client.unpause(&admin);
    assert!(!client.is_paused());
    client.flag_anomaly(&agent, &subject, &90);
    assert_eq!(client.get_latest_flag(&subject).unwrap().score, 90);
}


#[test]
fn batch_agent_administration_updates_all_entries_and_is_idempotent() {
    let env = Env::default();
    let contract_id = env.register(StellarSentinel, ());
    let client = StellarSentinelClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let first = Address::generate(&env);
    let second = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &70);
    client.authorize_agents(&admin, &vec![&env, first.clone(), second.clone(), first.clone()]);
    assert!(client.is_agent(&first));
    assert!(client.is_agent(&second));

    client.revoke_agents(&admin, &vec![&env, first.clone(), first.clone()]);
    assert!(!client.is_agent(&first));
    assert!(client.is_agent(&second));
}

#[test]
#[should_panic(expected = "agent batch exceeds maximum")]
fn batch_agent_administration_rejects_oversized_requests() {
    let env = Env::default();
    let contract_id = env.register(StellarSentinel, ());
    let client = StellarSentinelClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let mut agents = vec![&env];
    env.mock_all_auths();
    client.initialize(&admin, &70);
    for _ in 0..=MAX_AGENT_BATCH {
        agents.push_back(Address::generate(&env));
    }

    client.authorize_agents(&admin, &agents);
}


#[test]
fn versioned_flag_emits_digest_and_updates_latest_record_without_changing_v1() {
    let env = Env::default();
    let contract_id = env.register(StellarSentinel, ());
    let client = StellarSentinelClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let agent = Address::generate(&env);
    let legacy_subject = Address::generate(&env);
    let versioned_subject = Address::generate(&env);
    let digest = BytesN::from_array(&env, &[7; 32]);
    env.mock_all_auths();
    client.initialize(&admin, &75);
    client.authorize_agent(&admin, &agent);
    client.flag_anomaly(&agent, &legacy_subject, &80);
    let legacy_events = env.events().all();
    let (_, topics, value) = legacy_events.get(legacy_events.len() - 1).unwrap();
    assert_eq!(topics.len(), 3);
    assert_eq!(Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap(), symbol_short!("flagged"));
    assert_eq!(u32::try_from_val(&env, &value).unwrap(), 80);
    client.flag_anomaly_v2(&agent, &versioned_subject, &90, &digest);
    let versioned_events = env.events().all();
    let (_, topics, value) = versioned_events.get(versioned_events.len() - 1).unwrap();
    assert_eq!(topics.len(), 4);
    assert_eq!(Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap(), symbol_short!("flaggedv2"));
    assert_eq!(Address::try_from_val(&env, &topics.get(1).unwrap()).unwrap(), agent);
    assert_eq!(Address::try_from_val(&env, &topics.get(2).unwrap()).unwrap(), versioned_subject);
    assert_eq!(BytesN::<32>::try_from_val(&env, &topics.get(3).unwrap()).unwrap(), digest);
    assert_eq!(u32::try_from_val(&env, &value).unwrap(), 90);
    assert_eq!(client.get_latest_flag(&versioned_subject).unwrap().score, 90);
    assert_eq!(client.get_latest_flag(&legacy_subject).unwrap().score, 80);
}

#[test]
fn versioned_flag_enforces_agent_and_score_checks() {
    let env = Env::default();
    let contract_id = env.register(StellarSentinel, ());
    let client = StellarSentinelClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let agent = Address::generate(&env);
    let stranger = Address::generate(&env);
    let subject = Address::generate(&env);
    let digest = BytesN::from_array(&env, &[9; 32]);
    env.mock_all_auths();
    client.initialize(&admin, &75);
    client.authorize_agent(&admin, &agent);
    assert!(client.try_flag_anomaly_v2(&stranger, &subject, &90, &digest).is_err());
    assert!(client.try_flag_anomaly_v2(&agent, &subject, &74, &digest).is_err());
    assert!(client.try_flag_anomaly_v2(&agent, &subject, &101, &digest).is_err());
    assert_eq!(client.get_latest_flag(&subject), None);
    assert!(env.events().all().is_empty());
}

#[test]
fn monitor_and_responder_authority_are_separate_and_revocable() {
    let env = Env::default();
    let contract_id = env.register(StellarSentinel, ());
    let client = StellarSentinelClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let monitor = Address::generate(&env);
    let subject = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &70);
    client.authorize_monitor(&admin, &monitor);
    assert!(client.is_monitor(&monitor));
    assert!(!client.is_responder(&monitor));
    assert!(client.try_flag_anomaly(&monitor, &subject, &80).is_err());

    client.authorize_responder(&admin, &monitor);
    assert!(client.is_responder(&monitor));
    client.flag_anomaly(&monitor, &subject, &80);
    client.revoke_responder(&admin, &monitor);
    assert!(client.is_monitor(&monitor));
    assert!(!client.is_responder(&monitor));
    client.revoke_monitor(&admin, &monitor);
    assert!(!client.is_agent(&monitor));
}

#[test]
fn agent_capacity_and_self_revoke_keep_registry_consistent() {
    let env = Env::default();
    let contract_id = env.register(StellarSentinel, ());
    let client = StellarSentinelClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let agent = Address::generate(&env);
    let second = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &70);
    client.set_agent_capacity(&admin, &1);
    assert_eq!(client.get_agent_capacity(), 1);
    client.authorize_agent(&admin, &agent);
    assert_eq!(client.get_agent_count(), 1);
    assert!(client.try_authorize_agent(&admin, &second).is_err());

    client.revoke_self(&agent);
    assert!(!client.is_agent(&agent));
    assert_eq!(client.get_agent_count(), 0);
    assert_eq!(client.get_agents(&admin).len(), 0);
    client.authorize_agent(&admin, &second);
    assert_eq!(client.get_agent_count(), 1);
}

#[test]
fn expiring_agent_grants_stop_working_at_the_expiry_ledger() {
    let env = Env::default();
    let contract_id = env.register(StellarSentinel, ());
    let client = StellarSentinelClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let agent = Address::generate(&env);
    let subject = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &70);
    let expiry = env.ledger().sequence() + 10;
    client.authorize_agent_until(&admin, &agent, &expiry);
    assert!(client.is_responder(&agent));
    env.ledger().set_sequence_number(expiry);
    assert!(!client.is_agent(&agent));
    assert!(client.try_flag_anomaly(&agent, &subject, &80).is_err());
    assert_eq!(client.get_agent_count(), 0);
    let replacement = Address::generate(&env);
    client.authorize_agent(&admin, &replacement);
    assert_eq!(client.get_agent_count(), 1);
}

#[test]
fn admin_can_clear_latest_flag_without_removing_event_history() {
    let env = Env::default();
    let contract_id = env.register(StellarSentinel, ());
    let client = StellarSentinelClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let agent = Address::generate(&env);
    let subject = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &70);
    client.authorize_agent(&admin, &agent);
    client.flag_anomaly(&agent, &subject, &80);
    assert_eq!(env.events().all().len(), 1);
    assert!(client.get_latest_flag(&subject).is_some());
    client.clear_latest_flag(&admin, &subject);
    assert_eq!(client.get_latest_flag(&subject), None);
    assert!(env.events().all().is_empty());
}

#[test]
fn admin_and_guardian_pauses_gate_all_flag_submission_methods() {
    let env = Env::default();
    let contract_id = env.register(StellarSentinel, ());
    let client = StellarSentinelClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let guardian = Address::generate(&env);
    let agent = Address::generate(&env);
    let subject = Address::generate(&env);
    let digest = BytesN::from_array(&env, &[4; 32]);
    let submissions = vec![&env, FlagSubmission { subject: subject.clone(), score: 80 }];
    env.mock_all_auths();

    client.initialize(&admin, &70);
    client.authorize_agent(&admin, &agent);
    client.set_emergency_guardian(&admin, &guardian);
    client.guardian_pause(&guardian);
    assert!(client.is_guardian_paused());
    assert!(client.try_flag_anomaly(&agent, &subject, &80).is_err());
    assert!(client.try_flag_anomaly_v2(&agent, &subject, &80, &digest).is_err());
    assert!(client.try_flag_anomalies(&agent, &submissions).is_err());

    client.resume_from_guardian_pause(&admin);
    assert!(!client.is_guardian_paused());
    client.set_paused(&admin, &true);
    assert!(client.try_flag_anomaly(&agent, &subject, &80).is_err());
    client.set_paused(&admin, &false);
    client.flag_anomaly(&agent, &subject, &80);
}

#[test]
fn config_snapshot_storage_version_and_initialization_event_are_available() {
    let env = Env::default();
    let contract_id = env.register(StellarSentinel, ());
    let client = StellarSentinelClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    env.mock_all_auths();

    assert_eq!(client.get_storage_version(), 0);
    assert_eq!(client.get_contract_config(), None);
    client.initialize(&admin, &75);
    let events = env.events().all();
    assert_eq!(events.len(), 1);
    let (_, topics, value) = events.get(0).unwrap();
    assert_eq!(Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap(), symbol_short!("init"));
    assert_eq!(u32::try_from_val(&env, &value).unwrap(), 75);
    assert_eq!(client.get_storage_version(), STORAGE_VERSION);
    let config = client.get_contract_config().unwrap();
    assert_eq!(config.admin, admin);
    assert_eq!(config.threshold, 75);
}
