#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, BytesN, Env, Symbol, Vec};

// Storage keys
#[contracttype]
pub enum DataKey {
    Admin,
    Agent(Address),
    RiskThreshold,
    Paused,
    LatestFlag(Address),
    // Append registry storage to preserve existing storage-key encoding.
    AgentRegistry,
    AgentCount,
    AgentCapacity,
    MonitorAgent(Address),
    ResponderAgent(Address),
    AgentExpiry(Address),
    StorageVersion,
    EmergencyGuardian,
    GuardianPaused,
    QuotaWindow,
    QuotaLimit,
    AgentQuota(Address),
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

/// Administrator and threshold values observed in one contract read.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractConfig {
    pub admin: Address,
    pub threshold: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CircuitBreakerState {
    pub paused: bool,
    pub guardian_paused: bool,
}

/// Bounded per-agent submission usage within a ledger window.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuotaState {
    pub window_start: u32,
    pub used: u32,
}

/// Bounded read-only snapshot of non-sensitive operator configuration.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigurationSnapshot {
    pub interface_version: u32,
    pub initialized: bool,
    pub admin: Option<Address>,
    pub threshold: Option<u32>,
    pub paused: bool,
    pub guardian_paused: bool,
    pub emergency_guardian: Option<Address>,
    pub agent_count: u32,
    pub agent_capacity: Option<u32>,
    pub storage_version: Option<u32>,
}

/// Bounded, non-sensitive configuration metadata for operators.
///
/// Optional fields distinguish an uninitialized or older instance from a
/// configured value. The view intentionally reports the active-agent count,
/// not the registry addresses; use the administrator-authenticated
/// `get_agents` entry point when the addresses are required.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigurationView {
    pub interface_version: u32,
    pub initialized: bool,
    pub admin: Option<Address>,
    pub threshold: Option<u32>,
    pub paused: bool,
    pub guardian_paused: bool,
    pub emergency_guardian: Option<Address>,
    pub agent_count: u32,
    pub agent_capacity: Option<u32>,
    pub storage_version: Option<u32>,
}

const FLAG_EVENT: Symbol = symbol_short!("flagged");
const FLAGGED_V2_EVENT: Symbol = symbol_short!("flaggedv2");
const FLAGGED_V3_EVENT: Symbol = symbol_short!("flaggedv3");
const AGENT_ADD_EVENT: Symbol = symbol_short!("agent_add");
const AGENT_DEL_EVENT: Symbol = symbol_short!("agent_del");
const THRESHOLD_EVENT: Symbol = symbol_short!("threshold");
const INIT_EVENT: Symbol = symbol_short!("init");
const PAUSE_EVENT: Symbol = symbol_short!("pause");
const GUARDIAN_PAUSE_EVENT: Symbol = symbol_short!("gpause");
const GUARDIAN_RESUME_EVENT: Symbol = symbol_short!("gunpause");
const QUOTA_EVENT: Symbol = symbol_short!("quota");
const MAX_SCORE: u32 = 100;
const MAX_POLICY_ID: u32 = 1_000_000;
const MAX_REASON_MASK: u32 = 0xFFFF;
const INTERFACE_VERSION: u32 = 1;
const MAX_FLAG_BATCH: u32 = 16;
const INSTANCE_TTL_THRESHOLD: u32 = 10_000;
const INSTANCE_TTL_BUMP: u32 = 100_000;
const PERSISTENT_TTL_THRESHOLD: u32 = 10_000;
const PERSISTENT_TTL_BUMP: u32 = 100_000;
const MAX_AGENT_BATCH: u32 = 16;
const HARD_AGENT_CAPACITY: u32 = 128;
const MAX_AGENT_GRANT_LEDGERS: u32 = 100_000;
const STORAGE_VERSION: u32 = 1;
const DEFAULT_QUOTA_WINDOW: u32 = 100;
const DEFAULT_QUOTA_LIMIT: u32 = 16;
const MAX_QUOTA_WINDOW: u32 = 100_000;
const MAX_QUOTA_LIMIT: u32 = 16;

#[contract]
pub struct StellarSentinel;

#[contractimpl]
impl StellarSentinel {
    /// Return the external contract interface generation.
    pub fn get_interface_version() -> u32 {
        INTERFACE_VERSION
    }

    /// Return whether this contract instance has completed initialization.
    pub fn is_initialized(env: Env) -> bool {
        let initialized = env.storage().instance().has(&DataKey::Admin);
        if initialized { bump_instance_ttl(&env); }
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
        env.storage().instance().set(&DataKey::Paused, &false);
        env.storage().instance().set(&DataKey::GuardianPaused, &false);
        env.storage().instance().set(&DataKey::AgentCount, &0_u32);
        env.storage()
            .instance()
            .set(&DataKey::AgentCapacity, &HARD_AGENT_CAPACITY);
        env.storage()
            .instance()
            .set(&DataKey::StorageVersion, &STORAGE_VERSION);
        env.storage()
            .instance()
            .set(&DataKey::QuotaWindow, &DEFAULT_QUOTA_WINDOW);
        env.storage()
            .instance()
            .set(&DataKey::QuotaLimit, &DEFAULT_QUOTA_LIMIT);
        env.events()
            .publish((INIT_EVENT, admin.clone()), default_threshold);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_BUMP);
    }

    /// Return the configured contract administrator.
    pub fn get_admin(env: Env) -> Address {
        let admin = env.storage().instance().get(&DataKey::Admin).expect("not initialized");
        bump_instance_ttl(&env);
        admin
    }

    /// Return the administrator and threshold from one instance snapshot.
    pub fn get_contract_config(env: Env) -> Option<ContractConfig> {
        let admin: Option<Address> = env.storage().instance().get(&DataKey::Admin);
        let threshold: Option<u32> = env.storage().instance().get(&DataKey::RiskThreshold);
        match (admin, threshold) {
            (Some(admin), Some(threshold)) => {
                bump_instance_ttl(&env);
                Some(ContractConfig { admin, threshold })
            }
            _ => None,
        }
    }

    /// Return the public configuration snapshot without renewing TTLs.
    ///
    /// Agent identities remain available only through the administrator-authenticated
    /// `get_agents` method; this aggregate view exposes the active count only.
    pub fn get_configuration_snapshot(env: Env) -> ConfigurationSnapshot {
        let admin: Option<Address> = env.storage().instance().get(&DataKey::Admin);
        ConfigurationSnapshot {
            interface_version: INTERFACE_VERSION,
            initialized: admin.is_some(),
            admin,
            threshold: env.storage().instance().get(&DataKey::RiskThreshold),
            paused: env.storage().instance().get(&DataKey::Paused).unwrap_or(false),
            guardian_paused: env.storage().instance().get(&DataKey::GuardianPaused).unwrap_or(false),
            emergency_guardian: env.storage().instance().get(&DataKey::EmergencyGuardian),
            agent_count: active_agent_count_read_only(&env),
            agent_capacity: env.storage().instance().get(&DataKey::AgentCapacity),
            storage_version: env.storage().instance().get(&DataKey::StorageVersion),
        }
    }

    /// Return bounded, non-sensitive configuration metadata in one read.
    ///
    /// This view is public and does not emit events or rewrite configuration
    /// storage. It refreshes only the instance TTL for initialized instances.
    pub fn get_configuration_view(env: Env) -> ConfigurationView {
        let admin: Option<Address> = env.storage().instance().get(&DataKey::Admin);
        let initialized = admin.is_some();
        let view = ConfigurationView {
            interface_version: INTERFACE_VERSION,
            initialized,
            admin,
            threshold: env.storage().instance().get(&DataKey::RiskThreshold),
            paused: env.storage().instance().get(&DataKey::Paused).unwrap_or(false),
            guardian_paused: env.storage().instance().get(&DataKey::GuardianPaused).unwrap_or(false),
            emergency_guardian: env.storage().instance().get(&DataKey::EmergencyGuardian),
            agent_count: active_agent_count_read_only(&env),
            agent_capacity: env.storage().instance().get(&DataKey::AgentCapacity),
            storage_version: env.storage().instance().get(&DataKey::StorageVersion),
        };
        if initialized {
            bump_instance_ttl(&env);
        }
        view
    }

    pub fn get_storage_version(env: Env) -> u32 {
        let version: u32 = env.storage().instance().get(&DataKey::StorageVersion).unwrap_or(0);
        if version > 0 {
            bump_instance_ttl(&env);
        }
        version
    }

    /// Backfill the schema version for an existing initialized instance.
    pub fn migrate_storage_version(env: Env, admin: Address) {
        admin.require_auth();
        require_admin(&env, &admin);
        match env.storage().instance().get::<_, u32>(&DataKey::StorageVersion) {
            None => env.storage().instance().set(&DataKey::StorageVersion, &STORAGE_VERSION),
            Some(STORAGE_VERSION) => {
                bump_instance_ttl(&env);
                return;
            }
            Some(_) => panic!("unsupported storage version"),
        }
        bump_instance_ttl(&env);
    }

    /// Transfer administration only after both current and proposed admins approve.
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

    /// Legacy authorization alias: grants responder access without monitor access.
    pub fn authorize_agent(env: Env, admin: Address, agent: Address) {
        admin.require_auth();
        require_admin(&env, &admin);
        add_agent_to_registry(&env, &agent);
        env.storage().instance().set(&DataKey::Agent(agent.clone()), &true);
        env.storage().instance().set(&DataKey::MonitorAgent(agent.clone()), &false);
        env.storage().instance().set(&DataKey::ResponderAgent(agent.clone()), &true);
        env.storage().instance().remove(&DataKey::AgentExpiry(agent.clone()));
        bump_instance_ttl(&env);
        env.events().publish((AGENT_ADD_EVENT, admin, agent), true);
    }

    pub fn authorize_agent_until(
        env: Env,
        admin: Address,
        agent: Address,
        expires_at_ledger: u32,
    ) {
        admin.require_auth();
        require_admin(&env, &admin);
        let current_ledger = env.ledger().sequence();
        if expires_at_ledger <= current_ledger
            || expires_at_ledger - current_ledger > MAX_AGENT_GRANT_LEDGERS
        {
            panic!("agent grant expiry must be within the next 100000 ledgers");
        }
        add_agent_to_registry(&env, &agent);
        env.storage().instance().set(&DataKey::Agent(agent.clone()), &true);
        env.storage().instance().set(&DataKey::MonitorAgent(agent.clone()), &false);
        env.storage().instance().set(&DataKey::ResponderAgent(agent.clone()), &true);
        env.storage()
            .instance()
            .set(&DataKey::AgentExpiry(agent), &expires_at_ledger);
        bump_instance_ttl(&env);
    }

    pub fn authorize_monitor(env: Env, admin: Address, agent: Address) {
        admin.require_auth();
        require_admin(&env, &admin);
        add_agent_to_registry(&env, &agent);
        env.storage().instance().set(&DataKey::Agent(agent.clone()), &true);
        env.storage().instance().set(&DataKey::MonitorAgent(agent.clone()), &true);
        if !env.storage().instance().has(&DataKey::ResponderAgent(agent.clone())) {
            env.storage().instance().set(&DataKey::ResponderAgent(agent), &false);
        }
        bump_instance_ttl(&env);
    }

    pub fn authorize_responder(env: Env, admin: Address, agent: Address) {
        admin.require_auth();
        require_admin(&env, &admin);
        add_agent_to_registry(&env, &agent);
        env.storage().instance().set(&DataKey::Agent(agent.clone()), &true);
        env.storage().instance().set(&DataKey::ResponderAgent(agent.clone()), &true);
        if !env.storage().instance().has(&DataKey::MonitorAgent(agent.clone())) {
            env.storage().instance().set(&DataKey::MonitorAgent(agent.clone()), &false);
        }
        env.storage().instance().remove(&DataKey::AgentExpiry(agent));
        bump_instance_ttl(&env);
    }

    pub fn revoke_monitor(env: Env, admin: Address, agent: Address) {
        admin.require_auth();
        require_admin(&env, &admin);
        env.storage().instance().set(&DataKey::MonitorAgent(agent.clone()), &false);
        if !has_responder_role(&env, &agent) {
            env.storage().instance().set(&DataKey::Agent(agent.clone()), &false);
            remove_agent_from_registry(&env, &agent);
        }
        bump_instance_ttl(&env);
    }

    pub fn revoke_responder(env: Env, admin: Address, agent: Address) {
        admin.require_auth();
        require_admin(&env, &admin);
        env.storage().instance().set(&DataKey::ResponderAgent(agent.clone()), &false);
        if !has_monitor_role(&env, &agent) {
            env.storage().instance().set(&DataKey::Agent(agent.clone()), &false);
            env.storage().instance().remove(&DataKey::AgentExpiry(agent.clone()));
            remove_agent_from_registry(&env, &agent);
        }
        bump_instance_ttl(&env);
    }

    pub fn set_agent_capacity(env: Env, admin: Address, capacity: u32) {
        admin.require_auth();
        require_admin(&env, &admin);
        let count = active_agent_count(&env);
        if capacity > HARD_AGENT_CAPACITY || capacity < count {
            panic!("invalid agent capacity");
        }
        env.storage().instance().set(&DataKey::AgentCapacity, &capacity);
        bump_instance_ttl(&env);
    }

    pub fn migrate_agent_capacity(env: Env, admin: Address, active_count: u32, capacity: u32) {
        admin.require_auth();
        require_admin(&env, &admin);
        if env.storage().instance().has(&DataKey::AgentCount)
            || env.storage().instance().has(&DataKey::AgentCapacity)
            || active_count > HARD_AGENT_CAPACITY
            || capacity > HARD_AGENT_CAPACITY
            || capacity < active_count
        {
            panic!("invalid migrated agent capacity");
        }
        env.storage().instance().set(&DataKey::AgentCount, &active_count);
        env.storage().instance().set(&DataKey::AgentCapacity, &capacity);
        bump_instance_ttl(&env);
    }

    pub fn get_agent_capacity(env: Env) -> u32 {
        let capacity = agent_capacity(&env);
        bump_instance_ttl(&env);
        capacity
    }

    pub fn get_agent_count(env: Env) -> u32 {
        let count = active_agent_count(&env);
        bump_instance_ttl(&env);
        count
    }

    /// Admin-only: authorize up to MAX_AGENT_BATCH addresses in one call.
    /// Repeated addresses are idempotent, matching authorize_agent.
    pub fn authorize_agents(env: Env, admin: Address, agents: Vec<Address>) {
        admin.require_auth(); require_admin(&env, &admin);
        if agents.len() > MAX_AGENT_BATCH { panic!("agent batch exceeds maximum"); }
        for agent in agents {
            if !agent_is_authorized(&env, &agent) {
                add_agent_to_registry(&env, &agent);
                env.storage().instance().set(&DataKey::Agent(agent.clone()), &true);
                env.storage().instance().set(&DataKey::MonitorAgent(agent.clone()), &false);
                env.storage().instance().set(&DataKey::ResponderAgent(agent.clone()), &true);
                env.storage().instance().remove(&DataKey::AgentExpiry(agent));
            }
        }
        bump_instance_ttl(&env);
    }

    /// Admin-only: revoke an agent's ability to submit risk flags.
    pub fn revoke_agent(env: Env, admin: Address, agent: Address) {
        admin.require_auth();
        require_admin(&env, &admin);
        revoke_all_agent_roles(&env, &agent);
        bump_instance_ttl(&env);
        env.events().publish((AGENT_DEL_EVENT, admin, agent), false);
    }

    /// Admin-only: revoke up to MAX_AGENT_BATCH addresses in one call.
    /// Repeated or already-revoked addresses are idempotent.
    pub fn revoke_agents(env: Env, admin: Address, agents: Vec<Address>) {
        admin.require_auth(); require_admin(&env, &admin);
        if agents.len() > MAX_AGENT_BATCH { panic!("agent batch exceeds maximum"); }
        for agent in agents {
            revoke_all_agent_roles(&env, &agent);
        }
        bump_instance_ttl(&env);
    }

    pub fn revoke_self(env: Env, agent: Address) {
        agent.require_auth();
        revoke_all_agent_roles(&env, &agent);
        bump_instance_ttl(&env);
    }

    /// Return the active agent registry to the administrator.
    pub fn get_agents(env: Env, admin: Address) -> Vec<Address> {
        admin.require_auth();
        require_admin(&env, &admin);
        active_agent_count(&env);
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
        let current: u32 = env.storage().instance().get(&DataKey::RiskThreshold).expect("not initialized");
        if current == threshold { bump_instance_ttl(&env); return; }
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

    /// Configure the bounded per-agent submission quota.
    pub fn set_submission_quota(env: Env, admin: Address, window_ledgers: u32, max_flags: u32) {
        admin.require_auth();
        require_admin(&env, &admin);
        validate_quota_config(window_ledgers, max_flags);
        env.storage().instance().set(&DataKey::QuotaWindow, &window_ledgers);
        env.storage().instance().set(&DataKey::QuotaLimit, &max_flags);
        bump_instance_ttl(&env);
        env.events().publish((QUOTA_EVENT, admin), (window_ledgers, max_flags));
    }

    pub fn get_submission_quota(env: Env) -> (u32, u32) {
        let (window, limit) = configured_quota(&env);
        bump_instance_ttl(&env);
        (window, limit)
    }

    pub fn get_agent_quota(env: Env, agent: Address) -> QuotaState {
        let (window, _) = configured_quota(&env);
        let current = env.ledger().sequence();
        let state = env
            .storage()
            .persistent()
            .get::<_, QuotaState>(&DataKey::AgentQuota(agent))
            .unwrap_or(QuotaState { window_start: current, used: 0 });
        if current.saturating_sub(state.window_start) >= window {
            QuotaState { window_start: current, used: 0 }
        } else {
            state
        }
    }

    pub fn set_paused(env: Env, admin: Address, paused: bool) {
        admin.require_auth();
        require_admin(&env, &admin);
        set_paused_state(&env, &admin, paused);
    }

    /// Admin-only: stop agent flag submissions.
    pub fn pause(env: Env, admin: Address) {
        admin.require_auth();
        require_admin(&env, &admin);
        set_paused_state(&env, &admin, true);
    }

    /// Admin-only: resume agent flag submissions.
    pub fn unpause(env: Env, admin: Address) {
        admin.require_auth();
        require_admin(&env, &admin);
        set_paused_state(&env, &admin, false);
    }

    /// Report whether new flag submissions are currently paused.
    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    pub fn set_emergency_guardian(env: Env, admin: Address, guardian: Address) {
        admin.require_auth();
        require_admin(&env, &admin);
        env.storage()
            .instance()
            .set(&DataKey::EmergencyGuardian, &guardian);
        bump_instance_ttl(&env);
    }

    pub fn guardian_pause(env: Env, guardian: Address) {
        guardian.require_auth();
        let configured: Address = env
            .storage()
            .instance()
            .get(&DataKey::EmergencyGuardian)
            .expect("emergency guardian is not configured");
        if configured != guardian {
            panic!("not emergency guardian");
        }
        let paused: bool = env
            .storage()
            .instance()
            .get(&DataKey::GuardianPaused)
            .unwrap_or(false);
        if !paused {
            env.storage().instance().set(&DataKey::GuardianPaused, &true);
            env.events().publish((GUARDIAN_PAUSE_EVENT, guardian), true);
        }
        bump_instance_ttl(&env);
    }

    pub fn resume_from_guardian_pause(env: Env, admin: Address) {
        admin.require_auth();
        require_admin(&env, &admin);
        let paused: bool = env
            .storage()
            .instance()
            .get(&DataKey::GuardianPaused)
            .unwrap_or(false);
        if paused {
            env.storage().instance().set(&DataKey::GuardianPaused, &false);
            env.events().publish((GUARDIAN_RESUME_EVENT, admin), false);
        }
        bump_instance_ttl(&env);
    }

    pub fn is_guardian_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::GuardianPaused)
            .unwrap_or(false)
    }

    /// Return both circuit-breaker latches from one consistent read.
    pub fn get_circuit_breaker_state(env: Env) -> CircuitBreakerState {
        let paused = env.storage().instance().get(&DataKey::Paused).unwrap_or(false);
        let guardian_paused = env
            .storage()
            .instance()
            .get(&DataKey::GuardianPaused)
            .unwrap_or(false);
        CircuitBreakerState { paused, guardian_paused }
    }

    pub fn is_agent(env: Env, agent: Address) -> bool {
        let authorized = has_monitor_role(&env, &agent) || has_responder_role(&env, &agent);
        bump_instance_ttl(&env);
        authorized
    }

    pub fn is_monitor(env: Env, agent: Address) -> bool {
        has_monitor_role(&env, &agent)
    }

    pub fn is_responder(env: Env, agent: Address) -> bool {
        has_responder_role(&env, &agent)
    }

    /// Called by an authorized agent when it flags a transaction/address as
    /// anomalous. Scores below the configured threshold are rejected. Emits
    /// the stable `flagged` event and records the latest flag for the subject.
    pub fn flag_anomaly(env: Env, agent: Address, subject: Address, score: u32) {
        require_submissions_unpaused(&env);
        require_responder(&env, &agent);
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
        consume_agent_quota(&env, &agent, 1);
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

    /// Submit a flag linked to an off-chain report digest without storing report contents.
    pub fn flag_anomaly_v2(env: Env, agent: Address, subject: Address, score: u32, report_digest: BytesN<32>) {
        require_submissions_unpaused(&env);
        require_responder(&env, &agent);
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
        consume_agent_quota(&env, &agent, 1);
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
        env.events().publish((FLAGGED_V2_EVENT, agent, subject, report_digest), score);
    }

    /// Submit a policy-aware flag without storing raw report data.
    ///
    /// The versioned event keeps the legacy `flagged` and `flaggedv2` schemas
    /// unchanged while exposing bounded policy and reason metadata to indexers.
    pub fn flag_anomaly_v3(
        env: Env,
        agent: Address,
        subject: Address,
        score: u32,
        policy_id: u32,
        reason_mask: u32,
    ) {
        require_submissions_unpaused(&env);
        agent.require_auth();
        if !has_responder_role(&env, &agent) {
            panic!("not an authorized agent");
        }
        if score > MAX_SCORE {
            panic!("score must be between 0 and 100");
        }
        if policy_id == 0 || policy_id > MAX_POLICY_ID {
            panic!("policy id must be between 1 and 1000000");
        }
        if reason_mask == 0 || reason_mask > MAX_REASON_MASK {
            panic!("reason mask must use one of the lower 16 bits");
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
        env.events().publish(
            (FLAGGED_V3_EVENT, agent, subject, policy_id),
            (score, reason_mask),
        );
    }

    /// Submit up to MAX_FLAG_BATCH risk flags in one authorized transaction.
    pub fn flag_anomalies(env: Env, agent: Address, submissions: Vec<FlagSubmission>) {
        require_submissions_unpaused(&env);
        require_responder(&env, &agent);
        if submissions.is_empty() || submissions.len() > MAX_FLAG_BATCH {
            panic!("flag batch size must be between 1 and 16");
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
        consume_agent_quota(&env, &agent, submissions.len());
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

    /// Return the latest recorded flag and refresh its persistent retention TTL.
    ///
    /// Persistent entries remain restorable after archival, but an archived
    /// entry must be restored in the transaction footprint before this method
    /// can read it. Reads while the entry is live keep frequently queried
    /// security state from reaching archival in the first place.
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

    pub fn clear_latest_flag(env: Env, admin: Address, subject: Address) {
        admin.require_auth();
        require_admin(&env, &admin);
        env.storage().persistent().remove(&DataKey::LatestFlag(subject));
        bump_instance_ttl(&env);
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

    /// Report whether a score is within the contract range and meets policy.
    /// This query is advisory; flag_anomaly repeats the checks on-chain.
    pub fn is_score_accepted(env: Env, score: u32) -> bool {
        let threshold: Option<u32> = env
            .storage()
            .instance()
            .get(&DataKey::RiskThreshold);
        match threshold {
            Some(threshold) => {
                bump_instance_ttl(&env);
                score <= MAX_SCORE && score >= threshold
            }
            None => false,
        }
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
    let count = active_agent_count(env);
    if count >= agent_capacity(env) || count >= HARD_AGENT_CAPACITY {
        panic!("agent capacity reached");
    }
    agents.push_back(agent.clone());
    env.storage()
        .instance()
        .set(&DataKey::AgentRegistry, &agents);
    env.storage()
        .instance()
        .set(&DataKey::AgentCount, &(count + 1));
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
    env.storage()
        .instance()
        .set(&DataKey::AgentCount, &remaining.len());
}

fn active_agent_count(env: &Env) -> u32 {
    let agents: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::AgentRegistry)
        .unwrap_or(Vec::new(env));
    let mut active = Vec::new(env);
    for agent in agents.iter() {
        let authorized: bool = env
            .storage()
            .instance()
            .get(&DataKey::Agent(agent.clone()))
            .unwrap_or(false);
        let expiry: Option<u32> = env
            .storage()
            .instance()
            .get(&DataKey::AgentExpiry(agent.clone()));
        if authorized && expiry.map(|ledger| env.ledger().sequence() < ledger).unwrap_or(true) {
            active.push_back(agent);
        }
    }
    let count = active.len();
    env.storage().instance().set(&DataKey::AgentRegistry, &active);
    env.storage().instance().set(&DataKey::AgentCount, &count);
    count
}

fn active_agent_count_read_only(env: &Env) -> u32 {
    let agents: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::AgentRegistry)
        .unwrap_or(Vec::new(env));
    let mut active = 0_u32;
    for agent in agents.iter() {
        let authorized: bool = env
            .storage()
            .instance()
            .get(&DataKey::Agent(agent.clone()))
            .unwrap_or(false);
        let expiry: Option<u32> = env
            .storage()
            .instance()
            .get(&DataKey::AgentExpiry(agent.clone()));
        if authorized && expiry.map(|ledger| env.ledger().sequence() < ledger).unwrap_or(true) {
            active = active.checked_add(1).expect("agent count overflow");
        }
    }
    active
}

fn agent_capacity(env: &Env) -> u32 {
    if let Some(capacity) = env.storage().instance().get(&DataKey::AgentCapacity) {
        return capacity;
    }
    env.storage()
        .instance()
        .set(&DataKey::AgentCapacity, &HARD_AGENT_CAPACITY);
    HARD_AGENT_CAPACITY
}

fn agent_is_authorized(env: &Env, agent: &Address) -> bool {
    let authorized: bool = env
        .storage()
        .instance()
        .get(&DataKey::Agent(agent.clone()))
        .unwrap_or(false);
    if !authorized {
        return false;
    }
    let expiry: Option<u32> = env
        .storage()
        .instance()
        .get(&DataKey::AgentExpiry(agent.clone()));
    expiry
        .map(|expires_at| env.ledger().sequence() < expires_at)
        .unwrap_or(true)
}

fn has_monitor_role(env: &Env, agent: &Address) -> bool {
    if !agent_is_authorized(env, agent) {
        return false;
    }
    env.storage()
        .instance()
        .get(&DataKey::MonitorAgent(agent.clone()))
        .unwrap_or(true)
}

fn has_responder_role(env: &Env, agent: &Address) -> bool {
    if !agent_is_authorized(env, agent) {
        return false;
    }
    env.storage()
        .instance()
        .get(&DataKey::ResponderAgent(agent.clone()))
        .unwrap_or(true)
}

fn revoke_all_agent_roles(env: &Env, agent: &Address) {
    env.storage()
        .instance()
        .set(&DataKey::Agent(agent.clone()), &false);
    env.storage()
        .instance()
        .set(&DataKey::MonitorAgent(agent.clone()), &false);
    env.storage()
        .instance()
        .set(&DataKey::ResponderAgent(agent.clone()), &false);
    env.storage()
        .instance()
        .remove(&DataKey::AgentExpiry(agent.clone()));
    remove_agent_from_registry(env, agent);
}

fn set_paused_state(env: &Env, admin: &Address, paused: bool) {
    let current: bool = env
        .storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false);
    if current != paused {
        env.storage().instance().set(&DataKey::Paused, &paused);
        env.events().publish((PAUSE_EVENT, admin.clone()), paused);
    }
    bump_instance_ttl(env);
}

fn require_submissions_unpaused(env: &Env) {
    if env
        .storage()
        .instance()
        .get::<_, bool>(&DataKey::Paused)
        .unwrap_or(false)
    {
        panic!("contract is paused");
    }
    if env
        .storage()
        .instance()
        .get::<_, bool>(&DataKey::GuardianPaused)
        .unwrap_or(false)
    {
        panic!("emergency guardian paused contract");
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

fn require_responder(env: &Env, agent: &Address) {
    agent.require_auth();
    if !has_responder_role(env, agent) {
        panic!("not an authorized agent");
    }
}

fn validate_quota_config(window: u32, limit: u32) {
    if window == 0 || window > MAX_QUOTA_WINDOW || limit == 0 || limit > MAX_QUOTA_LIMIT {
        panic!("invalid quota configuration");
    }
}

fn configured_quota(env: &Env) -> (u32, u32) {
    let window = env.storage().instance().get(&DataKey::QuotaWindow).unwrap_or(DEFAULT_QUOTA_WINDOW);
    let limit = env.storage().instance().get(&DataKey::QuotaLimit).unwrap_or(DEFAULT_QUOTA_LIMIT);
    validate_quota_config(window, limit);
    (window, limit)
}

fn consume_agent_quota(env: &Env, agent: &Address, amount: u32) {
    let (window, limit) = configured_quota(env);
    let key = DataKey::AgentQuota(agent.clone());
    let current = env.ledger().sequence();
    let mut state = env
        .storage()
        .persistent()
        .get::<_, QuotaState>(&key)
        .unwrap_or(QuotaState { window_start: current, used: 0 });
    if current.saturating_sub(state.window_start) >= window {
        state = QuotaState { window_start: current, used: 0 };
    }
    if amount > limit || state.used > limit - amount {
        panic!("agent submission quota exceeded");
    }
    state.used = state.used.checked_add(amount).expect("quota usage overflow");
    env.storage().persistent().set(&key, &state);
    env.storage().persistent().extend_ttl(&key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_BUMP);
}

fn bump_instance_ttl(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_BUMP);
}

mod test;
