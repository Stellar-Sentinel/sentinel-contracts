#![cfg(test)]
use super::*;
use soroban_sdk::testutils::{Address as _, Events as _, MockAuth, MockAuthInvoke};
use soroban_sdk::IntoVal;

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
#[should_panic(expected = "agent registry is full")]
fn agent_registry_rejects_more_than_its_maximum() {
    let env = Env::default();
    let contract_id = env.register(StellarSentinel, ());
    let client = StellarSentinelClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &70);

    for _ in 0..=MAX_AUTHORIZED_AGENTS {
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
