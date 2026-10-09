#![cfg(test)]
use super::*;
use soroban_sdk::{testutils::Address as _, vec};

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
