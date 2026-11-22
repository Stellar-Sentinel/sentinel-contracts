# Wasm size and compute budget

The release profile is deliberately size-oriented (`opt-level = "z"`, LTO,
one codegen unit, stripped symbols, and an aborting panic strategy). CI reports
the optimized Wasm size and enforces the compatibility ceiling below.

| Measurement | Threshold | How to inspect |
| --- | ---: | --- |
| `sentinel_contract.wasm` | 256 KiB | `scripts/check-wasm-budget.sh target/wasm32-unknown-unknown/release/sentinel_contract.wasm` |
| Soroban CPU instructions per representative invocation | 10,000,000 | Simulate with `stellar contract invoke --send=no` and record the `cpu_insns` budget in the RPC result. |
| Soroban memory per representative invocation | 64 MiB | Record the simulation `mem_bytes` budget in the RPC result. |

The CPU and memory values are review thresholds for representative calls, not
ABI limits. Before extending the contract, simulate initialization,
single-flag, versioned-flag, maximum batch, and maximum agent-registry calls.
Record the command, network, interface version, Wasm hash, and budget result in
the PR or release notes. A failed simulation is a release blocker even when
the Wasm file remains below the byte ceiling.

## Release procedure

1. Build the exact artifact that will be deployed with
   `stellar contract build --locked` and run
   `scripts/check-wasm-budget.sh` against that artifact. Record its SHA-256,
   byte size, interface version, and source commit in the release notes.
2. Simulate the representative calls listed above on Testnet with
   `stellar contract invoke --send=no`. Keep the RPC result, including
   `cpu_insns` and `mem_bytes`, in the release record. Do not put a signing
   seed, token, or private key in a command, log, or committed file.
3. Compare the new interface and event schema with
   [`docs/EVENT_SCHEMA.md`](EVENT_SCHEMA.md). Additive changes require an
   explicitly versioned consumer rollout; changing an existing topic or
   function is a breaking release.

The backend and frontend must be updated in the same compatibility window:
the backend should deploy its decoder before emitting a new versioned event,
and the frontend should tolerate the previous version while rollout is in
progress. Link the contract source commit, Wasm hash, backend release, and
frontend release in the release notes so an incident can identify the exact
combination in production.

## Migration, rollback, and emergency response

Read the existing storage and interface versions before migration. Run only
the documented migration entry points, verify the resulting read-only views,
and record the before/after values. Never use a Wasm size or compute-budget
change as a reason to rewrite existing event or storage keys.

If a deployment exceeds a budget or a consumer cannot decode its events,
pause flag submissions, keep read-only views available, and roll the backend
and frontend back to the last compatible releases. Soroban code is not
upgraded in place here: a rollback of contract behavior means directing
clients to the previously verified instance or deploying a separately
verified replacement. Resume submissions only after Testnet verification,
consumer compatibility checks, and administrator approval.

For a review, attach the budget measurements, Wasm hash, compatibility
assessment, migration/rollback plan, and the result of the emergency-response
drill. A change that cannot provide those records is not ready to deploy.

Optimization guidance:

- Keep batch limits bounded; validate all entries before writing state.
- Prefer compact fixed-width values and existing storage keys over duplicated
  payloads. Do not store raw reports or arbitrary user text on-chain.
- Preserve the release profile and use `cargo bloat` only as a local diagnostic.
- Treat any threshold change as a compatibility decision and explain its
  deployment impact.
