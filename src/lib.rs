#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env, Symbol, Vec};

// Storage keys
#[contracttype]
pub enum DataKey {
    Admin,
    Agent(Address),
    RiskThreshold,
    LatestFlag(Address),
    // Append registry storage to preserve existing storage-key encoding.
    AgentRegistry,
}

/// Latest flag recorded for a subject. Soroban events remain the append-only
/// history; this record supports cheap current-state lookups.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FlagRecord {
    pub agent: Address,
    pub score: u32,
    pub ledger: u32,
    pub timestamp: u64,
}

/// A single subject and risk score in a bounded agent submission batch.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FlagSubmission {
    pub subject: Address,
    pub score: u32,
}

const FLAG_EVENT: Symbol = symbol_short!("flagged");
const AGENT_ADD_EVENT: Symbol = symbol_short!("agent_add");
const AGENT_DEL_EVENT: Symbol = symbol_short!("agent_del");
const THRESHOLD_EVENT: Symbol = symbol_short!("threshold");
const MAX_SCORE: u32 = 100;
const MAX_FLAG_BATCH: u32 = 16;
const INSTANCE_TTL_THRESHOLD: u32 = 10_000;
const INSTANCE_TTL_BUMP: u32 = 100_000;
const PERSISTENT_TTL_THRESHOLD: u32 = 10_000;
const PERSISTENT_TTL_BUMP: u32 = 100_000;
const MAX_AUTHORIZED_AGENTS: u32 = 128;

#[contract]
pub struct StellarSentinel;

#[contractimpl]
impl StellarSentinel {
    /// One-time setup. Sets the contract admin and a default risk threshold.
    pub fn initialize(env: Env, admin: Address, default_threshold: u32) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("already initialized");
        }
        if default_threshold > MAX_SCORE {
            panic!("threshold must be between 0 and 100");
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::RiskThreshold, &default_threshold);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_BUMP);
    }

    /// Transfer administration in one operation accepted by both addresses.
    pub fn transfer_admin(env: Env, current_admin: Address, new_admin: Address) {
        current_admin.require_auth();
        require_admin(&env, &current_admin);
        new_admin.require_auth();
        if current_admin == new_admin {
            panic!("new admin must differ from current admin");
        }
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        bump_instance_ttl(&env);
    }

    /// Admin-only: authorize an address to act as a monitoring agent.
    /// TODO(#issue): role separation between "monitor" and "responder" agents
    /// is not implemented yet — every authorized agent currently has full
    /// flagging rights. See CONTRIBUTING for the open issue.
    pub fn authorize_agent(env: Env, admin: Address, agent: Address) {
        admin.require_auth();
        require_admin(&env, &admin);
        env.storage()
            .instance()
            .set(&DataKey::Agent(agent.clone()), &true);
        add_agent_to_registry(&env, &agent);
        env.storage().instance().set(&DataKey::Agent(agent.clone()), &true);
        bump_instance_ttl(&env);
        env.events().publish((AGENT_ADD_EVENT, admin, agent), true);
    }

    /// Admin-only: revoke an agent's ability to submit risk flags.
    pub fn revoke_agent(env: Env, admin: Address, agent: Address) {
        admin.require_auth();
        require_admin(&env, &admin);
        env.storage()
            .instance()
            .set(&DataKey::Agent(agent.clone()), &false);
        remove_agent_from_registry(&env, &agent);
        bump_instance_ttl(&env);
        env.events().publish((AGENT_DEL_EVENT, admin, agent), false);
    }

    /// Return the active agent registry to the administrator.
    pub fn get_agents(env: Env, admin: Address) -> Vec<Address> {
        admin.require_auth();
        require_admin(&env, &admin);
        let agents = env
            .storage()
            .instance()
            .get(&DataKey::AgentRegistry)
            .unwrap_or(Vec::new(&env));
        bump_instance_ttl(&env);
        agents
    }

    /// Admin-only: update the accepted risk score threshold (0 through 100).
    pub fn set_threshold(env: Env, admin: Address, threshold: u32) {
        admin.require_auth();
        require_admin(&env, &admin);
        if threshold > MAX_SCORE {
            panic!("threshold must be between 0 and 100");
        }
        let previous: u32 = env
            .storage()
            .instance()
            .get(&DataKey::RiskThreshold)
            .expect("not initialized");
        env.storage()
            .instance()
            .set(&DataKey::RiskThreshold, &threshold);
        bump_instance_ttl(&env);
        env.events()
            .publish((THRESHOLD_EVENT, admin), (previous, threshold));
    }

    pub fn is_agent(env: Env, agent: Address) -> bool {
        let authorized = env.storage()
            .instance()
            .get(&DataKey::Agent(agent))
            .unwrap_or(false);
        bump_instance_ttl(&env);
        authorized
    }

    /// Called by an authorized agent when it flags a transaction/address as
    /// anomalous. Scores below the configured threshold are rejected. Emits
    /// the stable `flagged` event and records the latest flag for the subject.
    pub fn flag_anomaly(env: Env, agent: Address, subject: Address, score: u32) {
        agent.require_auth();
        let is_agent: bool = env
            .storage()
            .instance()
            .get(&DataKey::Agent(agent.clone()))
            .unwrap_or(false);
        if !is_agent {
            panic!("not an authorized agent");
        }
        if score > MAX_SCORE {
            panic!("score must be between 0 and 100");
        }
        let threshold: u32 = env
            .storage()
            .instance()
            .get(&DataKey::RiskThreshold)
            .expect("not initialized");
        if score < threshold {
            panic!("score below risk threshold");
        }
        let key = DataKey::LatestFlag(subject.clone());
        let record = FlagRecord {
            agent: agent.clone(),
            score,
            ledger: env.ledger().sequence(),
            timestamp: env.ledger().timestamp(),
        };
        env.storage().persistent().set(&key, &record);
        env.storage()
            .persistent()
            .extend_ttl(&key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_BUMP);
        bump_instance_ttl(&env);
        env.events().publish((FLAG_EVENT, agent, subject), score);
    }

    /// Submit up to MAX_FLAG_BATCH risk flags in one authorized transaction.
    pub fn flag_anomalies(env: Env, agent: Address, submissions: Vec<FlagSubmission>) {
        agent.require_auth();
        if submissions.len() == 0 || submissions.len() > MAX_FLAG_BATCH {
            panic!("flag batch size must be between 1 and 16");
        }
        let is_agent: bool = env
            .storage()
            .instance()
            .get(&DataKey::Agent(agent.clone()))
            .unwrap_or(false);
        if !is_agent {
            panic!("not an authorized agent");
        }
        let threshold: u32 = env
            .storage()
            .instance()
            .get(&DataKey::RiskThreshold)
            .expect("not initialized");
        for submission in submissions.clone() {
            if submission.score > MAX_SCORE {
                panic!("score must be between 0 and 100");
            }
            if submission.score < threshold {
                panic!("score below risk threshold");
            }
        }
        for submission in submissions {
            let key = DataKey::LatestFlag(submission.subject.clone());
            let record = FlagRecord {
                agent: agent.clone(),
                score: submission.score,
                ledger: env.ledger().sequence(),
                timestamp: env.ledger().timestamp(),
            };
            env.storage().persistent().set(&key, &record);
            env.storage().persistent().extend_ttl(
                &key,
                PERSISTENT_TTL_THRESHOLD,
                PERSISTENT_TTL_BUMP,
            );
            env.events().publish(
                (FLAG_EVENT, agent.clone(), submission.subject),
                submission.score,
            );
        }
        bump_instance_ttl(&env);
    }

    /// Return the latest recorded flag for a subject, if one exists.
    /// Persistent entries are extended when read, subject to the network's
    /// maximum TTL policy.
    pub fn get_latest_flag(env: Env, subject: Address) -> Option<FlagRecord> {
        let key = DataKey::LatestFlag(subject);
        let record = env.storage().persistent().get(&key);
        if record.is_some() {
            env.storage().persistent().extend_ttl(
                &key,
                PERSISTENT_TTL_THRESHOLD,
                PERSISTENT_TTL_BUMP,
            );
        }
        record
    }

    pub fn get_threshold(env: Env) -> u32 {
        let threshold = env
            .storage()
            .instance()
            .get(&DataKey::RiskThreshold)
            .unwrap_or(0);
        bump_instance_ttl(&env);
        threshold
    }
}

fn add_agent_to_registry(env: &Env, agent: &Address) {
    let mut agents: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::AgentRegistry)
        .unwrap_or(Vec::new(env));
    if agents.contains(agent) {
        return;
    }
    if agents.len() >= MAX_AUTHORIZED_AGENTS {
        panic!("agent registry is full");
    }
    agents.push_back(agent.clone());
    env.storage()
        .instance()
        .set(&DataKey::AgentRegistry, &agents);
}

fn remove_agent_from_registry(env: &Env, agent: &Address) {
    let agents: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::AgentRegistry)
        .unwrap_or(Vec::new(env));
    let mut remaining = Vec::new(env);
    for registered in agents.iter() {
        if registered != agent.clone() {
            remaining.push_back(registered);
        }
    }
    env.storage()
        .instance()
        .set(&DataKey::AgentRegistry, &remaining);
}

fn require_admin(env: &Env, admin: &Address) {
    let stored_admin: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .expect("not initialized");
    if stored_admin != *admin {
        panic!("unauthorized");
    }
}

fn bump_instance_ttl(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_BUMP);
}

mod test;
