# Stellar Sentinel Soroban Contract

[![CI](https://github.com/Stellar-Sentinel/sentinel-contracts/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/Stellar-Sentinel/sentinel-contracts/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)

Soroban contract for administrator-managed monitoring agents, a configurable 0–100 score threshold, and account flags. It publishes `flagged` events and stores the latest flag for each subject. It does not store full history; event indexing is needed for that.

## Agent registry

`get_agents` returns the administrator-authenticated list of active legacy agents. The registry is capped at 128 unique addresses; repeated authorization is idempotent, and revocation removes the address so capacity is released. Existing agent mapping storage remains intact, and the new registry key is appended to the storage-key enum.

## Architecture

```mermaid
flowchart LR
  Admin[Contract administrator] -->|authorize, revoke, set threshold| Contract[Stellar Sentinel Soroban contract]
  Agent[Authorized agent] -->|flag_anomaly| Contract
  Contract -->|latest flag state| State[Soroban persistent storage]
  Contract -->|flagged event| RPC[Stellar RPC event stream]
  RPC -->|read events| Backend[Stellar Sentinel backend]
  Backend -->|JSON API| UI[Dashboard]
```

The backend reads events and does not sign or submit transactions. The contract and backend RPC must use the same network for events to appear in the dashboard.

### Configuration events

Admin changes publish typed Soroban events for off-chain audit consumers:

| Topic 0 | Additional topics | Value | Meaning |
| --- | --- | --- | --- |
| `agent_add` | administrator address, affected agent address | `true` | The agent was authorized. |
| `agent_del` | administrator address, affected agent address | `false` | The agent was revoked. |
| `threshold` | administrator address | `(previous_threshold, new_threshold)` as two `u32` values | The risk threshold changed. |

The existing `flagged` event remains unchanged: its topics are `flagged`, agent address, and subject address, and its value is the `u32` score. The backend event reader currently filters only for `flagged`; decoding these configuration events is a separate follow-up. A failed or unauthorized call aborts before publishing an event.

## Testnet deployment

The current Stellar Sentinel instance is deployed and initialized on the **Stellar Testnet**. It uses the Testnet network passphrase `Test SDF Network ; September 2015` and the Soroban RPC endpoint `https://soroban-testnet.stellar.org`.

| Detail | Value |
| --- | --- |
| Contract ID | [`CCZAAZ3FJ7LKZA7E7A6EKQTU2HCNVI3YUVIHKWHSULGZSWAJFS2D2XVX`](https://stellar.expert/explorer/testnet/contract/CCZAAZ3FJ7LKZA7E7A6EKQTU2HCNVI3YUVIHKWHSULGZSWAJFS2D2XVX) |
| Contract source | [`5b6b5a7`](https://github.com/Stellar-Sentinel/sentinel-contracts/commit/5b6b5a7df1219242579833d0f46f4f215745eca1) |
| Wasm SHA-256 | `f501250438515ff26e18ebf951cd62de717c711e68d031fb0fad5c9a9b36bd5d` |
| Score threshold | `70` (initialized) |
| Admin account | `GAPNXDMWAMEWHEVMSDXYD5BS2V3P6E4A7OPG6F2MLME5666VSPT5BIHM` |
| Authorized monitoring agents | None yet |

Transaction records:

- Wasm upload: [transaction `3838b8af1eadb429f37e7d4c3d6c8267773b138a967d13b816491090a32e3992`](https://stellar.expert/explorer/testnet/tx/3838b8af1eadb429f37e7d4c3d6c8267773b138a967d13b816491090a32e3992)
- Contract deployment: [transaction `903e26dd3d1740d3833714fa30afaaf6466e0823c40250d842944dded6b8e123`](https://stellar.expert/explorer/testnet/tx/903e26dd3d1740d3833714fa30afaaf6466e0823c40250d842944dded6b8e123)
- Initialization: [transaction `bfd4da30f78d13160f95b6d401983db6f70bbba11273e9374989e5b4d0c19b2d`](https://stellar.expert/explorer/testnet/tx/bfd4da30f78d13160f95b6d401983db6f70bbba11273e9374989e5b4d0c19b2d)

The admin identity alias `stellar-sentinel-testnet-admin` is stored in the deploying machine's macOS Keychain using Stellar CLI secure storage. Its secret is not in this repository. Preserve the Keychain entry and securely back up the recovery material. The backend's `.env.example` points to this contract. The event feed is connected, but it currently has no flags because no monitoring agent has been authorized.

### Reproduce a separate Testnet deployment

These commands create a **new** identity and contract instance; they do not redeploy or alter the contract above. Install the Stellar CLI, Rust, and the Wasm target first. Keep the generated identity in secure storage and never commit its seed phrase.

```bash
stellar keys generate sentinel-admin --network testnet --fund --secure-store
stellar contract build --locked
stellar contract deploy \
  --wasm target/wasm32v1-none/release/sentinel_contract.wasm \
  --source-account sentinel-admin \
  --network testnet \
  --alias sentinel_contract
stellar contract invoke \
  --id sentinel_contract \
  --source-account sentinel-admin \
  --network testnet -- \
  initialize --admin sentinel-admin --default-threshold 70
```

To verify this deployment's initialized threshold without submitting a transaction:

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source-account sentinel-admin \
  --network testnet --send=no -- \
  get_threshold
```

Set the resulting contract ID as the backend's `CONTRACT_ID`. Keep its Horizon and Soroban RPC endpoints on Testnet as well.

## Project layout

- `src/lib.rs` — contract entry points, storage keys, authorization, threshold checks, event publication, and latest-flag lookup.
- `src/test.rs` — contract behavior tests using Soroban test utilities.
- `Cargo.toml` / `Cargo.lock` — Rust and Soroban SDK dependencies.
- `.github/workflows/ci.yml` — Wasm build, unit tests, and Clippy checks.

## Build and test

Requires stable Rust, the `wasm32-unknown-unknown` and `wasm32v1-none` targets, and Stellar CLI for optimized deployment builds. There are no contract-specific environment variables; network IDs and signing identities are supplied to deployment tooling outside this repository.

```bash
rustup target add wasm32-unknown-unknown wasm32v1-none
cargo build --target wasm32-unknown-unknown --release
stellar contract build --locked
cargo test
cargo clippy --all-targets -- -D warnings
```

The CI Wasm artifact is under `target/wasm32-unknown-unknown/release/`. `stellar contract build --locked` creates the optimized deployable artifact under `target/wasm32v1-none/release/`. CI runs the Wasm build, tests, and lint checks (Clippy).

## Contract interface

- `get_interface_version()` — return the interface generation; increment it for incompatible method or event changes.
- `initialize(admin, default_threshold)` — one-time admin and threshold setup.
- `get_admin()` — read the configured administrator after initialization.
- `transfer_admin(current_admin, new_admin)` — atomically transfer control; both addresses must authorize the same invocation.
- `authorize_agent(admin, agent)` / `revoke_agent(admin, agent)` — manage flagging agents. `authorize_agents` / `revoke_agents` support bounded batches of up to 16 addresses; repeated entries are idempotent.
- `set_threshold(admin, threshold)` / `get_threshold()` — configure/read the threshold.
- `is_score_accepted(score)` — preflight the score range and active threshold; `flag_anomaly` remains authoritative.
- `pause(admin)` / `unpause(admin)` / `is_paused()` — stop or resume new flag submissions; read operations remain available.
- `is_agent(agent)` — check agent authorization and extend the active contract instance TTL.
- `flag_anomaly(agent, subject, score)` — require an authorized agent and a score at or above threshold; persist the latest record and publish `flagged`.
- `flag_anomalies(agent, submissions)` — submit 1–16 subject/score entries in one transaction, validating the full batch before writes.
- `get_latest_flag(subject)` — read the latest record, if one exists.
- `clear_latest_flag(admin, subject)` — remove the current record while leaving historical `flagged` events unchanged.

Successful `authorize_agent`, `revoke_agent`, and `set_threshold` calls also publish the corresponding configuration events described above.

Only trusted addresses should receive agent authorization. The contract enforces the score range and threshold, but it cannot establish that an off-chain score is accurate. Storage follows Soroban TTL and archival rules. Deploy, initialize, and configure each network separately; never commit secrets.

Pausing is an emergency control for new `flag_anomaly` submissions. It does not erase existing flags or block read methods, and new deployments start unpaused. Only the administrator can pause or resume submissions.

For an administrator handover, prepare one `transfer_admin` invocation authorized by both the current admin and the proposed admin. The operation fails without either authorization, and the proposed address becomes the sole administrator after success. Verify the new administrator can call an admin-only method before removing access to the old signing setup.
