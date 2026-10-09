#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env, Symbol};

// Storage keys
#[contracttype]
pub enum DataKey {
    Admin,
    Agent(Address),
    RiskThreshold,
    LatestFlag(Address),
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

const FLAG_EVENT: Symbol = symbol_short!("flagged");
const MAX_SCORE: u32 = 100;
const INSTANCE_TTL_THRESHOLD: u32 = 10_000;
const INSTANCE_TTL_BUMP: u32 = 100_000;
const PERSISTENT_TTL_THRESHOLD: u32 = 10_000;
const PERSISTENT_TTL_BUMP: u32 = 100_000;

#[contract]
pub struct StellarSentinel;

#[contractimpl]
impl StellarSentinel {
    /// Return whether this contract instance has completed initialization.
    pub fn is_initialized(env: Env) -> bool {
        let initialized = env.storage().instance().has(&DataKey::Admin);
        if initialized {
            bump_instance_ttl(&env);
        }
        initialized
    }

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

    /// Admin-only: authorize an address to act as a monitoring agent.
    /// TODO(#issue): role separation between "monitor" and "responder" agents
    /// is not implemented yet — every authorized agent currently has full
    /// flagging rights. See CONTRIBUTING for the open issue.
    pub fn authorize_agent(env: Env, admin: Address, agent: Address) {
        admin.require_auth();
        require_admin(&env, &admin);
        env.storage().instance().set(&DataKey::Agent(agent), &true);
        bump_instance_ttl(&env);
    }

    /// Admin-only: revoke an agent's ability to submit risk flags.
    pub fn revoke_agent(env: Env, admin: Address, agent: Address) {
        admin.require_auth();
        require_admin(&env, &admin);
        env.storage()
            .instance()
            .set(&DataKey::Agent(agent), &false);
        bump_instance_ttl(&env);
    }

    /// Admin-only: update the accepted risk score threshold (0 through 100).
    pub fn set_threshold(env: Env, admin: Address, threshold: u32) {
        admin.require_auth();
        require_admin(&env, &admin);
        if threshold > MAX_SCORE {
            panic!("threshold must be between 0 and 100");
        }
        env.storage()
            .instance()
            .set(&DataKey::RiskThreshold, &threshold);
        bump_instance_ttl(&env);
    }

    pub fn is_agent(env: Env, agent: Address) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Agent(agent))
            .unwrap_or(false)
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
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_TTL_THRESHOLD,
            PERSISTENT_TTL_BUMP,
        );
        bump_instance_ttl(&env);
        env.events()
            .publish((FLAG_EVENT, agent, subject), score);
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
        let threshold = env.storage()
            .instance()
            .get(&DataKey::RiskThreshold)
            .unwrap_or(0);
        bump_instance_ttl(&env);
        threshold
    }
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
