# API Reference

Complete HTTP API reference for the Trace proxy. Covers the proxy endpoint, policy management endpoints, health, and metrics.

---

## Base URL

```
http://localhost:8080
```

The bind address is configurable via the `TRACE_BIND_ADDR` environment variable:

```bash
TRACE_BIND_ADDR="0.0.0.0:9090"
```

All endpoints described in this document are relative to the base URL. In production deployments behind a load balancer or API gateway, substitute the appropriate hostname and port.

---

## Authentication

### API key

All endpoints except `GET /health` and `GET /metrics` require an API key passed as a Bearer token in the `Authorization` header:

```
Authorization: Bearer <your-api-key>
```

The API key is configured at Trace startup via the `TRACE_API_KEY` environment variable. Requests without a valid key receive HTTP 401.

### Customer routing

The `x-customer-id` header routes proxy requests to the correct policy namespace:

```
x-customer-id: 3fa85f64-5717-4562-b3fc-2c963f66afa6
```

The value must be a valid UUID v4. Requests with an unrecognized `customer_id` (no loaded policy for that tenant) receive HTTP 404. See [Error codes](#error-codes) for the full response format.

---

## Endpoints

---

### POST /v1/proxy

Submit an `IncomingPayload` for policy evaluation. If the evaluation verdict is `pass` or `modify`, the (potentially modified) payload is forwarded to the configured upstream LLM endpoint and the response is streamed back to the caller. If the verdict is `block`, the upstream is not contacted.

**Request headers:**

| Header | Required | Description |
|---|---|---|
| `Content-Type` | Yes | Must be `application/json` |
| `Authorization` | Yes | `Bearer <api-key>` |
| `x-customer-id` | Yes | UUID of the tenant whose policy to evaluate against |
| `x-request-id` | No | UUID v4 for request tracing. Auto-generated if absent. Returned in response headers and included in all log lines for this request. |

**Request body — `IncomingPayload`:**

```json
{
  "prompt": "string",
  "system": "string",
  "target_model": "string",
  "context": {
    "key": "value"
  },
  "parameters": {}
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `prompt` | string | Yes | The user-facing prompt text. This is the primary field evaluated against keyword, vector similarity, and content length constraints. |
| `system` | string | No | System message or instruction context. Evaluated when a constraint's `target_field` is `"system"` or `"both"`. |
| `target_model` | string | Yes | Model identifier string (e.g., `"gpt-4o"`, `"claude-3-5-sonnet-20241022"`). Forwarded verbatim to the upstream LLM endpoint. Not used in policy evaluation. |
| `context` | object | No | Arbitrary string key-value pairs. Attached to evaluation logs and OpenTelemetry spans. Not evaluated against constraints unless a future custom Wasm constraint reads it. |
| `parameters` | object | No | Model generation parameters forwarded verbatim to the upstream (e.g., `{"temperature": 0.7, "max_tokens": 512}`). Not evaluated by Trace. |

**Response — 200 Pass:**

The upstream LLM response is forwarded as-is. Two Trace headers are appended:

```http
HTTP/1.1 200 OK
Content-Type: application/json
x-trace-verdict: pass
x-trace-latency-us: 412
x-request-id: 7e9a3c1d-8f42-4b0e-a123-000000000001

{ ...upstream LLM response body... }
```

**Response — 200 Modify:**

The payload was rewritten before forwarding. The upstream LLM response is forwarded as-is. Headers indicate the modification:

```http
HTTP/1.1 200 OK
Content-Type: application/json
x-trace-verdict: modify
x-trace-latency-us: 891
x-request-id: 7e9a3c1d-8f42-4b0e-a123-000000000002

{ ...upstream LLM response body... }
```

**Response — 403 Block:**

The upstream LLM was not contacted. Trace returns:

```http
HTTP/1.1 403 Forbidden
Content-Type: application/json
x-trace-verdict: block
x-trace-latency-us: 204
x-request-id: 7e9a3c1d-8f42-4b0e-a123-000000000003
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
| `verdict` | string | Always `"block"` |
| `explanation` | string | Human-readable description of the triggered constraint and matched pattern or reason |
| `triggered_constraints` | array of UUID strings | IDs of all constraints that matched. Typically one entry, but multiple log-action constraints may appear alongside a block-action constraint. |
| `latency_us` | integer | Trace evaluation time in microseconds, measured from payload receipt to verdict. Does not include network transit time. |

**Full curl example:**

```bash
curl -s -X POST http://localhost:8080/v1/proxy \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer your-trace-api-key" \
  -H "x-customer-id: 3fa85f64-5717-4562-b3fc-2c963f66afa6" \
  -H "x-request-id: 7e9a3c1d-8f42-4b0e-a123-000000000001" \
  -d '{
    "prompt": "Summarize the key risks in this earnings report.",
    "system": "You are a financial analyst assistant.",
    "target_model": "gpt-4o",
    "context": {
      "session_id": "sess_abc123",
      "environment": "production",
      "user_role": "analyst"
    },
    "parameters": {
      "temperature": 0.3,
      "max_tokens": 512
    }
  }'
```

---

### GET /v1/policies/{customer_id}

Retrieve the currently active policy for a customer tenant.

**Path parameters:**

| Parameter | Type | Description |
|---|---|---|
| `customer_id` | UUID string | The tenant whose policy to retrieve |

**Request headers:**

| Header | Required | Description |
|---|---|---|
| `Authorization` | Yes | `Bearer <api-key>` |

**Response — 200:**

Returns the full `CustomerPolicy` document currently loaded in memory.

```http
HTTP/1.1 200 OK
Content-Type: application/json
```

```json
{
  "customer_id": "3fa85f64-5717-4562-b3fc-2c963f66afa6",
  "version": "1.2.0",
  "default_verdict": "pass",
  "updated_at": "2024-01-15T14:32:00Z",
  "constraints": [
    {
      "id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
      "name": "trading_signal_block",
      "type": "keyword",
      "patterns": ["execute trade", "place order"],
      "case_sensitive": false,
      "target_field": "prompt",
      "action": "block",
      "priority": 1,
      "enabled": true
    }
  ]
}
```

**Full curl example:**

```bash
curl -s \
  -H "Authorization: Bearer your-trace-api-key" \
  "http://localhost:8080/v1/policies/3fa85f64-5717-4562-b3fc-2c963f66afa6" \
  | jq
```

---

### POST /v1/policies/{customer_id}

Create or replace the policy for a customer tenant. The update is applied atomically. In-flight requests continue evaluating against the previous policy; all subsequent requests use the new one.

**Path parameters:**

| Parameter | Type | Description |
|---|---|---|
| `customer_id` | UUID string | The tenant to create or update a policy for. Must match the `customer_id` field in the request body. |

**Request headers:**

| Header | Required | Description |
|---|---|---|
| `Content-Type` | Yes | `application/json` |
| `Authorization` | Yes | `Bearer <api-key>` |

**Request body:**

A complete `CustomerPolicy` document. See [Policy_Reference.md — Policy object schema](Policy_Reference.md#policy-object-schema) for the full field reference.

**Response — 200:**

```json
{
  "accepted": true,
  "version": "1.2.0",
  "constraint_count": 3
}
```

| Field | Type | Description |
|---|---|---|
| `accepted` | boolean | Always `true` on a 200 response |
| `version` | string | The `version` string from the submitted policy document |
| `constraint_count` | integer | Number of enabled constraints in the loaded policy |

**Response — 422:**

Returned when the policy document fails validation (invalid JSON schema, duplicate constraint IDs, empty `patterns` array, etc.):

```json
{
  "error": "validation_failed",
  "message": "constraint id 'a1b2c3d4-...' appears 2 times; constraint IDs must be unique within a policy"
}
```

**Full curl example:**

```bash
curl -s -X POST \
  "http://localhost:8080/v1/policies/3fa85f64-5717-4562-b3fc-2c963f66afa6" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer your-trace-api-key" \
  -d '{
    "customer_id": "3fa85f64-5717-4562-b3fc-2c963f66afa6",
    "version": "1.2.0",
    "default_verdict": "pass",
    "updated_at": "2024-01-15T14:32:00Z",
    "constraints": [
      {
        "id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
        "name": "trading_signal_block",
        "type": "keyword",
        "patterns": ["execute trade", "place order"],
        "case_sensitive": false,
        "target_field": "prompt",
        "action": "block",
        "priority": 1,
        "enabled": true
      },
      {
        "id": "e6f7a8b9-c0d1-2345-efab-678901234567",
        "name": "prompt_length_guard",
        "type": "content_length",
        "max_prompt_chars": 4096,
        "max_prompt_tokens": 1024,
        "action": "block",
        "priority": 2,
        "enabled": true
      },
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
    ]
  }' | jq
```

Expected response:

```json
{
  "accepted": true,
  "version": "1.2.0",
  "constraint_count": 3
}
```

---

### DELETE /v1/policies/{customer_id}

Remove all policies for a customer tenant. Subsequent proxy requests for this `customer_id` will receive HTTP 404 until a new policy is loaded.

**Path parameters:**

| Parameter | Type | Description |
|---|---|---|
| `customer_id` | UUID string | The tenant whose policy to remove |

**Request headers:**

| Header | Required | Description |
|---|---|---|
| `Authorization` | Yes | `Bearer <api-key>` |

**Response — 200:**

```json
{
  "removed": true
}
```

**Response — 404:**

Returned when no policy exists for the specified `customer_id`:

```json
{
  "error": "not_found",
  "message": "no policy found for customer_id '3fa85f64-5717-4562-b3fc-2c963f66afa6'"
}
```

**Full curl example:**

```bash
curl -s -X DELETE \
  -H "Authorization: Bearer your-trace-api-key" \
  "http://localhost:8080/v1/policies/3fa85f64-5717-4562-b3fc-2c963f66afa6" \
  | jq
```

---

### GET /health

Returns the operational status of the Trace instance. Does not require authentication. Intended for load balancer health checks and container liveness probes.

**Response — 200:**

```json
{
  "status": "ok",
  "uptime_seconds": 3842,
  "policy_count": 7
}
```

| Field | Type | Description |
|---|---|---|
| `status` | string | Always `"ok"` when the server is operational |
| `uptime_seconds` | integer | Seconds since the process started |
| `policy_count` | integer | Number of customer policies currently loaded in memory |

**Example Kubernetes liveness probe configuration:**

```yaml
livenessProbe:
  httpGet:
    path: /health
    port: 8080
  initialDelaySeconds: 5
  periodSeconds: 10
  failureThreshold: 3
```

**Full curl example:**

```bash
curl -s http://localhost:8080/health | jq
```

---

### GET /metrics

Returns Prometheus-format metrics for the Trace instance. Does not require authentication. Designed for scraping by a Prometheus server or compatible collector.

**Response — 200:**

```
Content-Type: text/plain; version=0.0.4; charset=utf-8
```

The response body is plain-text Prometheus exposition format.

**Key metrics exposed:**

| Metric | Type | Labels | Description |
|---|---|---|---|
| `trace_requests_total` | Counter | `verdict="pass\|block\|modify"` | Total proxy requests processed, partitioned by verdict |
| `trace_evaluation_latency_us` | Summary | `quantile="0.5\|0.95\|0.99"` | Trajectory Engine evaluation latency in microseconds (p50, p95, p99) |
| `trace_policy_violations_total` | Counter | `customer_id="..."`, `constraint_id="..."` | Total constraint matches, regardless of action type. Increments for `block`, `log`, and `modify` actions. |
| `trace_upstream_latency_ms` | Histogram | — | End-to-end upstream LLM response time in milliseconds. Not recorded for blocked requests. |
| `trace_upstream_errors_total` | Counter | `status="timeout\|connect_error\|http_error"` | Upstream LLM call failures, partitioned by error class |
| `trace_policy_reloads_total` | Counter | `customer_id="..."` | Number of policy hot-reloads per tenant since startup |

**Example metric output:**

```
# HELP trace_requests_total Total number of proxy requests processed
# TYPE trace_requests_total counter
trace_requests_total{verdict="pass"} 14823
trace_requests_total{verdict="block"} 342
trace_requests_total{verdict="modify"} 91

# HELP trace_evaluation_latency_us Trajectory Engine evaluation latency in microseconds
# TYPE trace_evaluation_latency_us summary
trace_evaluation_latency_us{quantile="0.5"} 387
trace_evaluation_latency_us{quantile="0.95"} 2841
trace_evaluation_latency_us{quantile="0.99"} 7203
trace_evaluation_latency_us_sum 9823412
trace_evaluation_latency_us_count 15256

# HELP trace_policy_violations_total Total constraint match events
# TYPE trace_policy_violations_total counter
trace_policy_violations_total{customer_id="3fa85f64-5717-4562-b3fc-2c963f66afa6",constraint_id="a1b2c3d4-e5f6-7890-abcd-ef1234567890"} 87

# HELP trace_upstream_latency_ms Upstream LLM response latency in milliseconds
# TYPE trace_upstream_latency_ms histogram
trace_upstream_latency_ms_bucket{le="100"} 2341
trace_upstream_latency_ms_bucket{le="500"} 12048
trace_upstream_latency_ms_bucket{le="1000"} 14701
trace_upstream_latency_ms_bucket{le="+Inf"} 14914
trace_upstream_latency_ms_sum 3841920
trace_upstream_latency_ms_count 14914
```

**Full curl example:**

```bash
curl -s http://localhost:8080/metrics
```

**Prometheus scrape configuration:**

```yaml
scrape_configs:
  - job_name: trace
    static_configs:
      - targets: ["localhost:8080"]
    metrics_path: /metrics
    scrape_interval: 15s
```

---

## Error codes

| HTTP Status | Error key | Meaning | Remediation |
|---|---|---|---|
| `400 Bad Request` | `invalid_payload` | Request body is not valid JSON or is missing required fields (`prompt`, `target_model`) | Validate the request body against the `IncomingPayload` schema |
| `401 Unauthorized` | `missing_auth` | `Authorization` header is absent | Add `Authorization: Bearer <api-key>` to the request |
| `401 Unauthorized` | `invalid_auth` | API key does not match `TRACE_API_KEY` | Verify the API key value; check for trailing whitespace or encoding issues |
| `403 Forbidden` | — | A `block` action constraint matched the request | This is an expected operational response, not an error. Inspect the `triggered_constraints` field. |
| `404 Not Found` | `customer_not_found` | No policy is loaded for the specified `x-customer-id` | POST a policy to `/v1/policies/{customer_id}` before sending proxy requests for that tenant |
| `422 Unprocessable Entity` | `validation_failed` | Policy document failed schema validation on a management endpoint | Inspect the `message` field for the specific validation error |
| `429 Too Many Requests` | `rate_limit_exceeded` | Management API rate limit exceeded (100 req/min per API key) | Reduce request frequency; see [Rate limits on the management API](#rate-limits-on-the-management-api) |
| `500 Internal Server Error` | `internal_error` | An unexpected error occurred inside Trace | Check Trace structured logs for the `request_id`; report to support with the full log line |
| `502 Bad Gateway` | `upstream_error` | Trace reached the upstream LLM but received an error response | Check upstream LLM service status and `TRACE_UPSTREAM_URL` configuration |
| `504 Gateway Timeout` | `upstream_timeout` | Upstream LLM did not respond within the configured timeout | Check `TRACE_UPSTREAM_TIMEOUT_MS`; verify upstream LLM service health |

All error responses follow the same JSON body structure:

```json
{
  "error": "error_key",
  "message": "Human-readable description of the error",
  "request_id": "7e9a3c1d-8f42-4b0e-a123-000000000001"
}
```

The `request_id` in error responses corresponds to either the `x-request-id` header value supplied by the caller, or the auto-generated UUID assigned by Trace.

---

## Rate limits on the management API

The policy management endpoints (`GET /v1/policies/*`, `POST /v1/policies/*`, `DELETE /v1/policies/*`) are rate-limited to **100 requests per minute per API key**. The proxy endpoint (`POST /v1/proxy`) is not subject to management API rate limits; tenant-level rate limiting is handled via the `rate_limit` constraint type within policies.

When the management API rate limit is exceeded, Trace returns:

```http
HTTP/1.1 429 Too Many Requests
Content-Type: application/json
Retry-After: 14
```

```json
{
  "error": "rate_limit_exceeded",
  "message": "Management API rate limit exceeded: 100 requests per 60 seconds",
  "retry_after_seconds": 14
}
```

The `Retry-After` header and `retry_after_seconds` field indicate how many seconds to wait before the next request will succeed.

---

## API versioning

The Trace API is versioned at the path prefix level. The current stable version is `/v1/`. All endpoints documented here are under `/v1/`.

Future breaking changes will be introduced at `/v2/` with a minimum **90-day deprecation window** for `/v1/` endpoints. Deprecation notices will be communicated via:

- `Deprecation` and `Sunset` HTTP response headers on affected endpoints
- Release notes in the Trace changelog
- Direct notification to registered API key holders (enterprise plans)

Non-breaking additions (new optional request fields, new response fields, new metric labels) may be made to existing `/v1/` endpoints without a version increment.
