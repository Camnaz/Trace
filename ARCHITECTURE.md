# Trace Architecture Specification

## Overview

Trace is an ultra-low-latency, asynchronous HTTP proxy that intercepts LLM-bound payloads, evaluates their semantic trajectory against customer-defined constraints, and renders a verdict in under 15 milliseconds.

---

## 1. Ingress/Egress Flow

### 1.1 Request Lifecycle

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   Client    │────▶│ Axum Router │────▶│   Proxy     │────▶│   Engine    │
│  Request    │     │  (Ingress)  │     │   Handler   │     │ (Evaluator) │
└─────────────┘     └─────────────┘     └─────────────┘     └──────┬──────┘
                                                                  │
                    ┌─────────────┐     ┌─────────────┐           │
                    │   Upstream  │◀────│   Proxy     │◀────────┘
                    │    LLM      │     │  (Egress)   │
                    └─────────────┘     └─────────────┘
```

### 1.2 Flow Stages

**Stage 1: Ingress Reception**
- Axum HTTP server accepts incoming POST requests at `/v1/proxy`
- Request body is streamed into a pre-allocated buffer pool to minimize allocations
- Headers are parsed and validated (content-type, authorization, x-customer-id)

**Stage 2: Payload Normalization**
- Body is deserialized into `IncomingPayload` using zero-copy deserialization where possible
- JSON parsing uses `serde_json` with `&str` references for string fields
- Customer context is extracted from headers and payload

**Stage 3: Trajectory Evaluation**
- Payload is passed to the Engine for semantic analysis
- Engine queries the Policy Store for active constraints
- Evaluation occurs without blocking the async runtime

**Stage 4: Verdict Application**
- `TraceVerdict::Pass`: Forward unmodified to upstream LLM
- `TraceVerdict::Block`: Return 403 with blocking reason, skip upstream call
- `TraceVerdict::Modify`: Apply transformations, then forward modified payload

**Stage 5: Egress Transmission**
- Axum client (hyper) forwards request to configured upstream LLM endpoint
- Response is streamed back to the client with minimal buffering
- Metrics are emitted asynchronously to avoid blocking

### 1.3 Performance Requirements

| Stage | Budget (ms) | Strategy |
|-------|-------------|----------|
| Ingress Reception | 2 | Pre-allocated buffers, connection pooling |
| Payload Parsing | 3 | Zero-copy deserialization, `Cow<str>` |
| Trajectory Eval | 8 | In-memory policy cache, SIMD vector ops |
| Verdict Apply | 2 | Branch prediction, pre-allocated response templates |
| **Total** | **15** | **Strict ceiling** |

---

## 2. The Trajectory Engine

### 2.1 Design Philosophy

The Trajectory Engine performs semantic evaluation without external LLM calls. It uses a hybrid approach combining:

1. **Vector Similarity Cache**: Fast in-memory embedding lookup
2. **Wasm Policy Matcher**: User-defined logic for complex constraints
3. **Regex/Keyword Cache**: O(1) pattern matching for known violations

### 2.2 Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     Trajectory Engine                          │
├─────────────────────────────────────────────────────────────────┤
│  ┌───────────────┐  ┌───────────────┐  ┌───────────────┐      │
│  │   Fast Path   │  │  Vector Cache │  │  Wasm Sandbox │      │
│  │  (Keyword/    │  │  (Embedding    │  │  (Complex     │      │
│  │   Regex)      │  │   Similarity)  │  │   Policies)   │      │
│  └───────┬───────┘  └───────┬───────┘  └───────┬───────┘      │
│          │                  │                  │              │
│          └──────────────────┴──────────────────┘              │
│                             │                                 │
│                             ▼                                 │
│                    ┌─────────────────┐                       │
│                    │  Score Fusion   │                       │
│                    │   & Threshold   │                       │
│                    └────────┬────────┘                       │
│                             │                                 │
│                             ▼                                 │
│                    ┌─────────────────┐                       │
│                    │  TraceVerdict   │                       │
│                    └─────────────────┘                       │
└─────────────────────────────────────────────────────────────────┘
```

### 2.3 Fast Path (Regex/Keyword Matcher)

- Pre-compiled `regex::RegexSet` for O(1) multi-pattern matching
- Aho-Corasick automaton for substring/keyword detection
- Bloom filter for probabilistic negative checks

### 2.4 Vector Similarity Cache

- Fixed-dimension embeddings (384-d or 768-d) stored in aligned arrays
- Cosine similarity computed via SIMD (AVX2/NEON)
- HNSW index for sub-millisecond approximate nearest neighbor search
- Cache eviction: LRU with TTL per customer policy

### 2.5 Wasm Policy Matcher

- Sandboxed execution for user-defined logic
- Pre-compiled Wasm modules cached in memory
- Gas metering to prevent runaway policies
- Fallback to default verdict if execution exceeds 5ms

---

## 3. State Management

### 3.1 In-Memory Policy Store

Customer policies and constraints are stored in an atomic, lock-free data structure to enable dynamic updates without restart.

```
┌──────────────────────────────────────────────────────────────┐
│                    Policy Store Architecture                  │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│   ┌─────────────────────┐      ┌─────────────────────┐      │
│   │   ArcSwap<Policy>   │      │   ArcSwap<Policy>   │      │
│   │   (Customer A)      │      │   (Customer B)      │      │
│   └─────────────────────┘      └─────────────────────┘      │
│                                                              │
│   ┌─────────────────────────────────────────────────────┐   │
│   │           DashMap<CustomerId, ArcSwap<Policy>>        │   │
│   │              (Lock-free concurrent hashmap)           │   │
│   └─────────────────────────────────────────────────────┘   │
│                                                              │
│   ┌─────────────────────────────────────────────────────┐   │
│   │        Update Channel (mpsc for hot reloads)          │   │
│   └─────────────────────────────────────────────────────┘   │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

### 3.2 Update Mechanism

**Hot Reload via Control API:**
```
POST /admin/v1/policies/{customer_id}
Authorization: Bearer {admin_token}
Content-Type: application/json

{
  "version": "2.1.0",
  "constraints": [...],
  "embedding_model": "all-MiniLM-L6-v2"
}
```

**Update Flow:**
1. New policy is validated (schema, Wasm bytecode check)
2. Vector embeddings are pre-computed and cached
3. `ArcSwap` atomically swaps the reference
4. Old policy is dropped after all in-flight requests complete

### 3.3 Persistence

- Policies are backed by an embedded KV store (sled) for crash recovery
- Periodic snapshots to disk (every 60s or on explicit admin trigger)
- On startup: Load from disk → Populate in-memory cache → Begin serving

### 3.4 Consistency Model

- **Eventual Consistency**: Policy updates propagate within 100ms
- **Read-Your-Own-Writes**: Admin API returns only after in-memory update
- **Per-Customer Isolation**: One customer's policy update never blocks another

---

## 4. Concurrency & Safety

### 4.1 Tokio Runtime Configuration

```rust
// Multi-threaded scheduler with work-stealing
let runtime = tokio::runtime::Builder::new_multi_thread()
    .worker_threads(num_cpus::get())
    .max_blocking_threads(512)
    .thread_stack_size(2 * 1024 * 1024)
    .enable_all()
    .build()
    .unwrap();
```

### 4.2 Memory Safety Guarantees

| Concern | Mitigation |
|---------|------------|
| Data Races | Rust borrow checker + `ArcSwap` for shared state |
| Memory Leaks | Bounded channels, drop guards, periodic heap profiling |
| Buffer Overruns | Pre-allocated buffer pools with size limits |
| Use-After-Free | `Arc` for shared data, explicit lifetimes |

### 4.3 Backpressure Handling

- Semaphore on concurrent evaluations (per-customer limits)
- Request queue with timeout (fail fast if >15ms)
- Circuit breaker for upstream LLM timeouts

---

## 5. Observability

### 5.1 Metrics (Prometheus-compatible)

- `trace_requests_total` - Counter with labels: customer_id, verdict
- `trace_latency_histogram` - Buckets: 1ms, 5ms, 10ms, 15ms, +Inf
- `trace_policy_cache_hits` - Gauge per customer
- `trace_upstream_errors` - Counter with upstream endpoint label

### 5.2 Tracing

- OpenTelemetry-compatible spans for each request stage
- Trace propagation via `traceparent` header
- Sampling: 1% by default, 100% for flagged customers

---

## 6. Security Model

### 6.1 Threat Mitigation

| Threat | Control |
|--------|---------|
| Prompt Injection | Vector similarity to known injection patterns |
| Data Exfiltration | Regex patterns for PII, block on match |
| Policy Tampering | mTLS on admin API, signed policy bundles |
| DoS via Large Payload | Body size limit (1MB default), streaming parser |
| Timing Attacks | Constant-time vector comparison where feasible |

---

## Appendix: File Structure

```
stria-trace/
├── Cargo.toml              # Workspace root
├── ARCHITECTURE.md         # This document
└── src/
    ├── main.rs             # Runtime initialization
    ├── proxy.rs            # Ingress/egress handling
    ├── types.rs            # Core data structures
    ├── engine/
    │   ├── mod.rs          # Public interface
    │   └── evaluator.rs    # Trajectory evaluation logic
    ├── policy/
    │   ├── mod.rs          # Public interface
    │   └── store.rs        # Lock-free policy storage
    └── config.rs           # Configuration loading
```

---

*Document Version: 1.0*
*Status: Planning Phase*
