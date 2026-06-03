# Quickstart

Get Trace running, evaluate your first payload, and load your first policy in under ten minutes.

---

## Prerequisites

| Requirement | Version | Notes |
|---|---|---|
| Docker | 20.10+ | Required to run the Trace container |
| curl | Any recent | Used for all examples below |
| OpenAI or Anthropic API key | — | Optional. Required only for live upstream forwarding. Not needed for Auto-Demo mode. |

All examples in this guide use `localhost:8080`. If you bind Trace to a different address, substitute accordingly.

---

## Installation

Pull the latest Trace image:

```bash
docker pull stria/trace:latest
```

### Option A — Connect to a real LLM upstream

Set your upstream LLM endpoint and API key via environment variables:

```bash
docker run --rm -d \
  --name trace \
  -p 8080:8080 \
  -e TRACE_UPSTREAM_URL="https://api.openai.com/v1/chat/completions" \
  -e TRACE_UPSTREAM_API_KEY="sk-..." \
  -e TRACE_API_KEY="your-trace-api-key" \
  stria/trace:latest
```

Trace will forward evaluated (non-blocked) requests to the configured upstream URL, inject the upstream API key, and stream the response back to the caller.

### Option B — Auto-Demo mode (no LLM upstream required)

Set `TRACE_DEMO=1` to start Trace with pre-loaded financial services policies and synthetic traffic. No upstream LLM configuration is needed.

```bash
docker run --rm -d \
  --name trace \
  -p 8080:8080 \
  -e TRACE_DEMO=1 \
  -e TRACE_API_KEY="demo" \
  stria/trace:latest
```

See [Auto-Demo mode](#auto-demo-mode) below for a full walkthrough of what this environment provides.

### Verify the server is healthy

```bash
curl -s http://localhost:8080/health | jq
```

Expected response:

```json
{
  "status": "ok",
  "uptime_seconds": 3,
  "policy_count": 0
}
```

In Auto-Demo mode, `policy_count` will be non-zero immediately at startup.

---

## Your first request

Submit a payload to the proxy endpoint. Trace evaluates it against any loaded policies for the specified customer, then either blocks the request, modifies it, or forwards it to the upstream LLM.

```bash
curl -s -X POST http://localhost:8080/v1/proxy \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer demo" \
  -H "x-customer-id: 3fa85f64-5717-4562-b3fc-2c963f66afa6" \
  -H "x-request-id: 7e9a3c1d-8f42-4b0e-a123-000000000001" \
  -d '{
    "prompt": "What is the capital of France?",
    "system": "You are a helpful assistant.",
    "target_model": "gpt-4o",
    "context": {
      "session_id": "sess_001",
      "environment": "production"
    },
    "parameters": {
      "temperature": 0.7,
      "max_tokens": 256
    }
  }' | jq
```

**Field reference:**

| Field | Type | Required | Description |
|---|---|---|---|
| `prompt` | string | Yes | The user-facing prompt text to evaluate and forward |
| `system` | string | No | System message / instruction context |
| `target_model` | string | Yes | Model identifier string forwarded to the upstream endpoint |
| `context` | object | No | Arbitrary key-value metadata attached to the request for policy evaluation and logging |
| `parameters` | object | No | Model parameters forwarded verbatim to the upstream (e.g., `temperature`, `max_tokens`) |

The `x-customer-id` header is a UUID that routes the request to the correct policy namespace. The `x-request-id` header is optional — Trace auto-generates a UUID v4 if absent.

---

## Reading the verdict

Trace communicates its evaluation decision via HTTP status code and response headers.

### Pass (HTTP 200)

When no constraints fire, Trace forwards the request to the upstream and streams the response back. Two headers are injected into the upstream response:

```
x-trace-verdict: pass
x-trace-latency-us: 412
```

`x-trace-latency-us` is the Trace evaluation time in microseconds, not including upstream LLM latency.

### Modify (HTTP 200)

When a `modify` action constraint fires, Trace rewrites the payload before forwarding. The response headers indicate the modification:

```
x-trace-verdict: modify
x-trace-latency-us: 891
```

### Block (HTTP 403)

When a `block` action constraint fires, Trace does not contact the upstream. It returns:

```http
HTTP/1.1 403 Forbidden
Content-Type: application/json
x-trace-verdict: block
x-trace-latency-us: 204
```

```json
{
  "verdict": "block",
  "explanation": "Prompt matched keyword constraint 'trading_signal_block': pattern 'execute trade'",
  "triggered_constraints": ["a1b2c3d4-e5f6-7890-abcd-ef1234567890"],
  "latency_us": 204
}
```

| Field | Type | Description |
|---|---|---|
| `verdict` | string | Always `"block"` on a 403 |
| `explanation` | string | Human-readable reason for the block |
| `triggered_constraints` | array of UUIDs | IDs of the constraints that matched |
| `latency_us` | integer | Trace evaluation time in microseconds |

---

## Auto-Demo mode

When `TRACE_DEMO=1` is set, Trace bootstraps a self-contained demonstration environment. No upstream LLM is contacted; responses to non-blocked requests are synthesized locally.

**What is pre-loaded:**

- **Four financial services policies** covering unauthorized trading signals, PII leakage, investment advice boundaries, and prompt length limits. See [Policy_Reference.md — Financial services policy patterns](Policy_Reference.md#financial-services-policy-patterns) for the full definitions.
- **Three synthetic customer tenants** (UUIDs printed to stdout at startup) with distinct policy assignments.
- **Continuous synthetic traffic** — a background goroutine emits ~10 requests/second across tenants, producing a live mix of pass, block, and modify verdicts.

**Policy Studio UI:**

Open [http://localhost:8080/ui](http://localhost:8080/ui) in a browser to access the Policy Studio. In demo mode you will see:

- A live verdict stream with per-request verdict, latency, and triggered constraint details
- Per-tenant traffic graphs (requests/min, block rate)
- The full policy configuration for each demo tenant, editable in-browser

**Startup output** in demo mode includes the demo tenant UUIDs:

```
INFO trace: demo mode enabled
INFO trace: loaded 4 demo policies
INFO trace: demo tenants:
INFO trace:   acme-bank      → 3fa85f64-5717-4562-b3fc-2c963f66afa6
INFO trace:   meridian-funds → 8d6e2a0f-3c4b-4d7e-9f1a-2b3c4d5e6f70
INFO trace:   clearview-am   → 1a2b3c4d-5e6f-7a8b-9c0d-e1f2a3b4c5d6
INFO trace: synthetic traffic generator started (10 req/s)
```

Use those UUIDs as `x-customer-id` header values when sending manual test requests in demo mode.

---

## Write your first policy

A policy is a JSON document containing one or more constraints. Each constraint specifies what to detect, which field to check, and what action to take.

The following policy defines a single `keyword` constraint that blocks any prompt mentioning `"execute trade"` or `"place order"`. Save it as `policy.json`:

```json
{
  "customer_id": "3fa85f64-5717-4562-b3fc-2c963f66afa6",
  "version": "1.0.0",
  "default_verdict": "pass",
  "updated_at": "2024-01-15T00:00:00Z",
  "constraints": [
    {
      "id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
      "name": "trading_signal_block",
      "type": "keyword",
      "patterns": [
        "execute trade",
        "place order"
      ],
      "case_sensitive": false,
      "target_field": "prompt",
      "action": "block",
      "priority": 1,
      "enabled": true
    }
  ]
}
```

**Key fields:**

| Field | Value | Notes |
|---|---|---|
| `type` | `"keyword"` | Uses Aho-Corasick for multi-pattern matching |
| `patterns` | array of strings | Each pattern is a literal string or regex; OR logic — any match triggers the constraint |
| `case_sensitive` | `false` | Matching is case-insensitive |
| `target_field` | `"prompt"` | Only the `prompt` field is evaluated; `"system"` and `"both"` are also valid |
| `action` | `"block"` | Request is rejected with HTTP 403; upstream is never contacted |
| `priority` | `1` | Lower numbers are evaluated first. Priority 1 is the highest urgency. |

---

## Load the policy

POST the policy document to the policy management endpoint. Replace the UUID in the path with your customer ID:

```bash
curl -s -X POST \
  "http://localhost:8080/v1/policies/3fa85f64-5717-4562-b3fc-2c963f66afa6" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer demo" \
  -d @policy.json | jq
```

Expected response:

```json
{
  "accepted": true,
  "version": "1.0.0",
  "constraint_count": 1
}
```

The policy is applied atomically. In-flight requests continue evaluating against the previous policy; all subsequent requests use the new one. No restart is required.

---

## Test the policy

### Request that triggers the block

```bash
curl -s -X POST http://localhost:8080/v1/proxy \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer demo" \
  -H "x-customer-id: 3fa85f64-5717-4562-b3fc-2c963f66afa6" \
  -d '{
    "prompt": "Please execute trade AAPL x100 at market price.",
    "target_model": "gpt-4o"
  }' | jq
```

Expected response (HTTP 403):

```json
{
  "verdict": "block",
  "explanation": "Prompt matched keyword constraint 'trading_signal_block': pattern 'execute trade'",
  "triggered_constraints": ["a1b2c3d4-e5f6-7890-abcd-ef1234567890"],
  "latency_us": 187
}
```

### Request that passes

```bash
curl -s -X POST http://localhost:8080/v1/proxy \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer demo" \
  -H "x-customer-id: 3fa85f64-5717-4562-b3fc-2c963f66afa6" \
  -d '{
    "prompt": "Summarize the Q3 earnings report for AAPL.",
    "target_model": "gpt-4o"
  }'
```

This prompt contains no patterns from the constraint. Trace evaluates it, finds no match, and (in live upstream mode) forwards it to the configured LLM. In Auto-Demo mode, a synthetic response is returned with `x-trace-verdict: pass`.

---

## Next steps

- **[Policy_Reference.md](Policy_Reference.md)** — Full reference for all constraint types (`keyword`, `vector_similarity`, `rate_limit`, `content_length`), actions, priority semantics, policy versioning, and financial services policy patterns.
- **[API_Reference.md](API_Reference.md)** — Complete HTTP API reference for the proxy endpoint, policy management endpoints, health, and metrics.
