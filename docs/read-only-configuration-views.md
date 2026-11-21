# Read-only configuration view

`get_configuration_view()` is the bounded operator snapshot for the current
contract configuration. It is public, requires no authorization, emits no
event, and does not return registry addresses or any secret material. Existing
`get_contract_config()` remains unchanged for ABI compatibility.

## ABI and fields

The Soroban ABI return type is `ConfigurationView`:

| Field | Meaning |
| --- | --- |
| `interface_version` | Constant external interface generation. |
| `initialized` | Whether the administrator key exists. |
| `admin` | Configured administrator, or `None` before initialization. |
| `threshold` | Risk threshold, or `None` before initialization. |
| `paused` | Standard submission pause state; defaults to `false`. |
| `guardian_paused` | Emergency guardian pause state; defaults to `false`. |
| `emergency_guardian` | Optional configured guardian address. |
| `agent_count` | Active, non-expired agents at the current ledger. |
| `agent_capacity` | Persisted capacity, or `None` on legacy instances. |
| `storage_version` | Persisted schema version, or `None` before migration. |

The view scans the bounded registry without cleaning expired entries. It
refreshes the instance TTL only when initialized; it does not extend
persistent flag TTLs, write configuration values, or publish events. Agent
addresses remain available only through the existing administrator-authenticated
`get_agents(admin)` method.

## Compatibility and migration

This is an additive entry point. Existing methods, storage keys, and event
schemas are unchanged, and `get_contract_config()` retains its original
`Option<ContractConfig>` return type. Legacy instances may report `None` for
fields whose storage keys were introduced after deployment. Consumers should
branch on `initialized` and treat optional fields as absent rather than
inventing defaults. No migration is required to call the view.

## Invocation and RPC decoding

CLI invocation (read-only simulation):

```sh
stellar contract invoke --id <CONTRACT_ID> --network testnet --send=no -- \
  get_configuration_view
```

The RPC result is an XDR-encoded `ConfigurationView`; decode it with the same
contract specification used for the deployed WASM. A decoded response has the
following shape (addresses abbreviated):

```text
ConfigurationView {
  interface_version: 1,
  initialized: true,
  admin: Some(Address("G...")),
  threshold: Some(70),
  paused: false,
  guardian_paused: false,
  emergency_guardian: None,
  agent_count: 1,
  agent_capacity: Some(128),
  storage_version: Some(1),
}
```

The result is a point-in-time read. Ledger expiry can change `agent_count`
between calls, while the registry is compacted only by existing administrative
or count/list refresh paths.
