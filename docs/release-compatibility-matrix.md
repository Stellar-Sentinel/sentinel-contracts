# Release compatibility matrix

This matrix is the release gate for the Sentinel contract, backend, and
frontend. The contract keeps its public entry points and event topics stable;
the other repositories must treat optional response fields as optional and
must not infer a successful write from an event alone.

| Surface | Current contract v1 | Older deployment | Consumer rule |
| --- | --- | --- | --- |
| Entry points | Existing methods plus additive version/storage views | Existing methods remain callable | Do not remove or rename methods without a versioned interface change |
| Events | Existing topics and value shapes | Existing history remains readable | Ignore unknown future topics; never treat an event as proof of state |
| Storage | Version marker `1`; persistent keys retain their encoding | Missing marker can be backfilled by the guarded migration | Read the version before migration; do not synthesize missing records |
| Backend | Uses contract IDs, network passphrases, and explicit optional fields | May omit newer metadata | Preserve on-chain facts and label unavailable observations |
| Frontend | Renders facts, observations, and heuristic interpretation separately | Older response shapes remain valid | Treat optional fields as absent, never as zero or fabricated data |

## Upgrade order

1. Deploy and verify the contract build and interface version on the target
   network.
2. Confirm the storage version. Run the guarded storage migration only for an
   older supported version, then verify configuration and role views.
3. Deploy the backend with compatibility handling for both old and current
   response fields.
4. Deploy the frontend and verify loading, degraded, unavailable, and recovery
   states against a mocked old response and a current response.

## Rollback and failure atomicity

Pause new submissions before rolling back a consumer. Contract writes must
either complete with their state and success event together or leave no partial
state/event behind. A failed migration or compatibility check is a release
failure; do not retry by changing keys or writing arbitrary strings on-chain.

Record the contract ID, Wasm hash, network passphrase, interface version,
storage version, and migration transaction before declaring a release complete.
Never include signing seeds, tokens, private keys, raw reports, or personal
data in this matrix or contract state.
