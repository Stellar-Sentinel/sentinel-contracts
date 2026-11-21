# Event topic schema and invariants

This note is the consumer contract for Soroban RPC event decoding. Topic
position and value type are part of the ABI. Consumers must check the event
name and exact arity before decoding an address, digest, or score.

## Stable event table

`topic[0]` is the short event symbol. Remaining topics are ordered exactly as
shown below.

| Event | Topic sequence | Value | Validity and compatibility |
| --- | --- | --- | --- |
| `init` | `init`, admin | `u32 threshold` | One-time initialization; threshold is 0–100. |
| `agent_add` | `agent_add`, admin, agent | `bool true` | Successful grant only. |
| `agent_del` | `agent_del`, admin, agent | `bool false` | Successful revoke only. |
| `threshold` | `threshold`, admin | `(u32 previous, u32 current)` | Values are 0–100; unchanged values emit no event. |
| `pause` | `pause`, admin | `bool` | Standard pause state transition. |
| `gpause` | `gpause`, guardian | `bool true` | First guardian pause transition only. |
| `gunpause` | `gunpause`, admin | `bool false` | Guardian-pause recovery. |
| `flagged` | `flagged`, agent, subject | `u32 score` | Legacy v1 shape; exactly 3 topics. |
| `flaggedv2` | `flaggedv2`, agent, subject, report digest | `u32 score` | Digest is exactly 32 bytes; raw reports are never stored. |
| `flaggedv3` | `flaggedv3`, agent, subject, policy id | `(u32 score, u32 reason mask)` | Policy id is 1–1,000,000; reason mask is 1–65,535. |

Addresses are Soroban `Address` values, not strings inside the XDR event.
Scores, thresholds, policy IDs, and reason masks are unsigned 32-bit values.
The v2 digest is `BytesN<32>`. A decoder must reject an event with missing,
extra, or misordered topics rather than guessing a version.

## Valid, invalid, and boundary cases

- Valid flag: score equals the configured threshold or is greater; one latest
  record is written and one version-appropriate event is emitted.
- Invalid flag: score above 100, below threshold, unauthorized agent, or an
  active pause aborts before the latest record and event are written.
- Valid v3 boundaries: policy ID 1 and 1,000,000; reason mask 1 and 65,535.
- Invalid v3 boundaries: policy ID 0 or above 1,000,000; reason mask 0 or any
  bit above the lower 16 bits. No event or latest-record write is allowed.
- Valid v2 digest: exactly 32 bytes. A different length is rejected by the
  contract type before event publication.
- Empty or unknown RPC event: keep it out of the typed event stream and log a
  decoder mismatch for investigation; never render it as a flag.

All rejected calls are failure-atomic: no latest flag, storage migration,
authorization change, or event is partially committed. Events are append-only
history; a latest flag may be cleared or archived without deleting history.

## RPC and invocation examples

Read the interface and state without submitting a transaction:

```bash
stellar contract invoke --id <CONTRACT_ID> --network testnet --send=no -- \
  get_interface_version
stellar contract invoke --id <CONTRACT_ID> --network testnet --send=no -- \
  get_configuration_view
```

The standard submission shape is:

```bash
stellar contract invoke --id <CONTRACT_ID> --source-account <AGENT_ALIAS> \
  --network testnet -- flag_anomaly_v3 \
  --agent <AGENT_ADDRESS> --subject <SUBJECT_ADDRESS> --score 90 \
  --policy-id 7 --reason-mask 3
```

Use an indexer's `getEvents` response to inspect `topics` and `value`, then
decode only after checking the topic symbol and count. The backend should
retain the raw event identifier, ledger, contract ID, and transaction hash so
an operator can reproduce a decoding mismatch without adding report contents
to chain.

## Versioning and storage constraints

`get_interface_version()` is currently `1`. Existing `flagged` and `flaggedv2`
consumers remain valid; `flaggedv3` is additive and must be explicitly opted
into. An incompatible change requires a new event/interface version and a
coordinated backend and frontend release. Existing storage keys and
`FlagRecord` remain unchanged by versioned event metadata.

TTL maintenance applies to the instance configuration and persistent latest
flag records, not to event history. If a latest record is archived, restore
the exact persistent key in the transaction footprint before reading it; the
contract cannot restore an archived entry during invocation.

Event topics and values must contain no signing seed, token, private key, raw
report, PII, or arbitrary user text. A score or flag is a monitoring signal,
not proof of fraud, compliance, ownership, or intent.

Coordinate decoder changes through the [sentinel-backend issue
tracker](https://github.com/Stellar-Sentinel/sentinel-backend/issues) and
frontend rendering through the [sentinel-frontend issue
tracker](https://github.com/Stellar-Sentinel/sentinel-frontend/issues).
