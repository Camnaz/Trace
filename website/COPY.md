# STRIA SYSTEMS — TRACE WEBSITE COPY

---

# 1. HOMEPAGE

---

## Above the Fold

### Headline
**Trace**  
HTTP Proxy for Deterministic LLM Request Governance

### Subheadline
Intercept, evaluate, and enforce policy on every outbound LLM request. Sub-15ms latency. Zero external dependencies. Runs entirely within your VPC.

### Primary CTA
[Schedule Technical Demo]

### Secondary CTA
[Read Architecture Documentation]

---

## How It Works

### Section Label
Implementation Path

### Step 1: Route
Redirect LLM-bound traffic through Trace. Deploy as a sidecar, standalone proxy, or Kubernetes service. Configure your application to point to Trace's ingress endpoint. No SDK required.

### Step 2: Define
Author policies in the Policy Studio or declarative YAML. Define match conditions—regex patterns, semantic similarity thresholds, or custom Wasm logic. Assign verdicts: Pass, Block, or Modify.

### Step 3: Enforce
Every request is evaluated against active policies in under 15 milliseconds. Verdicts are applied inline. All decisions are logged, exportable, and auditable.

---

## Performance Specifications

### Section Headline
Sub-15ms Evaluation Ceiling

### The Breakdown
| Phase | Latency | Mechanism |
|-------|---------|-----------|
| Ingress | 2ms | HTTP/1.1 + HTTP/2 parser with zero-copy buffer management |
| Parse | 3ms | SIMD-accelerated JSON extraction; payload normalization |
| Eval | 8ms | Fast-path regex, vector cache lookup, optional Wasm sandbox |
| Apply | 2ms | Verdict execution: forward, reject, or transform |
| **Total** | **15ms** | Hard ceiling at p99 under production load |

### Key Metrics
- **Throughput**: 50,000+ evaluations per second per node
- **Memory**: <512MB base footprint per instance
- **p99 Latency**: 12.4ms sustained under 10K RPS load test
- **Cold Start**: Zero. Policies reload without restart.

---

## Differentiation: Why Trace Is Not Another Guardrail Wrapper

### Section Headline
Trace vs. SaaS Guardrail Layers

1. **No API Calls in the Critical Path**  
   SaaS guardrails require outbound HTTPS to external infrastructure. Trace evaluates entirely within your network boundary. No added round-trip. No third-party latency variance.

2. **Deterministic Performance Profile**  
   SaaS latency is a distribution with outliers. Trace latency is a ceiling: 15ms maximum. Predictable for high-frequency trading infrastructure.

3. **Data Residency Guarantee**  
   No request body, header, or metadata leaves your VPC. No cross-border data transfer. No shared-tenancy processing. Required for MiFID II and SEC record-keeping.

4. **Structured Audit Trail for Examiners**  
   Every verdict generates a WORM-compatible log entry: timestamp, policy ID, match condition, verdict, and request hash. Direct export to compliance repositories.

5. **Engineered for Financial Services Risk Tolerance**  
   Built by infrastructure engineers from trading technology backgrounds. No "AI magic." No probabilistic safety. Deterministic policy evaluation with measurable coverage.

---

## Financial Services: Risk Addressed

### Section Headline
Regulatory Exposure. Eliminated.

### Pain Point: Unstructured Risk
Financial institutions deploying LLMs face SEC, FINRA, and MiFID II scrutiny without structured controls. Ad-hoc prompt filtering creates liability gaps. SaaS guardrails introduce data sovereignty conflicts.

### Trace Solution
- **Prompt Injection Prevention**: Pattern matching and semantic analysis block instructions designed to override system prompts or extract unauthorized outputs.
- **PII Containment**: Regex and NER-based detection prevents SSNs, account numbers, ISIN codes, and client identifiers from reaching third-party LLMs.
- **Unauthorized Trading Signal Prevention**: Block prompts that could generate actionable trading recommendations outside approved compliance workflows.
- **Position and Order Data Protection**: Ensure no proprietary position data, order intent, or strategy signals leave the firm via LLM prompts.
- **Examiner-Ready Audit Trail**: Structured logs with policy versioning, operator attribution, and tamper-evident hashing for regulatory examination.

---

## Social Proof: Testimonial Placeholders

### Block 1
> "Trace replaced three separate SaaS guardrail contracts. Our compliance team now has a single source of truth for every LLM interaction. The audit trail alone justified the migration."  
> — **Chief Risk Officer**, Global Asset Manager  
> AUM: $800B

### Block 2
> "We evaluated five solutions. Trace was the only one that met our p50 latency requirements for real-time trading applications. Sub-15ms is not a marketing claim—it is our production reality."  
> — **Head of ML Platform Engineering**, Bulge-Bracket Investment Bank  
> Regulatory Jurisdiction: SEC, FINRA, FCA

### Block 3
> "The Policy Studio let our legal and compliance teams verify policy coverage without engineering tickets. That operational separation is now a documented control in our SOC 2 Type II audit."  
> — **CISO**, Institutional Trading Firm  
> Deployment: Air-gapped VPC

---

## Footer CTA

### Headline
Deploy Trace in Your VPC This Quarter

### Subheadline
Technical demo includes live policy evaluation, latency benchmarking, and compliance documentation review.

### CTA Button
[Schedule Demo with Solutions Engineer]

---

# 2. TRACE PRODUCT PAGE

---

## Hero Section

### Product Name
Trace

### One-Liner
Deterministic LLM request governance at wire speed.

### CTA
[Download Open Core]  
[Request Enterprise Brief]

---

## Architecture Overview

Trace operates as an inline HTTP proxy between application clients and LLM providers. The architecture consists of four primary components: the Ingress Handler, the Trajectory Evaluation Engine, the Policy Store, and the Verdict Applicator.

The Ingress Handler accepts HTTP/1.1 and HTTP/2 requests, normalizes headers, and performs zero-copy payload parsing. Parsed requests are passed to the Trajectory Evaluation Engine, which executes policy evaluation against active rulesets. The Policy Store maintains a lock-free, hot-reloadable policy namespace with versioned configuration. The Verdict Applicator executes the determined action—forwarding to upstream LLM, returning a 403 rejection, or applying a payload transformation.

All components are written in Rust and compiled to a single static binary. No container orchestration is required, though Kubernetes deployment is supported. The entire system runs within the customer's network boundary with no external API dependencies.

---

## Verdict Types: Deep Dive

### Pass
Requests matching no block or modify conditions proceed unmodified to the upstream LLM. The Pass verdict is the default state. Pass decisions are logged with request metadata, policy namespace, and timestamp for audit purposes. No latency penalty beyond the 15ms evaluation ceiling.

### Block
Requests matching defined block conditions return HTTP 403 immediately. The upstream LLM receives no data. Block conditions typically include: prompt injection signatures, unauthorized PII patterns, prohibited topic classifications, or custom logic returning a deny decision. Block responses include a structured error body with policy ID and match condition reference for debugging and audit correlation.

### Modify
Requests matching modify conditions are transformed before upstream forwarding. Modifications include: PII redaction (masking or token replacement), prompt injection sanitization, payload schema enforcement, or custom transformations via Wasm sandbox. Modified requests carry an audit trail indicating the transformation applied and the policy version responsible.

---

## The Trajectory Evaluation Engine

The Trajectory Evaluation Engine is the core policy execution subsystem. It implements a three-tier evaluation strategy designed to minimize latency while maximizing coverage.

### Fast Path: Regex and Aho-Corasick
The first evaluation tier applies compiled regular expressions and Aho-Corasick multi-pattern string matching. These operations execute in microsecond timeframes and catch high-confidence violations: known prompt injection prefixes, hard-coded PII patterns, and explicitly prohibited terms.

### Vector Path: Semantic Intent Classification
For semantic evaluation, the engine computes request embeddings using SIMD-accelerated cosine similarity against a precomputed vector cache. This tier catches conceptually similar requests that evade keyword matching—paraphrased injection attempts, topic drift, or intent shift. Vector cache updates are hot-reloadable without restart.

### Sandbox Path: Custom Wasm Logic
For domain-specific evaluation, the engine executes customer-provided logic in a sandboxed WebAssembly runtime. Wasm modules receive the parsed request context and return a verdict. The sandbox is resource-constrained: 10ms execution ceiling, 64MB memory limit, no network access. Custom logic enables complex rules: checking against internal databases, evaluating request context against proprietary risk models, or integrating with existing authorization systems.

---

## Policy Studio

Policy Studio is the browser-based interface for policy management, traffic monitoring, and compliance verification. It connects to the Trace Policy Store API and provides real-time visibility into evaluation decisions.

### Live Traffic Monitoring
The Verdict Stream displays incoming requests, applied policies, and resulting verdicts in real time. Compliance and risk teams observe system behavior without engineering access. Filters by namespace, verdict type, and time range enable targeted monitoring.

### Policy Editor
Policies are authored in a structured interface with regex testing, semantic threshold tuning, and Wasm module upload. Changes are validated against the active schema before deployment. Policy versioning maintains history with operator attribution.

### Coverage Verification
The Policy Studio surfaces metrics: match rate by policy, evaluation latency distribution, and verdict breakdown. Coverage gaps are flagged for remediation. This visibility is documented as a control in SOC 2 and regulatory examinations.

---

## Observability

Trace exports OpenTelemetry traces for every evaluation. Each request generates a span containing: ingress timestamp, parse duration, evaluation phase breakdown, verdict decision, and applied action. Traces are compatible with Jaeger, Datadog, Honeycomb, and other OTel-compliant backends.

Metrics export in Prometheus format. Key indicators include: request rate, verdict distribution, evaluation latency percentiles, policy match rate, and Wasm sandbox execution time. Alerting rules can be configured for anomaly detection: latency spikes, unexpected block rate changes, or policy evaluation errors.

Log output is structured JSON with configurable fields. Default includes: timestamp, trace ID, policy namespace, verdict, policy ID, and match condition. Logs can be streamed to SIEM systems, compliance repositories, or WORM storage for regulatory retention.

---

## Multi-Tenancy

Trace implements policy namespace isolation via the `x-customer-id` HTTP header. Incoming requests are routed to the corresponding policy namespace, ensuring complete separation between organizational units, product lines, or client environments.

Each namespace maintains:
- Independent policy rulesets
- Isolated evaluation caches
- Separate audit logs
- Dedicated metrics labels

Namespace boundaries are enforced at the evaluation engine level. No cross-namespace data leakage is possible. This architecture supports service provider deployments, internal platform teams, and segregated compliance environments.

Growth tier includes 5 policy namespaces. Enterprise VPC tier includes unlimited namespaces with RBAC enforcement in Policy Studio.

---

## Financial Services Use Cases

### Use Case 1: Retail Client Service Automation
A bulge-bracket bank deploys LLM-powered chat for retail client inquiries. Trace blocks prompts attempting to extract position data, order history, or account numbers. PII regex patterns prevent SSN and account number transmission to external LLMs. The audit trail documents every blocked attempt for FINRA examination.

### Use Case 2: Research Analyst Assistant
An asset manager provides LLM tools to equity research analysts. Trace semantic classification prevents prompts requesting unauthorized trading recommendations outside approved compliance workflows. Wasm sandbox integration checks analyst entitlements against the firm's authorization system before permitting sensitive queries.

### Use Case 3: Trading Desk Communication Monitoring
An institutional trading firm monitors LLM interactions from trading desk terminals. Trace detects prompt injection attempts designed to override safety instructions or extract proprietary strategy signals. Block decisions are logged with millisecond precision for post-trade surveillance correlation.

### Use Case 4: Cross-Border Regulatory Compliance
A global bank with US and EU operations routes LLM traffic through region-specific Trace deployments. Data residency is enforced: EU client data never leaves EU infrastructure. WORM-compatible audit logs are exported to regional compliance repositories. Policy namespaces segregate US and EU rule sets to address jurisdictional differences.

---

# 3. PRICING PAGE

---

## Pricing Framing

Stria Systems prices Trace by deployment model and support tier, not by API call volume. We do not tax your LLM usage. Our pricing reflects the infrastructure reality: deploying Trace requires engineering effort, compliance review, and ongoing maintenance. We align our commercial model with your operational success.

Open Core provides full evaluation engine access for engineers evaluating the technology. Growth adds the operational interfaces required for production deployment. Enterprise VPC includes the security, compliance, and support infrastructure required by regulated financial institutions.

---

## Pricing Tiers

| | Open Core | Growth | Enterprise VPC |
|---|---|---|---|
| **Deployment** | Single binary, Docker image | Kubernetes Helm charts, Terraform modules | Air-gapped, SOC 2 Type II documented |
| **Policy Studio** | Not included | Full UI included | Full UI with SAML/SSO, RBAC |
| **Multi-Tenancy** | Single namespace | Up to 5 namespaces | Unlimited namespaces |
| **Observability** | stdout logs | OTel + Prometheus + Grafana dashboards | SIEM integration, WORM log export |
| **Support** | GitHub issues | Email support, 24-hour response | Dedicated deployment engineer, 4-hour response, 99.99% SLA |
| **Compliance** | Self-attestation | Self-attestation | Signed documentation package, audit support |
| **Pricing** | Free | $2,500/month | Custom quote |

---

## Enterprise VPC: Included Services

Enterprise VPC deployments include dedicated professional services and operational support designed for regulated financial infrastructure.

### Deployment Engineering
A Stria Systems deployment engineer is assigned to your implementation. Responsibilities include: architecture review, network configuration validation, policy migration assistance, and production cutover support. The deployment engineer remains your technical contact through initial production deployment and quarterly reviews.

### Compliance Documentation
Enterprise VPC includes a SOC 2 Type II documentation package with signed attestations for security, availability, and confidentiality controls. Additional compliance mappings are available for PCI-DSS, GDPR, and MiFID II upon request. Audit support includes direct response to examiner questions and evidence provision.

### Air-Gapped Installation
For firms requiring complete network isolation, Trace deploys without external connectivity. Container images, policy bundles, and documentation are delivered via secure transfer. Update mechanisms operate through manual or controlled distribution channels. No telemetry, license verification, or external dependency is required.

### Signed Audit Log Export
Verdict logs export in WORM-compatible format with cryptographic signing. Export destinations include: AWS S3 with Object Lock, Azure Immutable Blob Storage, on-premise NAS with hardware WORM enforcement. Log integrity is verifiable via published hash chains.

### Uptime SLA
99.99% availability SLA with financial credits for downtime exceeding threshold. SLA measurement includes Policy Store availability and evaluation engine responsiveness. Monitoring and alerting are configured during deployment engineering engagement.

---

## Frequently Asked Questions

### Data Residency
**Q: Where does request data reside during evaluation?**  
A: Entirely within compute instances you provision in your VPC or on-premise data center. No request content, metadata, or derived data is transmitted to Stria Systems infrastructure or third parties.

### Compliance Certifications
**Q: What compliance certifications does Trace hold?**  
A: Stria Systems maintains SOC 2 Type II attestation for the Enterprise VPC deployment option. The attestation covers security, availability, and confidentiality trust services criteria. Compliance documentation package is available to Enterprise VPC customers under NDA.

### Air-Gap Deployment
**Q: Can Trace operate in a fully air-gapped environment?**  
A: Yes. Enterprise VPC supports deployment with zero external network connectivity. All components, including Policy Studio static assets, are self-hosted. License verification is handled via offline mechanism. Updates are delivered through controlled channels.

### SLA Terms
**Q: What is the uptime SLA and what are the credit terms?**  
A: Enterprise VPC includes a 99.99% uptime SLA measured monthly. Downtime exceeding 0.01% of measured minutes qualifies for service credits: 10% of monthly fee for 99.9%–99.99%, 25% for 99.5%–99.9%, 50% for 99.0%–99.5%, 100% for below 99.0%. SLA excludes scheduled maintenance and customer-caused outages.

### Migration from SaaS Guardrails
**Q: How does migration from existing SaaS guardrail providers work?**  
A: Stria Systems provides migration tooling for common SaaS guardrail policy formats. Migration path includes: parallel evaluation (running Trace alongside existing provider), policy coverage comparison, phased cutover by traffic segment, and decommissioning of SaaS dependency. Migration support is included in Enterprise VPC deployment engineering.

---

## Final CTA

### Headline
Enterprise Deployment Evaluation

### Subheadline
Schedule a 45-minute technical demonstration covering: live policy evaluation, latency benchmarking on your workload profile, compliance documentation review, and migration planning.

### CTA Button
[Schedule Enterprise Demo]

### Secondary Link
[Download Open Core for Evaluation]

---

*Stria Systems, Inc. — Deterministic infrastructure for regulated environments.*
