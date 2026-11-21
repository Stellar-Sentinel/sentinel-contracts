# Soroban event schema

The contract emits typed Soroban events. Consumers must decode the topic list
and value with the schema below; topic position is significant. `topic[0]` is
the event name and the remaining topics are ordered arguments.

| Event | Topics, in order | Value | Compatibility |
| --- | --- | --- | --- |
| `init` | `init`, `admin: Address` | `threshold: u32` | Emitted once during initialization. |
| `agent_add` | `agent_add`, `admin: Address`, `agent: Address` | `true: bool` | Legacy authorization event. |
| `agent_del` | `agent_del`, `admin: Address`, `agent: Address` | `false: bool` | Legacy revoke event. |
| `threshold` | `threshold`, `admin: Address` | `(previous: u32, current: u32)` | No event is emitted when the value is unchanged. |
| `pause` | `pause`, `admin: Address` | `paused: bool` | Only emitted when the state changes. |
| `gpause` | `gpause`, `guardian: Address` | `true: bool` | Guardian pause; only emitted on the first pause. |
| `gunpause` | `gunpause`, `admin: Address` | `false: bool` | Resumes a guardian pause. |
| `flagged` | `flagged`, `agent: Address`, `subject: Address` | `score: u32` | Stable legacy flag event; do not reinterpret or add topics. |
| `flaggedv2` | `flaggedv2`, `agent: Address`, `subject: Address`, `report_digest: BytesN<32>` | `score: u32` | Versioned event. The digest is a canonical off-chain report hash, not report contents. |

## Decoding rules

- Decode `topic[0]` as a short `Symbol`, then validate exact event arity
  before reading later topics.
- Decode addresses as Soroban `Address`, scores and thresholds as `u32`, and
  `flaggedv2` digests as exactly 32 bytes.
- Decode `threshold`'s value as a two-element tuple `(u32, u32)` and all other
  values as the scalar type shown above.
- A failed or unauthorized invocation is atomic: it changes neither state nor
  events. Consumers should not infer success from a submitted transaction
  until the confirmed event is present.
- Events are append-only history. `get_latest_flag` is only the current
  snapshot and may be cleared or expire independently of historical events.

## Versioning and legacy behavior

`get_interface_version()` currently returns `1`. Existing consumers must keep
the `flagged` decoder at three topics and a `u32` value. New report-linked
submissions use `flaggedv2`; they do not change `flagged` or the `FlagRecord`
shape. An incompatible topic or value change requires a new interface/event
version and coordinated backend migration. Raw reports, secrets, and PII must
never be placed in topics or values.

## Example decoded events

```json
{
  "topics": ["flagged", "G...agent", "G...subject"],
  "value": 90
}
```

```json
{
  "topics": ["flaggedv2", "G...agent", "G...subject", "32-byte SHA-256 digest"],
  "value": 90
}
```
