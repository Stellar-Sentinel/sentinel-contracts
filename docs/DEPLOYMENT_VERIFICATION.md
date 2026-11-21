# Testnet deployment verification

Post-deploy verification is read-only and must be run against the same network
and contract ID used by the backend. The checks below establish that the
deployed instance is the intended interface, configuration, roles, pause
state, and Wasm artifact.

Set local shell variables without committing them:

```bash
export CONTRACT_ID=<contract-id>
export SOURCE_ACCOUNT=<read-only-capable-account-alias>
export NETWORK=testnet
```

Run these invocations with `--send=no`:

```bash
stellar contract invoke --id "$CONTRACT_ID" --source-account "$SOURCE_ACCOUNT" \
  --network "$NETWORK" --send=no -- get_interface_version
stellar contract invoke --id "$CONTRACT_ID" --source-account "$SOURCE_ACCOUNT" \
  --network "$NETWORK" --send=no -- get_contract_config
stellar contract invoke --id "$CONTRACT_ID" --source-account "$SOURCE_ACCOUNT" \
  --network "$NETWORK" --send=no -- get_storage_version
stellar contract invoke --id "$CONTRACT_ID" --source-account "$SOURCE_ACCOUNT" \
  --network "$NETWORK" --send=no -- is_paused
stellar contract invoke --id "$CONTRACT_ID" --source-account "$SOURCE_ACCOUNT" \
  --network "$NETWORK" --send=no -- is_guardian_paused
stellar contract invoke --id "$CONTRACT_ID" --source-account "$SOURCE_ACCOUNT" \
  --network "$NETWORK" --send=no -- get_agent_count
stellar contract invoke --id "$CONTRACT_ID" --source-account "$SOURCE_ACCOUNT" \
  --network "$NETWORK" --send=no -- get_agents --admin <admin-address>
```

For the deployment documented in `README.md`, the expected values are:

- interface version `1`, storage version `1`, threshold `70`;
- the documented admin address is the returned `admin`;
- `is_paused` and `is_guardian_paused` are `false`;
- the responder address is present in `get_agents` and
  `is_responder --agent <responder-address>` returns `true`;
- the contract ID and network passphrase match the backend configuration.

## Wasm and event checks

Build the exact locked source used for the release, then compare its SHA-256
with the release record. Do not use a locally generated identity or submit a
transaction for verification:

```bash
stellar contract build --locked
shasum -a 256 target/wasm32v1-none/release/sentinel_contract.wasm
```

The digest must match the published deployment record. For event verification,
query the Testnet Soroban RPC `getEvents` method for the contract and confirm a
known `init`, administrative, or `flagged` event decodes according to
`docs/EVENT_SCHEMA.md`. Check topic count before decoding values; an event with
unexpected arity is a deployment or decoder mismatch.

Record the contract ID, source commit, Wasm SHA-256, network, interface
version, returned configuration, role addresses, pause states, and event
transaction/RPC identifiers in the release note. A mismatch blocks promotion
and must not be “fixed” by changing the expected values silently.
