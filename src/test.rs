#![cfg(test)]
use super::*;
use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::{Symbol, TryFromVal};

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
