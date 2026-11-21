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

Optimization guidance:

- Keep batch limits bounded; validate all entries before writing state.
- Prefer compact fixed-width values and existing storage keys over duplicated
  payloads. Do not store raw reports or arbitrary user text on-chain.
- Preserve the release profile and use `cargo bloat` only as a local diagnostic.
- Treat any threshold change as a compatibility decision and explain its
  deployment impact.
