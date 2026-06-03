# Policy Reference

Complete reference for the Trace policy system: schema definitions, constraint types, evaluation semantics, and production patterns for financial services deployments.

---

## Overview

A **policy** is the authoritative rule set governing how Trace evaluates requests for a given customer tenant. Policies are JSON documents stored in Trace's in-memory policy store and evaluated synchronously on the hot path of every inbound request.

### Natural language to deterministic rules

Trace does not perform its own semantic interpretation of human intent. Policies are explicit, deterministic, and version-controlled. The intended workflow is:

1. A compliance or engineering team identifies a category of restricted behavior (e.g., "LLM must not be used to generate trading instructions").
2. That intent is encoded as one or more constraints with specific patterns, thresholds, or limits.
3. The policy document is committed to source control and deployed via the policy management API.
4. Trace evaluates every request against these constraints at the time of the request, with no external calls.

### Policy lifecycle

```
Author policy.json → POST /v1/policies/{customer_id} → ArcSwap atomic replace
                                                               ↓
                                          In-flight requests complete on old policy
                                          All new requests use new policy
                                                               ↓
                                          Old policy Arc ref-count reaches zero → dropped
```

Policy updates are atomic. There is no window during which requests are evaluated against a partially-applied policy. Zero restarts are required.

---

## Policy object schema

A `CustomerPolicy` document has the following top-level structure:

```json
{
  "customer_id": "<uuid>",
  "version": "<semver string>",
  "default_verdict": "pass",
  "updated_at": "<RFC 3339 datetime>",
  "constraints": [ ... ]
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `customer_id` | UUID string | Yes | The tenant this policy applies to. Must match the `x-customer-id` header used on proxy requests. |
| `version` | string | Yes | Semantic version string (e.g., `"2.1.0"`). Stored for audit and rollback purposes. Not used for ordering or conflict resolution — the most recently POSTed policy is always authoritative. |
| `default_verdict` | `"pass"` \| `"block"` \| `"modify"` | No | Verdict returned when no constraints match. Defaults to `"pass"`. Set to `"block"` for deny-by-default postures. |
| `updated_at` | RFC 3339 datetime | Yes | Timestamp of the policy version. Used in audit logs and Policy Studio display. Set by the caller; Trace does not override this value. |
| `constraints` | array of `PolicyConstraint` | Yes | Ordered list of constraints. Evaluated in ascending `priority` order. An empty array with `default_verdict: "pass"` is valid and will pass all requests. |

---

## PolicyConstraint schema

Each entry in the `constraints` array is a `PolicyConstraint` object. The `type` field is a discriminated union tag that determines which additional fields are required.

```json
{
  "id": "<uuid>",
  "name": "<string>",
  "type": "<keyword|vector_similarity|rate_limit|content_length>",
  "action": "<block|log|modify>",
  "priority": 1,
  "enabled": true,
  ...type-specific fields...
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | UUID string | Yes | Stable identifier for this constraint. Referenced in `triggered_constraints` arrays in `EvaluationResult` responses and in metrics labels. Must be unique within a policy. |
| `name` | string | Yes | Human-readable label. Appears in block response `explanation` fields and in Policy Studio. |
| `type` | string enum | Yes | Constraint type. Determines which additional fields are expected. |
| `action` | string enum | Yes | Action to take when this constraint matches. See [Actions](#actions). |
| `priority` | integer (0–65535) | Yes | Evaluation order. Lower values are evaluated first. When multiple constraints match, the one with the lowest `priority` value determines the final verdict. |
| `enabled` | boolean | No | Whether this constraint participates in evaluation. Defaults to `true`. Set to `false` to disable without removing the constraint. |

---

## Constraint types

### keyword

Evaluates whether the target field(s) contain any of the specified patterns. Matching uses Aho-Corasick for literal strings and compiled `regex::RegexSet` for pattern strings.

**Under the hood:** At policy load time, Trace inspects each entry in `patterns`. Strings containing regex metacharacters (`.`, `*`, `+`, `?`, `[`, `]`, `(`, `)`, `{`, `}`, `^`, `$`, `|`, `\`) are compiled into a `RegexSet`. Pure literal strings are added to an Aho-Corasick automaton. At evaluation time, the Aho-Corasick pass runs first (O(n) in text length, independent of pattern count), followed by the `RegexSet` pass if any regex patterns are defined. This means a policy with 1,000 literal keyword patterns adds negligible latency compared to a policy with one.

**JSON schema:**

```json
{
  "id": "...",
  "name": "...",
  "type": "keyword",
  "patterns": ["string or regex", "..."],
  "case_sensitive": false,
  "target_field": "prompt",
  "action": "block",
  "priority": 1,
  "enabled": true
}
```

**Type-specific fields:**

| Field | Type | Required | Description |
|---|---|---|---|
| `patterns` | array of strings | Yes | One or more literal strings or regular expressions. Evaluation uses OR logic: any single match triggers the constraint. Must not be empty. |
| `case_sensitive` | boolean | No | Whether pattern matching is case-sensitive. Defaults to `false`. When `false`, text is lowercased before matching; patterns should be written in lowercase. |
| `target_field` | `"prompt"` \| `"system"` \| `"both"` | No | Which payload field(s) to evaluate. Defaults to `"prompt"`. Use `"both"` to match against either `prompt` or `system`; the constraint fires if either field matches. |

**Complete example:**

```json
{
  "id": "b3c4d5e6-f7a8-9012-bcde-f34567890abc",
  "name": "trading_signal_block",
  "type": "keyword",
  "patterns": [
    "execute trade",
    "place order",
    "buy \\d+ shares",
    "sell \\d+ shares"
  ],
  "case_sensitive": false,
  "target_field": "prompt",
  "action": "block",
  "priority": 1,
  "enabled": true
}
```

**Performance notes:** Keyword constraints are the fastest constraint type. Evaluation time is O(n) where n is the length of the target field. A keyword constraint with 100 literal patterns and a 4,096-character prompt typically evaluates in under 200 µs. This is well within the 8 ms Trajectory Eval budget.

---

### vector_similarity

Evaluates semantic proximity by computing cosine similarity between the prompt embedding and a pre-computed reference embedding. The reference embedding encodes the concept you want to detect (e.g., "giving specific investment advice").

**How reference embeddings work:** You generate the reference embedding offline using the same model you specify in the `model` field. Embed a sentence that canonically represents the concept you want to catch, such as: `"I recommend you buy this specific stock because it will go up"`. Store the resulting float array as `reference_embedding`. At evaluation time, Trace embeds the incoming prompt using the same model (cached per model per request), then computes cosine similarity. If the similarity score is ≥ `threshold`, the constraint fires.

**Generating a reference embedding** (example using OpenAI):

```python
import openai, json

client = openai.OpenAI(api_key="sk-...")
response = client.embeddings.create(
    model="text-embedding-3-small",
    input="I recommend you buy this specific stock because it will increase in value"
)
embedding = response.data[0].embedding
print(json.dumps(embedding))  # Paste into reference_embedding field
```

**JSON schema:**

```json
{
  "id": "...",
  "name": "...",
  "type": "vector_similarity",
  "reference_embedding": [0.023, -0.041, 0.118, "..."],
  "threshold": 0.82,
  "model": "text-embedding-3-small",
  "action": "block",
  "priority": 10,
  "enabled": true
}
```

**Type-specific fields:**

| Field | Type | Required | Description |
|---|---|---|---|
| `reference_embedding` | array of float32 | Yes | The pre-computed embedding vector for the reference concept. Dimension must match the output of the specified `model`. |
| `threshold` | float (0.0–1.0) | Yes | Cosine similarity threshold. The constraint fires when `similarity(prompt_embedding, reference_embedding) >= threshold`. A value of `1.0` means exact match (identity). Values in the `0.75`–`0.90` range are typical for semantic category detection. Lower thresholds increase recall but reduce precision. |
| `model` | string | Yes | Identifier of the embedding model. Trace must have access to this model's inference endpoint. The model identifier is passed to the configured embedding backend. |

**Complete example:**

```json
{
  "id": "c4d5e6f7-a8b9-0123-cdef-456789012345",
  "name": "investment_advice_boundary",
  "type": "vector_similarity",
  "reference_embedding": [0.0231, -0.0412, 0.1183, -0.0774, 0.0558],
  "threshold": 0.82,
  "model": "text-embedding-3-small",
  "action": "block",
  "priority": 10,
  "enabled": true
}
```

**Latency notes:** Vector similarity constraints add to the Trajectory Eval budget. Embedding inference for a 512-token prompt typically takes 3–6 ms against a local embedding server, or 20–80 ms against a remote API. For the 8 ms eval budget to hold, embedding inference must use a locally-hosted model (e.g., `all-MiniLM-L6-v2` via ONNX Runtime) with SIMD-accelerated cosine similarity. Remote embedding calls will exceed the 15 ms total budget. Configure `TRACE_EMBEDDING_ENDPOINT` to point to a local embedding service.

---

### rate_limit

Limits the number of requests a customer tenant can send within a rolling time window. Rate limit state is maintained in-memory per tenant and resets automatically as the window expires.

**JSON schema:**

```json
{
  "id": "...",
  "name": "...",
  "type": "rate_limit",
  "max_requests": 1000,
  "window_seconds": 60,
  "action": "block",
  "priority": 5,
  "enabled": true
}
```

**Type-specific fields:**

| Field | Type | Required | Description |
|---|---|---|---|
| `max_requests` | integer (u32) | Yes | Maximum number of requests permitted within the window. |
| `window_seconds` | integer (u64) | Yes | Length of the rolling time window in seconds. |

**Scoping:** Rate limit state is scoped to the `customer_id`. Each tenant has an independent counter. There is no global rate limit across tenants.

**Complete example:**

```json
{
  "id": "d5e6f7a8-b9c0-1234-defa-567890123456",
  "name": "per_tenant_rate_limit",
  "type": "rate_limit",
  "max_requests": 500,
  "window_seconds": 60,
  "action": "block",
  "priority": 5,
  "enabled": true
}
```

When this constraint fires, the block response `explanation` field will include the current request count and window reset time.

---

### content_length

Rejects prompts that exceed a character or token count ceiling. Applies before the prompt reaches the Trajectory Engine's deeper evaluation stages, making it an efficient pre-filter.

**Token counting:** `max_prompt_tokens` uses an approximate tokenizer that counts whitespace-delimited tokens. It is not model-specific. For exact token counts, rely on `max_prompt_chars` which is O(1) via string length.

**JSON schema:**

```json
{
  "id": "...",
  "name": "...",
  "type": "content_length",
  "max_prompt_chars": 4096,
  "max_prompt_tokens": 1024,
  "action": "block",
  "priority": 2,
  "enabled": true
}
```

**Type-specific fields:**

| Field | Type | Required | Description |
|---|---|---|---|
| `max_prompt_chars` | integer (usize) | Yes | Maximum allowed prompt length in Unicode characters (not bytes). Prompts exceeding this length are rejected before any other evaluation. |
| `max_prompt_tokens` | integer (usize) | No | Maximum allowed prompt length in approximate tokens. When both `max_prompt_chars` and `max_prompt_tokens` are set, the constraint fires if either limit is exceeded. |

**Complete example:**

```json
{
  "id": "e6f7a8b9-c0d1-2345-efab-678901234567",
  "name": "prompt_length_guard",
  "type": "content_length",
  "max_prompt_chars": 4096,
  "max_prompt_tokens": 1024,
  "action": "block",
  "priority": 2,
  "enabled": true
}
```

---

## Actions

Each constraint specifies an `action` that determines what Trace does when the constraint matches.

### block

The request is rejected immediately. Trace returns HTTP 403 with a JSON body containing the verdict, explanation, triggered constraint IDs, and evaluation latency. The upstream LLM is never contacted. No further constraints are evaluated after a `block` fires.

Response body on block:

```json
{
  "verdict": "block",
  "explanation": "Prompt matched keyword constraint 'trading_signal_block': pattern 'execute trade'",
  "triggered_constraints": ["a1b2c3d4-e5f6-7890-abcd-ef1234567890"],
  "latency_us": 204
}
```

### log

The request passes through to the upstream LLM as if no constraint matched. The constraint match is recorded in the structured log output and emitted as a `trace_policy_violations_total` Prometheus metric increment. The `x-trace-verdict: pass` response header is set. Use `log` action for monitoring and alerting on violations before enforcing a block.

### modify

The payload is transformed before being forwarded to the upstream LLM. The modification logic depends on the constraint type:

- **keyword** with `modify`: The matched pattern occurrences in the prompt are redacted (replaced with `[REDACTED]`).
- **content_length** with `modify`: The prompt is truncated to `max_prompt_chars` characters. A truncation marker `[TRUNCATED]` is appended.

The response includes `x-trace-verdict: modify`. The `EvaluationResult` contains a `modified_payload` field with the transformed request body that was forwarded upstream.

---

## Priority and evaluation order

Constraints within a policy are evaluated in ascending `priority` order (lowest number first). Evaluation stops at the first constraint that triggers a `block` action.

**Example with multiple constraints:**

```
priority 1 → content_length (block if > 4096 chars)
priority 2 → keyword "execute trade" (block)
priority 5 → rate_limit (block if > 500 req/min)
priority 10 → vector_similarity investment advice (log)
```

Evaluation order for a request:

1. `content_length` is checked first. If the prompt is 5,000 characters, the request is blocked immediately. Constraints at priorities 2, 5, and 10 are not evaluated.
2. If the prompt is within the length limit, `keyword` is evaluated. If "execute trade" is found, the request is blocked. Priority 5 and 10 are not evaluated.
3. If no keyword match, `rate_limit` is checked.
4. If within rate limit, `vector_similarity` is evaluated; a match produces a log entry but passes the request.

**When multiple `log` action constraints match**, all matching constraint IDs are collected into `triggered_constraints` and all are emitted to metrics. Evaluation continues through all remaining constraints.

**`modify` and `log` constraints do not stop evaluation.** Only `block` terminates the evaluation chain. After a `modify` action fires, subsequent constraints at higher priority values are still evaluated against the (potentially modified) payload.

**`default_verdict`** applies only when no constraints fire at all, or when all matching constraints have `action: log`.

---

## Policy versioning

The `version` field is a free-form string. Stria Systems recommends semantic versioning (`MAJOR.MINOR.PATCH`) with the following conventions:

| Increment | When |
|---|---|
| `PATCH` | Adding or enabling/disabling a constraint without changing block logic |
| `MINOR` | Adding new block constraints or changing thresholds |
| `MAJOR` | Changing `default_verdict`, restructuring constraint set, or breaking changes |

**Hot-reload behavior:** When a new policy is POSTed to `/v1/policies/{customer_id}`, Trace performs an atomic swap using `ArcSwap<CustomerPolicy>`. The sequence is:

1. New policy document is validated (schema, pattern compilation, embedding dimension check).
2. Aho-Corasick automaton and `RegexSet` are compiled from the new constraint set.
3. `ArcSwap` store is updated atomically — all readers see either the old or the new policy, never a partial state.
4. In-flight requests holding a reference to the old policy continue to completion.
5. Once all in-flight references are dropped, the old policy is deallocated.

There is no configuration file to reload, no `SIGHUP` to send, and no restart required.

---

## Financial services policy patterns

The following four patterns are included in Auto-Demo mode and represent common compliance requirements for financial services LLM deployments. Each is production-ready and can be adapted directly.

### `unauthorized_trading_signals`

Blocks prompts that contain language attempting to use the LLM to generate or relay trading instructions, including ticker + action patterns.

```json
{
  "id": "f7a8b9c0-d1e2-3456-fabc-789012345678",
  "name": "unauthorized_trading_signals",
  "type": "keyword",
  "patterns": [
    "execute trade",
    "place order",
    "buy \\d+ shares",
    "sell \\d+ shares",
    "go long",
    "go short",
    "open position",
    "close position",
    "take profit at",
    "stop loss at",
    "[A-Z]{1,5}\\s+(buy|sell|long|short)"
  ],
  "case_sensitive": false,
  "target_field": "both",
  "action": "block",
  "priority": 1,
  "enabled": true
}
```

The ticker pattern `[A-Z]{1,5}\s+(buy|sell|long|short)` matches constructions like `AAPL buy`, `TSLA short`, or `NVDA long`. Set `case_sensitive: true` if your deployment only handles uppercase tickers.

---

### `pii_financial_leakage`

Blocks prompts containing Social Security Numbers, IBAN account numbers, ISIN security identifiers, or standard account number patterns. Prevents inadvertent PII submission to LLM providers.

```json
{
  "id": "a8b9c0d1-e2f3-4567-abcd-890123456789",
  "name": "pii_financial_leakage",
  "type": "keyword",
  "patterns": [
    "\\b\\d{3}-\\d{2}-\\d{4}\\b",
    "\\b\\d{9}\\b",
    "\\bIBAN\\s*:?\\s*[A-Z]{2}\\d{2}[A-Z0-9]{4}\\d{7}([A-Z0-9]?){0,16}\\b",
    "\\b[A-Z]{2}[A-Z0-9]{9}[0-9]\\b",
    "\\baccount\\s*(number|no\\.?|#)\\s*:?\\s*\\d{6,20}\\b",
    "\\brouting\\s*number\\s*:?\\s*\\d{9}\\b"
  ],
  "case_sensitive": false,
  "target_field": "both",
  "action": "block",
  "priority": 2,
  "enabled": true
}
```

Pattern annotations:
- `\b\d{3}-\d{2}-\d{4}\b` — US SSN in dashed format
- `\b\d{9}\b` — Nine-digit SSN without dashes (also catches EIN; tune as needed)
- IBAN pattern — Matches GB, DE, FR, and other IBAN formats
- `\b[A-Z]{2}[A-Z0-9]{9}[0-9]\b` — 12-character ISIN format
- Account/routing number patterns — Catches explicit labeled references

---

### `investment_advice_boundary`

Uses vector similarity to detect prompts semantically close to soliciting personalized investment advice, regardless of exact wording. This catches paraphrased requests that keyword matching would miss.

```json
{
  "id": "b9c0d1e2-f3a4-5678-bcde-901234567890",
  "name": "investment_advice_boundary",
  "type": "vector_similarity",
  "reference_embedding": [
    0.0231, -0.0412, 0.1183, -0.0774, 0.0558,
    0.0342, -0.0991, 0.0667, 0.1024, -0.0388
  ],
  "threshold": 0.82,
  "model": "text-embedding-3-small",
  "action": "block",
  "priority": 10,
  "enabled": true
}
```

To use this constraint in production, replace `reference_embedding` with a vector generated from a sentence that canonically represents the restricted concept, such as:

> "You should invest your savings in this specific stock because it will perform well in the next quarter."

The 10-element embedding shown is illustrative. Production embeddings from `text-embedding-3-small` are 1,536-dimensional.

---

### `prompt_length_guard`

Rejects prompts exceeding 4,096 characters or 1,024 approximate tokens. Prevents context-stuffing attacks and controls upstream LLM costs.

```json
{
  "id": "c0d1e2f3-a4b5-6789-cdef-012345678901",
  "name": "prompt_length_guard",
  "type": "content_length",
  "max_prompt_chars": 4096,
  "max_prompt_tokens": 1024,
  "action": "block",
  "priority": 3,
  "enabled": true
}
```

Set `priority` lower than semantic constraints so that oversized prompts are rejected before the more expensive vector similarity evaluation is attempted.

---

## Policy-as-code

Trace policies are JSON documents and integrate naturally into source control workflows.

### Recommended directory structure

```
policies/
├── README.md
├── schema/
│   └── policy.schema.json          # JSON Schema for validation
├── customers/
│   ├── acme-bank/
│   │   ├── policy.json             # Active policy
│   │   └── history/
│   │       ├── v1.0.0.json
│   │       └── v1.1.0.json
│   ├── meridian-funds/
│   │   └── policy.json
│   └── clearview-am/
│       └── policy.json
└── shared/
    └── constraints/
        ├── pii_financial_leakage.json   # Reusable constraint fragments
        └── prompt_length_guard.json
```

### CI/CD integration pattern

The following GitHub Actions workflow validates and deploys policy changes on merge to `main`:

```yaml
name: Deploy Policies

on:
  push:
    branches: [main]
    paths: ["policies/customers/**"]

jobs:
  validate-and-deploy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Validate policy schemas
        run: |
          for f in policies/customers/*/policy.json; do
            echo "Validating $f"
            npx ajv validate \
              -s policies/schema/policy.schema.json \
              -d "$f" \
              --strict=false
          done

      - name: Deploy changed policies
        env:
          TRACE_API_KEY: ${{ secrets.TRACE_API_KEY }}
          TRACE_URL: ${{ secrets.TRACE_URL }}
        run: |
          git diff --name-only HEAD~1 HEAD \
            | grep '^policies/customers/.*/policy.json$' \
            | while read policy_file; do
                customer_dir=$(dirname "$policy_file")
                customer_id=$(jq -r '.customer_id' "$policy_file")
                echo "Deploying policy for customer $customer_id"
                response=$(curl -s -o /dev/null -w "%{http_code}" \
                  -X POST "${TRACE_URL}/v1/policies/${customer_id}" \
                  -H "Authorization: Bearer ${TRACE_API_KEY}" \
                  -H "Content-Type: application/json" \
                  -d @"$policy_file")
                if [ "$response" != "200" ]; then
                  echo "ERROR: Policy deployment failed for $customer_id (HTTP $response)"
                  exit 1
                fi
                echo "Deployed $customer_id → HTTP $response"
              done
```

**Recommended practices:**

- Pin `version` to match your git tag. Use `git describe --tags` or a CI-injected variable to populate this field automatically.
- Require peer review on all `policies/customers/` changes via branch protection rules.
- Store previous policy versions in `history/` as immutable files for rollback and audit trails. The Trace API does not retain policy history — that is the responsibility of your version control system.
- Use `enabled: false` to shadow-disable a constraint for testing before removal, rather than deleting it outright. This preserves the constraint ID in historical evaluation logs.
