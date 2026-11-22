# Storage TTL and archival operations

This contract keeps security configuration in instance storage and each latest
flag in a persistent `LatestFlag(subject)` entry. Soroban archival is expected
maintenance, not a contract upgrade: archived entries must be restored by the
transaction footprint before a method can read them.

## TTL extension points

The current implementation uses a threshold of 10,000 ledgers and bumps live
entries by 100,000 ledgers. These values are implementation policy, not a new
ABI field. The contract refreshes the instance TTL at initialization, reads of
live instance configuration, and successful state-changing administrative or
flag-submission calls. A latest-flag persistent entry is refreshed when it is
written and when `get_latest_flag` finds it live. `clear_latest_flag` removes
the entry and does not recreate it.

The following calls do not resurrect an archived entry from inside the
contract:

- `get_latest_flag(subject)` returns `None` when no live record exists, but an
  archived record must be restored first to read its value.
- `flag_anomaly`, `flag_anomaly_v2`, `flag_anomaly_v3`, and `flag_anomalies`
  write a fresh persistent record after their normal authorization checks.
- `migrate_storage_version` and `migrate_agent_capacity` update instance
  configuration only after administrator authorization.

Events are append-only history. They remain queryable by indexers even after a
latest-flag record is archived or cleared.

## Read and restore procedure

Use the same network and contract ID as the backend. First inspect the
instance configuration with read-only calls:

```bash
export CONTRACT_ID=<contract-id>
export SOURCE_ACCOUNT=<read-only-capable-account-alias>

stellar contract invoke --id "$CONTRACT_ID" --source-account "$SOURCE_ACCOUNT" \
  --network testnet --send=no -- get_interface_version
stellar contract invoke --id "$CONTRACT_ID" --source-account "$SOURCE_ACCOUNT" \
  --network testnet --send=no -- get_storage_version
stellar contract invoke --id "$CONTRACT_ID" --source-account "$SOURCE_ACCOUNT" \
  --network testnet --send=no -- get_configuration_view
```

If `get_latest_flag` needs an archived persistent entry, prepare a transaction
with the Soroban restore footprint for the exact `LatestFlag(subject)` key,
then invoke the read or state-changing operation in that transaction. The
restore operation is network/tooling-specific and must be performed by the
authorized operational account; the contract cannot add it during execution.
Never put a seed, token, private key, or credential-bearing RPC URL in these
commands or in this repository.

After restoration, verify the subject record, `get_storage_version`, pause
latches, and the backend event feed. If the record was intentionally cleared,
the expected result is an explicit empty state and the historical event stream
remains the source for past flags.

## Migration compatibility

`get_storage_version` returns the instance marker, or `0` for an older
deployment without one. Run `migrate_storage_version(admin)` only when the
administrator has confirmed that the deployment is an older supported schema.
The migration backfills the marker, rejects an unsupported nonzero version,
does not rewrite agent roles or latest flags, and publishes no flag event.

For instances predating the agent registry, run
`migrate_agent_capacity(admin, active_count, capacity)` only with values
verified from the deployment's existing agent state. Confirm capacity bounds,
agent count, monitor/responder roles, and expiry state after the transaction.
Existing function and event schemas remain unchanged; a new deployment is
required for contract code changes because this contract is not upgraded in
place.

## Failure atomicity and emergency response

An unauthorized migration, invalid capacity, unsupported storage version, or
failed restore must not be treated as a successful maintenance action. Record
the transaction result and re-run the read-only verification set. During an
incident, pause flag submissions with the administrator or configured
guardian, keep read paths available, and resume only after the restored state
and backend compatibility are verified.

Coordinate deployment and API compatibility through the
[sentinel-backend issue tracker](https://github.com/Stellar-Sentinel/sentinel-backend/issues)
and the [sentinel-frontend issue tracker](https://github.com/Stellar-Sentinel/sentinel-frontend/issues).
The dashboard must distinguish an archived/unavailable record from a score of
zero and must not treat an on-chain flag as proof of fraud or compliance.

## Rollback checklist

1. Pause new submissions if the migration or restore result is uncertain.
2. Preserve the contract ID, network, transaction hash, source commit, and
   observed storage version.
3. Roll the application deployment back to the last compatible frontend and
   backend versions; contract code is not rolled back in place.
4. Re-run the read-only configuration and event checks above.
5. Document the compatibility decision before resuming monitoring.
