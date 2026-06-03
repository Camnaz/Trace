# Stria Systems — Trace: Enterprise Pitch Deck Outline

**Document classification:** Internal — Sales & Marketing  
**Version:** 1.0  
**Last updated:** [DATE]  
**Intended audience:** Enterprise sales team, executive leadership  
**Target buyer personas:** CRO, CISO, ML/Platform Engineering Lead — bulge-bracket banks, major fintechs, asset managers  

---

## Deck Structure

| Act | Slides | Purpose |
|-----|--------|---------|
| I — The Unmapped Risk | 1–5 | Establish unguarded LLM deployments as a balance-sheet and regulatory risk |
| II — The Structural Requirement | 6–8 | Define what a compliant control framework demands |
| III — Trace | 9–15 | Introduce Trace as the engineered response to that requirement |
| IV — Enterprise & Commercial | 16–19 | De-risk the procurement decision |
| V — Close | 20–22 | Specific ask and next steps |

---

## ACT I: THE UNMAPPED RISK

### Slide 1 — Title / Cover

**Slide title:**  
Stria Systems — Trace  
*Deterministic Governance for Every LLM Request*

**Visual / layout direction:**  
Full-bleed dark background. Stria Systems wordmark top-left, "Trace" in a restrained sans-serif beneath it. Bottom-right: presenter name placeholder, title, date, and "CONFIDENTIAL" designation. No imagery. No gradients. The slide should communicate institutional discipline, not startup energy.

**Speaker notes:**  
- "Thank you for making time. I know you see dozens of vendor decks a quarter, so I'll be direct about what this meeting is and isn't."
- "This is not a demo of an AI safety product. This is a conversation about a specific infrastructure gap in how your firm governs LLM-bound requests — and what closing that gap looks like architecturally."
- "We'll spend the first few minutes on the risk landscape — not to tell you things you don't know, but to make sure we're calibrated on the same regulatory surface area."
- "Then we'll walk through what we've built, why we built it the way we did, and what a pilot engagement looks like."
- "I'll keep this to [X] minutes and leave time for your questions."

**Key message:**  
This is an infrastructure conversation about regulatory and operational risk — not an AI safety pitch.

---

### Slide 2 — The AI Deployment Reality at Financial Institutions

**Slide title:**  
LLM Adoption Is Outpacing Every Prior Technology Cycle in Financial Services

**Visual / layout direction:**  
Left side: a single vertical bar chart or timeline showing adoption velocity — internal copilots, client-facing summarization, research augmentation, trade idea generation — with deployment counts or percentage adoption figures. Right side: three to four pull-quotes from public earnings calls or regulatory filings by tier-1 financial institutions acknowledging production LLM deployments. Clean, data-forward. No stock photos.

**Speaker notes:**  
- "By most estimates, over 75% of large financial institutions have LLM workloads in production or advanced piloting as of early 2025 — across research, operations, client engagement, and compliance functions."
- "The deployment velocity is unprecedented. Cloud migrations took years. LLM integrations are reaching production in weeks — often initiated by individual teams, not centrally orchestrated."
- "What makes this different from prior technology adoption cycles is that LLMs produce unstructured, non-deterministic output that is consumed directly by clients, counterparties, and internal decision-makers."
- "The question is not whether your firm is deploying LLMs. The question is whether the control framework around those deployments matches the risk profile of the output they produce."
- "In most institutions we speak with, the honest answer is: not yet."

**Key message:**  
LLMs are being deployed faster than any prior technology in financial services, and the control infrastructure has not kept pace.

---

### Slide 3 — The Governance Gap

**Slide title:**  
The Gap Between Deployment Velocity and Control Framework Coverage

**Visual / layout direction:**  
A simple two-line divergence chart. X-axis: time (2023–2025). Line 1 (ascending steeply): "LLM workloads in production." Line 2 (ascending slowly): "Workloads covered by formal AI governance controls." The widening gap between the lines is shaded and labeled "Uncontrolled exposure." Below the chart, three bullet points listing what "formal governance controls" means: pre-request policy enforcement, immutable audit trail, deterministic verdict on every output.

**Speaker notes:**  
- "This is the structural problem. Your firm likely has an AI governance policy. You may have an AI risk committee. You almost certainly have model risk management frameworks under SR 11-7."
- "But governance policies and governance controls are not the same thing. A policy says 'LLM outputs must not contain unauthorized investment advice.' A control is the infrastructure that enforces that policy on every request, in real time, with an audit record."
- "In most institutions, the policy exists. The control does not. The gap between those two is where regulatory exposure lives."
- "This is not an indictment — it's a timing problem. The technology moved faster than the control infrastructure could be built. Our thesis is that this gap must be closed in 2025, and that it requires purpose-built infrastructure to do so."

**Key message:**  
Most financial institutions have AI governance policies but lack the real-time enforcement infrastructure to make those policies operative — that gap is where regulatory and balance-sheet exposure accumulates.

---

### Slide 4 — Regulatory Exposure Mapping

**Slide title:**  
What "Unsupervised LLM Output" Means Under Current Regulatory Frameworks

**Visual / layout direction:**  
A four-row table, each row representing a regulatory framework. Columns: Framework | Relevant Provision | What It Requires | Exposure from Unsupervised LLM Output. Frameworks: SEC/FINRA Rule 17a-4, Federal Reserve SR 11-7, MiFID II Article 25, EU DORA. Each row should be dense but legible. This is a reference slide — the audience should want to photograph it.

**Speaker notes:**  
- "Let me map this concretely. Under SEC Rule 17a-4 and FINRA recordkeeping requirements, all communications related to the firm's business — including those generated or augmented by an LLM — must be retained in immutable, auditable format. If an LLM produces output that constitutes a business communication and you have no audit trail of what was generated, when, and under what policy, you have a recordkeeping violation."
- "Under SR 11-7, any model used in a decision-making capacity — and LLMs used for research synthesis, trade idea generation, or client communication clearly qualify — must have documented controls, ongoing monitoring, and independent validation. An LLM with no pre-output policy evaluation is an uncontrolled model under SR 11-7."
- "MiFID II Article 25 requires that investment advice, including algorithmically generated advice, be suitable, documented, and auditable. An LLM that drifts into investment advice language without detection creates an Article 25 exposure."
- "DORA — the EU Digital Operational Resilience Act — requires ICT risk management frameworks that include third-party risk controls. LLM API providers are ICT third parties. Sending unfiltered prompts containing client data to those providers without a control layer is a DORA gap."
- "None of these are hypothetical. These are the frameworks your compliance team is already operating under. The question is whether LLM workloads are inside or outside of those frameworks today."

**Key message:**  
Unsupervised LLM output creates specific, documentable exposure under SEC/FINRA Rule 17a-4, SR 11-7, MiFID II Article 25, and DORA — not in theory, but under the frameworks your firm is already regulated by.

---

### Slide 5 — The Attack Surface

**Slide title:**  
Four Scenarios Your Firm Cannot Afford to Discover in Production

**Visual / layout direction:**  
Four vertically stacked scenario panels, each with a short title, a one-sentence scenario description, and a one-sentence consequence. Subtle red/amber severity indicators. No illustrations — text only, formatted for gravity. Think incident report, not infographic.

**Speaker notes:**  
- "**Scenario 1 — Unauthorized Trading Language.** A portfolio manager uses an internal LLM copilot for research summarization. The model's response includes the phrase 'based on current momentum indicators, we recommend increasing exposure to [TICKER].' That language, if forwarded to a client or logged in a CRM, constitutes an unauthorized trading signal. The firm now has a supervisory failure under FINRA Rule 3110."
- "**Scenario 2 — PII Exfiltration via Prompt.** A client services associate pastes a client's Social Security number, account number, and full name into an LLM prompt to generate a client summary letter. That data is now transmitted to a third-party API provider, potentially stored in training data, and outside the firm's data residency perimeter. This is a data breach under multiple frameworks."
- "**Scenario 3 — Investment Advice Boundary Erosion.** A research analyst uses an LLM to draft a market outlook. The model generates language that crosses from descriptive analysis into prescriptive investment advice — 'investors should consider reallocating to fixed income given the current yield environment.' If published, this is unregistered investment advice."
- "**Scenario 4 — Prompt Injection Producing Adversarial Output.** An external-facing LLM application receives a crafted prompt injection that causes the model to generate content contradicting the firm's official positions, disclosing internal risk thresholds, or producing fabricated regulatory citations. The firm's brand and regulatory standing are exposed."
- "Each of these scenarios is preventable with a policy enforcement layer that evaluates every request before it reaches the model and every response before it reaches the user. Without that layer, these are not 'if' scenarios — they are 'when' scenarios."

**Key message:**  
The attack surface is not abstract — it is unauthorized trade language, PII exfiltration, investment advice boundary erosion, and prompt injection, each carrying specific regulatory and reputational consequences.

---

## ACT II: THE STRUCTURAL REQUIREMENT

### Slide 6 — What a Compliant AI Control Framework Requires

**Slide title:**  
Five Non-Negotiable Properties of a Production AI Governance Control

**Visual / layout direction:**  
Five rows, each with an icon (lock, clock, checkmark, ledger, gears), a property name, and a one-sentence definition. Generous whitespace. This slide should read like an engineering specification, not a marketing claim. Consider a numbered list format that visually echoes a requirements document.

**Speaker notes:**  
- "Before we talk about our product, let's agree on what any compliant solution must do. These aren't our requirements — these are the requirements implied by the regulatory frameworks we just reviewed."
- "**1. Data residency.** The control must operate within your perimeter. Sending LLM request content to an external SaaS API for policy evaluation means the control layer itself becomes a data egress point. That's not a control — that's a new risk."
- "**2. Latency neutrality.** The control must not degrade the user experience or the operational performance of LLM-dependent workflows. If your policy evaluation adds 200–500ms of latency, engineering teams will route around it. A control that gets bypassed is worse than no control — it creates a false sense of coverage."
- "**3. Deterministic verdicts.** Every request must receive one of three outcomes: Pass, Block, or Modify. The verdict must be the product of explicit, auditable policy logic — not a probabilistic model. Regulators do not accept 'the model thought it was probably fine.'"
- "**4. Immutable audit trail.** Every verdict, every policy evaluation, every request and response must be logged in a format that is tamper-evident, exportable, and compatible with your firm's recordkeeping infrastructure. This is not optional under 17a-4."
- "**5. Policy governance.** Policies must be versionable, testable, reviewable, and deployable through a controlled change management process — just like any other production risk control. Ad hoc configuration is not governance."

**Key message:**  
Any production-grade AI governance control must deliver data residency, latency neutrality, deterministic verdicts, an immutable audit trail, and formal policy governance — these are not preferences, they are regulatory requirements.

---

### Slide 7 — Why Existing Approaches Fail

**Slide title:**  
Current Alternatives Against the Five Requirements

**Visual / layout direction:**  
A comparison matrix. Rows: SaaS Guardrail Platforms (e.g., Guardrails.ai, Nemo Guardrails), LLM Provider Safety Features (OpenAI moderation, Anthropic constitutional AI), Manual Review / Human-in-the-Loop, Build In-House. Columns: Data Residency, Latency Neutrality (<50ms), Deterministic Verdicts, Immutable Audit Trail, Policy Governance. Cells marked with clear pass/fail indicators. Trace is not on this chart — it will appear on the next act. This slide should make the audience realize nothing currently satisfies all five.

**Speaker notes:**  
- "**SaaS guardrail platforms** — Guardrails.ai, NVIDIA NeMo Guardrails, and similar offerings — require your LLM request content to be sent to an external API for evaluation. That violates data residency. They also add 200–800ms of latency per evaluation because they often invoke their own LLM for classification. They fail on two of the five requirements before you evaluate anything else."
- "**LLM provider safety features** — OpenAI's moderation endpoint, Anthropic's constitutional AI, Bedrock Guardrails — operate on the provider side. Your data has already left the perimeter before the control is applied. You have no visibility into the policy logic, no control over the verdicts, and no audit trail that you own. These are provider risk controls, not your risk controls."
- "**Manual review and human-in-the-loop** — cannot scale. If your firm processes thousands of LLM requests per hour, human review introduces latency measured in minutes or hours, creates bottlenecks, and still depends on the reviewer's judgment rather than deterministic policy."
- "**Building in-house** — is the most intellectually honest option, but the engineering cost is substantial. A high-performance proxy with sub-15ms evaluation, a lock-free policy store, WASM sandboxing for custom logic, and production-grade observability is 6–18 months of senior Rust engineering time. And then you maintain it forever."
- "None of these options satisfy all five requirements simultaneously. That is not a market criticism — it is the reason we built Trace."

**Key message:**  
No existing approach — SaaS guardrails, provider safety features, manual review, or building in-house — satisfies all five requirements of a compliant AI governance control simultaneously.

---

### Slide 8 — The Design Constraint

**Slide title:**  
The Hard Constraint: Any Solution That Adds >50ms or Requires Data Egress Is Operationally Non-Viable

**Visual / layout direction:**  
A single, stark statement centered on the slide in large typography. Below it, a latency comparison bar chart: "Typical SaaS guardrail evaluation: 200–800ms" vs. "Acceptable overhead for production LLM request governance: <15ms." The visual disparity should be immediately obvious — one bar should be 15–50x longer than the other.

**Speaker notes:**  
- "This is the constraint that eliminates most of the market. It's not a preference — it's an operational reality."
- "If your LLM request governance layer adds more than 50ms, your engineering teams will find ways to bypass it. They will hard-code exceptions. They will route traffic around the proxy. The control degrades from a governance layer to a suggestion."
- "If your governance layer requires data to leave the perimeter — even to a 'trusted' SaaS vendor — you have created a new data egress path that must itself be governed, audited, and risk-assessed. You have not reduced risk. You have moved it."
- "The only architecturally sound approach is a governance layer that runs inside your perimeter, evaluates every request in single-digit milliseconds, and requires zero external API calls in the critical path."
- "That is what we designed Trace to be."

**Key message:**  
The solution must add less than 15ms of latency and must never send data outside the firm's perimeter — any approach that violates either constraint will be operationally circumvented or will itself become a risk vector.

---

## ACT III: TRACE

### Slide 9 — Introducing Trace

**Slide title:**  
Trace — Deterministic Policy Enforcement for Every LLM Request

**Visual / layout direction:**  
Center of slide: a clean, horizontal architecture diagram. Three elements in a line: [Client Application] --HTTP--> [Trace Proxy] --HTTP--> [LLM Provider / Self-Hosted Model]. Below the Trace Proxy box, three verdict badges: PASS (green), BLOCK (red), MODIFY (amber). Above the Trace Proxy box, a single line: "< 15ms. Zero data egress. Deterministic." No other text. Let the diagram speak.

**Speaker notes:**  
- "Trace is an ultra-low-latency HTTP proxy that intercepts every LLM-bound request before it reaches the model provider. It evaluates each request against a deterministic policy set and returns one of three verdicts: Pass, Block, or Modify."
- "It is written in Rust, deployed on-prem or in your VPC, and makes zero external API calls in the evaluation path. It is fully air-gappable."
- "The integration model is a one-line proxy redirect. Your application points its LLM HTTP client at Trace instead of directly at the provider. Trace handles the evaluation and forwards compliant requests transparently."
- "Every evaluation completes within a 15ms hard ceiling. This is not a benchmark average — it is a design constraint enforced by the architecture."
- "This is not a wrapper, a SDK, or a middleware library. It is a standalone infrastructure component — a proxy — that sits in the network path and governs traffic."

**Key message:**  
Trace is a Rust-based HTTP proxy that sits between your applications and LLM providers, enforcing deterministic policies on every request in under 15ms with zero data egress.

---

### Slide 10 — The 15ms Guarantee

**Slide title:**  
Latency Budget Breakdown: Engineering Proof, Not Marketing Claim

**Visual / layout direction:**  
A horizontal stacked bar chart showing the 15ms budget decomposition. Four segments, each labeled with the phase and its allocation: Ingress & TLS termination (2ms), Request parse & policy routing (3ms), Policy evaluation — all layers (8ms), Verdict application & forwarding (2ms). Total: 15ms. Below the bar, a footnote: "P99 measured under sustained 10k req/s load. No external API calls. No model inference. Deterministic path only."

**Speaker notes:**  
- "Let me break down how we achieve the 15ms ceiling, because this is the most important engineering claim we make and I want you to understand why it holds."
- "**Ingress — 2ms.** TLS termination and connection handling. Trace uses Rust's async runtime with zero-copy buffer management. There is no garbage collection pause. There is no thread pool contention."
- "**Parse & Route — 3ms.** The incoming HTTP request is parsed, the tenant namespace is resolved from the x-customer-id header, and the applicable policy set is loaded from the lock-free policy store. Policy store reads are wait-free — there is no mutex, no lock acquisition, no contention under concurrent load."
- "**Evaluation — 8ms.** This is the core. Every request passes through up to three evaluation layers — regex/keyword fast path, semantic vector cache, and optional Wasm sandbox for custom logic. Critically, none of these layers make external API calls. The semantic cache uses pre-computed embeddings. The Wasm sandbox runs compiled customer logic in an isolated runtime. Everything is local."
- "**Verdict & Forward — 2ms.** The verdict is applied — pass-through, block with structured error response, or request modification — and the request is forwarded to the upstream LLM provider. The verdict is emitted as an OpenTelemetry span simultaneously."
- "This is not a tuned benchmark. This is the architectural ceiling. We cannot exceed 15ms because the design does not permit operations that would cause us to exceed it."

**Key message:**  
The 15ms latency ceiling is a structural property of the architecture — no external calls, no locks, no inference — not a benchmark optimized for a demo.

---

### Slide 11 — The Trajectory Engine

**Slide title:**  
Three Evaluation Layers — Zero External Dependencies

**Visual / layout direction:**  
A vertical funnel or cascade diagram showing three layers, top to bottom. Layer 1 (widest, fastest): "Fast Path — Regex, Keyword, Content Length" — labeled "<1ms." Layer 2 (middle): "Vector Cache — Semantic Intent Matching" — labeled "~3ms." Layer 3 (narrowest): "Wasm Sandbox — Custom Logic" — labeled "~4ms." An arrow on the right side labeled "Short-circuit: first definitive verdict wins." A callout box: "No external model calls. No network I/O. All evaluation is local."

**Speaker notes:**  
- "The evaluation engine — what we call the Trajectory Engine — operates in three layers, and a request can short-circuit at any layer."
- "**Layer 1: Fast Path.** Regex pattern matching, keyword detection, content length enforcement, rate limiting. This catches the obvious cases — SSN patterns, ISIN numbers, banned terms, prompt length violations — in under 1ms. Most blocked requests never reach Layer 2."
- "**Layer 2: Vector Cache.** For policies that require semantic understanding — 'does this prompt ask for investment advice?' or 'does this response contain trading recommendations?' — we use a pre-computed vector similarity cache. Embeddings are generated at policy deployment time and stored locally. At evaluation time, we compute a lightweight embedding of the request content and compare it against the cached policy vectors. No external model call. No API roundtrip. Approximately 3ms."
- "**Layer 3: Wasm Sandbox.** For policies that require custom logic — complex business rules, multi-field validation, conditional evaluation chains — customers can deploy compiled WebAssembly modules that execute in an isolated sandbox. The Wasm runtime is memory-sandboxed, CPU-time-limited, and cannot make network calls. This is where institution-specific logic lives. Approximately 4ms."
- "The key architectural property: no layer requires any external network call. The entire evaluation path is local to the Trace process. This is what makes air-gapped deployment possible and what makes the 15ms ceiling credible."

**Key message:**  
Trace evaluates requests through three local-only layers — regex fast path, semantic vector cache, and Wasm sandbox — with no external dependencies, enabling deterministic sub-15ms verdicts.

---

### Slide 12 — Verdict Types and Policy Actions

**Slide title:**  
Three Verdicts. Every Request. No Exceptions.

**Visual / layout direction:**  
Three columns, each representing a verdict type. Column 1 — PASS (green header): icon, definition, finserv example. Column 2 — BLOCK (red header): icon, definition, finserv example. Column 3 — MODIFY (amber header): icon, definition, finserv example. Below the columns, a single line: "Every verdict is logged. Every verdict is auditable. There is no silent pass-through."

**Speaker notes:**  
- "Every request that passes through Trace receives exactly one of three verdicts. There is no ambiguity, no confidence score, no 'maybe.'"
- "**PASS** — the request complies with all applicable policies. It is forwarded to the LLM provider unmodified. Example: a research analyst submits a market data summarization prompt with no PII, no trading language, within rate limits. Trace forwards it transparently."
- "**BLOCK** — the request violates one or more policies. It is not forwarded. The client receives a structured error response indicating which policy was violated, without exposing internal policy details. Example: a prompt contains a client's Social Security number. Trace blocks the request, returns a policy violation code, and logs the event with the full policy evaluation chain."
- "**MODIFY** — the request can be made compliant through deterministic transformation. Trace modifies the request and forwards the modified version. Example: a prompt exceeds the maximum token length policy. Trace truncates the prompt to the policy-defined maximum and forwards the truncated version. Or: Trace redacts detected PII patterns and forwards the sanitized prompt."
- "Critically, MODIFY verdicts are deterministic transformations — not LLM-generated rewrites. The modification logic is defined in the policy. There is no secondary model call to 'fix' the request."
- "Every verdict — Pass, Block, or Modify — is emitted as an OpenTelemetry span and written to the audit log. There is no category of request that passes through Trace without a recorded verdict."

**Key message:**  
Every LLM request receives a deterministic Pass, Block, or Modify verdict — each logged, auditable, and explainable — with no silent pass-through and no probabilistic judgment.

---

### Slide 13 — Financial Services Policy Library

**Slide title:**  
Purpose-Built Policy Patterns for Financial Services Risk Scenarios

**Visual / layout direction:**  
Four policy cards arranged in a 2x2 grid. Each card: policy name, one-sentence description, example trigger, verdict type. Below the grid, a JSON snippet showing one policy definition (e.g., the PII leakage policy with regex patterns for SSN, ISIN, IBAN). The JSON should be real and syntactically correct — this audience will read it.

**Speaker notes:**  
- "Trace ships with a curated policy library for financial services. These are not generic content filters — they are policies designed for the specific risk scenarios we discussed earlier."
- "**Policy 1: Unauthorized Trading Signals.** Detects language that constitutes a buy/sell/hold recommendation, price target, or position sizing suggestion. Uses semantic vector matching against a curated corpus of FINRA-cited supervisory failure examples. Verdict: Block."
- "**Policy 2: PII Leakage Prevention.** Detects Social Security numbers, ISIN identifiers, IBAN account numbers, and other structured PII via regex pattern matching on the fast path. Verdict: Block or Modify (redact and forward)."
- "**Policy 3: Investment Advice Boundary Detection.** Detects when model output crosses from descriptive market commentary into prescriptive investment advice — 'investors should,' 'we recommend,' 'consider reallocating.' Uses semantic vector matching calibrated against SEC and FINRA guidance on the advice/commentary boundary. Verdict: Block."
- "**Policy 4: Prompt Length and Rate Guard.** Enforces maximum prompt token counts and per-user or per-application rate limits. Prevents runaway costs, resource exhaustion, and prompt stuffing attacks. Verdict: Block."
- "Every policy is defined as a JSON or YAML document, version-controlled, and deployable through the Policy Studio UI or via API. Here's what the PII leakage policy looks like in practice — [reference JSON on screen]."

```json
{
  "id": "finserv-pii-leakage-v1",
  "name": "PII Leakage Prevention",
  "version": "1.0.0",
  "layer": "fast_path",
  "match": {
    "type": "regex_any",
    "patterns": [
      "\\b\\d{3}-\\d{2}-\\d{4}\\b",
      "\\b[A-Z]{2}\\d{2}[A-Z0-9]{4}\\d{7}([A-Z0-9]?){0,16}\\b",
      "\\b[A-Z]{2}\\d{10,12}\\b"
    ]
  },
  "verdict": "block",
  "metadata": {
    "regulatory_reference": "SEC Rule 17a-4, GDPR Art. 5(1)(f)",
    "severity": "critical",
    "owner": "information-security@firm.com"
  }
}
```

**Key message:**  
Trace includes purpose-built, auditable policy patterns for the four highest-priority financial services risk scenarios — unauthorized trading signals, PII leakage, investment advice boundaries, and rate/length guards — each deployable as version-controlled policy-as-code.

---

### Slide 14 — Observability and Audit

**Slide title:**  
Every Request Is a Span. Every Verdict Is a Record.

**Visual / layout direction:**  
Left side: a mock OpenTelemetry trace waterfall showing a single LLM request with child spans for ingress, parse, each policy evaluation, verdict, and forwarding — with timing data on each span. Right side: a simplified audit log entry showing timestamp, request hash, tenant ID, policy IDs evaluated, verdict, latency, and a cryptographic signature. Bottom: logos/icons for Prometheus, Grafana, Splunk, and generic SIEM. Subtext: "WORM-compatible signed export."

**Speaker notes:**  
- "Observability is not an add-on in Trace. It is a core architectural property. Every request that passes through Trace produces a full OpenTelemetry trace with child spans for each processing phase."
- "This means your existing observability stack — Prometheus, Grafana, Datadog, Splunk, whatever you run — can ingest Trace telemetry natively. You get latency percentiles, verdict distribution, policy hit rates, and error rates without building any custom instrumentation."
- "For audit purposes, every verdict is written to a signed audit log. Each log entry includes: the request timestamp, a cryptographic hash of the request content, the tenant ID, the list of policies evaluated, the verdict, the evaluation latency, and a digital signature that makes the entry tamper-evident."
- "The audit log supports WORM-compatible export — Write Once, Read Many — which is the storage standard required by SEC Rule 17a-4 for business communication records. You can export these logs directly to your firm's compliant archival system."
- "The Policy Studio dashboard provides a live view of request volume, verdict distribution, policy performance, and anomaly detection. This is what your compliance team monitors. This is what your regulator wants to see when they ask 'how do you govern your AI outputs.'"

**Key message:**  
Trace provides native OpenTelemetry observability integrated with your existing monitoring stack and WORM-compatible, cryptographically signed audit logs that satisfy SEC Rule 17a-4 recordkeeping requirements.

---

### Slide 15 — Deployment Architecture

**Slide title:**  
VPC-Native. Air-Gappable. Zero Data Egress.

**Visual / layout direction:**  
A deployment diagram showing Trace inside a customer VPC boundary. Elements: customer applications on the left, Trace proxy in the center (inside the VPC boundary line), LLM providers on the right (outside the boundary, with arrows showing that only compliant requests cross the boundary). Inside the VPC: Policy Studio UI, OTel Collector, audit log storage. Callout box listing deployment options: single Rust binary, Docker container, Kubernetes Helm chart. A second callout: "Air-gap option: no internet connectivity required post-deployment."

**Speaker notes:**  
- "Trace is deployed inside your infrastructure — your VPC, your data center, your private cloud. There is no Stria-hosted component in the data path."
- "The deployment artifact is a single compiled Rust binary. No JVM. No Python runtime. No dependency chain to audit. Alternatively, we provide a minimal Docker container image and a Kubernetes Helm chart for orchestrated environments."
- "For air-gapped environments — and several of our design partners require this — Trace operates with zero internet connectivity after initial deployment. Policy updates are delivered via signed artifact bundles that can be transferred through your existing secure file transfer process. There is no phone-home, no telemetry exfiltration, no license server callback."
- "Data flow is unidirectional: your applications send requests to Trace, Trace evaluates and forwards to the LLM provider. At no point does request content, policy configuration, or evaluation data leave your perimeter to reach Stria Systems."
- "Your security team can verify this architecturally — there is no outbound network path to Stria infrastructure. We don't want your data. We want your governance layer to be structurally incapable of leaking it."

**Key message:**  
Trace deploys entirely within your infrastructure as a single binary with zero data egress to Stria Systems — fully air-gappable, with no external dependencies in the operational path.

---

## ACT IV: ENTERPRISE & COMMERCIAL

### Slide 16 — Enterprise Tier

**Slide title:**  
Enterprise Tier: Built for Regulated Infrastructure

**Visual / layout direction:**  
A feature grid with six items, each with an icon and a one-sentence description. Features: SAML/SSO integration, Role-Based Access Control (RBAC), multi-tenant namespace isolation, SOC 2 Type II compliance package, 99.99% uptime SLA, dedicated SRE and deployment engineering support. Clean, structured, no marketing language — formatted like a capabilities matrix in an RFP response.

**Speaker notes:**  
- "The Enterprise tier is designed for procurement by regulated institutions. Every feature in this tier exists because a compliance, security, or infrastructure team at a financial institution told us it was a requirement."
- "**SAML/SSO** — Trace integrates with your existing identity provider. No separate credentials. No shadow authentication system. Your IAM team controls access."
- "**RBAC** — Policy creation, policy deployment, audit log access, and system configuration are governed by role-based access controls. Your risk team can author policies. Your engineering team can deploy them. Your compliance team can read audit logs. Each role sees only what it should."
- "**Multi-tenant namespace isolation** — the x-customer-id header creates isolated policy namespaces. Different business units, different client segments, or different applications can operate under different policy sets without cross-contamination. This is the same architecture that allows us to serve multiple internal teams within a single deployment."
- "**SOC 2 Type II** — we provide a complete SOC 2 Type II compliance package, including our audit report, control descriptions, and evidence artifacts. Your vendor risk team gets what they need to approve Trace without a six-month assessment cycle."
- "**99.99% SLA** — four nines. Backed by service credits. Measured against your monitoring, not ours."
- "**Dedicated support** — Enterprise contracts include a named deployment engineer for initial integration and a named SRE for ongoing operational support."

**Key message:**  
The Enterprise tier delivers SAML/SSO, RBAC, multi-tenant isolation, SOC 2 Type II, 99.99% SLA, and dedicated engineering support — every feature exists because a regulated institution required it.

---

### Slide 17 — Integration Effort

**Slide title:**  
One-Line Integration. Not a Platform Migration.

**Visual / layout direction:**  
Top half: a before/after code snippet. Before: `client.base_url = "https://api.openai.com"`. After: `client.base_url = "https://trace.internal.firm.com"`. That's the integration. Bottom half: a three-phase timeline. Day 1: Deploy Trace, configure upstream provider. Week 1: Deploy first policy set, begin audit log collection. Month 1: Full policy coverage across all LLM workloads, compliance team trained on Policy Studio.

**Speaker notes:**  
- "I want to be very specific about what integration looks like, because every vendor says 'easy integration' and most of them are lying."
- "Trace is an HTTP proxy. Integration means changing the base URL of your LLM HTTP client from the provider's endpoint to Trace's endpoint. That is the entire code change. One line. One configuration update."
- "There is no SDK to install. There is no library to import. There is no wrapper to deploy. There is no agent to run on every node. Trace sits in the network path. If your application can make an HTTP request, it can use Trace."
- "The deployment timeline we've validated with design partners: Day 1, Trace is deployed and forwarding traffic in pass-through mode — all requests pass, all verdicts are logged, no blocking. This gives your team visibility into traffic patterns before any enforcement begins."
- "Week 1, you deploy your first policy set — typically PII leakage prevention and rate limiting. You observe the verdict distribution and tune thresholds."
- "Month 1, you have full policy coverage, your compliance team is using Policy Studio to monitor verdicts, and your audit log is feeding your SIEM. The total integration effort is measured in hours of engineering time, not weeks."

**Key message:**  
Trace integration is a one-line proxy redirect — no SDK, no library, no agent — with a validated Day 1 deploy, Week 1 first policies, Month 1 full coverage timeline.

---

### Slide 18 — Commercial Model

**Slide title:**  
Open Core. Transparent Pricing. No Per-Request Metering.

**Visual / layout direction:**  
Three-column pricing tier layout. Column 1 — Open Core (free): open-source on GitHub, core proxy + policy engine, community support. Column 2 — Growth (self-serve): managed SaaS, Policy Studio UI, standard support, usage-based pricing. Column 3 — Enterprise VPC (custom contract): VPC/on-prem deployment, air-gap option, SAML/RBAC/SOC 2, dedicated SRE, fixed annual fee. The Enterprise VPC column should be visually emphasized. A callout at the bottom: "Enterprise VPC is a fixed-fee infrastructure contract. No per-request charges. No variable cost exposure."

**Speaker notes:**  
- "Our commercial model is Open Core. The core Trace proxy and policy engine are open-source on GitHub. You can inspect every line of code. Your security team can audit the binary. There is no vendor lock-in at the code level."
- "The Growth tier is for teams that want the Policy Studio UI, managed hosting, and standard support. It's self-serve, usage-based, and designed for mid-market fintechs and smaller institutions."
- "The tier relevant to this conversation is Enterprise VPC. This is a fixed annual fee for a VPC-deployed or on-prem installation with all enterprise features — SAML, RBAC, multi-tenant, SOC 2, SLA, dedicated support."
- "Critically, Enterprise VPC is not per-request priced. There is no variable cost exposure. You are not paying more as your LLM usage grows. This is a fixed-fee infrastructure contract — like your SIEM license or your API gateway. Your CFO will understand this model."
- "We structure Enterprise contracts as annual commitments with a defined scope of deployment. Pricing is based on deployment footprint, not traffic volume."

**Key message:**  
Enterprise VPC is a fixed annual fee with no per-request metering — it is an infrastructure contract, not a consumption model, eliminating variable cost exposure as LLM usage scales.

---

### Slide 19 — Reference Architecture

**Slide title:**  
Trace in a Financial Services Production Stack

**Visual / layout direction:**  
A detailed but clean architecture diagram showing a realistic financial services deployment. Left side: Trading Platform, Research Portal, Client Advisory Tool, Internal Copilot — each with an arrow to a central load balancer. Center: Load Balancer → Trace Proxy Cluster (2–3 nodes for HA). Right side: Arrows from Trace to OpenAI API, Anthropic API, and Self-Hosted Model (Bedrock / vLLM). Below Trace: OTel Collector → Prometheus/Grafana + SIEM/Splunk + WORM Audit Archive. Above Trace: Policy Studio UI ← Risk/Compliance Team. Side callout: "Trace handles routing to multiple LLM providers. Single governance layer for all model traffic."

**Speaker notes:**  
- "This is what Trace looks like in production at a financial institution. Multiple applications — trading platforms, research tools, client advisory systems, internal copilots — all route their LLM traffic through a Trace proxy cluster."
- "Trace handles upstream routing to multiple LLM providers — OpenAI, Anthropic, Bedrock, or self-hosted models running on vLLM or similar. You don't need a separate governance layer per provider. Trace is the single control point for all model traffic."
- "The observability pipeline feeds your existing infrastructure. OpenTelemetry spans go to your OTel Collector, which routes to Prometheus/Grafana for operational dashboards and to your SIEM for security monitoring. Audit logs export to your WORM-compliant archive."
- "Your risk and compliance team interacts with Trace through the Policy Studio UI — authoring policies, reviewing verdicts, monitoring coverage. They do not need access to the underlying infrastructure."
- "For high availability, Trace deploys as a stateless cluster behind your existing load balancer. Policy state is replicated across nodes via the lock-free store. There is no single point of failure and no session affinity requirement."

**Key message:**  
Trace serves as the single governance layer for all LLM traffic across multiple applications and providers, integrating natively with existing observability, SIEM, and compliance infrastructure.

---

## ACT V: CLOSE

### Slide 20 — Proof Points and Validation

**Slide title:**  
Early Validation

**Visual / layout direction:**  
Three sections. Top: "Design Partners" — [X] institutions in active design partnership (logos or anonymized descriptions, e.g., "Top-10 US bank by AUM," "European electronic market maker"). Middle: "Performance Benchmarks" — key metrics in large type: <15ms P99 latency at 10k req/s sustained, 0 external API calls in evaluation path, <50MB memory footprint per instance. Bottom: "Compliance Readiness" — SOC 2 Type II [in progress / completed], air-gap deployment validated, WORM audit export validated with [archival vendor].

**Speaker notes:**  
- "We are in active design partnerships with [X] financial institutions, ranging from [anonymized description]. These partnerships are structured as co-development engagements where the institution's risk, compliance, and engineering teams shape the policy library and deployment architecture."
- "Our performance benchmarks are measured under realistic conditions — sustained 10,000 requests per second, mixed policy sets including all three evaluation layers, production-equivalent hardware. P99 latency remains below 15ms. We publish the benchmark methodology and invite independent validation."
- "The memory footprint of a Trace instance is under 50MB. This is a Rust binary with no runtime overhead, no garbage collector, and no dependency chain. It runs on minimal infrastructure."
- "Our SOC 2 Type II engagement is [in progress / completed as of DATE]. The air-gapped deployment model has been validated in [X] environments. WORM audit log export has been validated against [specific archival platforms]."
- "We are not asking you to be an early adopter of an unproven technology. We are asking you to evaluate a purpose-built infrastructure component that has been designed with and validated by institutions like yours."

**Key message:**  
Trace has been validated in design partnerships with financial institutions, with independently reproducible performance benchmarks and compliance readiness for regulated environments.

---

### Slide 21 — The Ask

**Slide title:**  
Next Step: 30-Day Proof of Concept

**Visual / layout direction:**  
A simple three-row structure. Row 1: "What we're proposing" — a 30-day POC deployment in your staging or production VPC. Row 2: "What we need from you" — a designated engineering contact, a designated compliance/risk contact, one LLM workload to instrument. Row 3: "What you get" — full Trace deployment, enterprise policy set, audit log integration, performance validation against your traffic. A clear "Schedule POC Kickoff" call-to-action. Presenter contact information.

**Speaker notes:**  
- "Here is what we'd like to do next. We're proposing a 30-day Proof of Concept deployment in your staging or production environment."
- "During the POC, we deploy Trace in your VPC, configure it against one or more of your LLM workloads, deploy the financial services policy set, and integrate the audit log with your existing archival infrastructure."
- "At the end of 30 days, you will have: independently verified latency data from your own traffic, a functioning audit trail that your compliance team has reviewed, and a documented policy evaluation against your specific risk scenarios."
- "What we need from you is straightforward: a designated engineering contact who can coordinate the deployment, a designated compliance or risk contact who can validate the policy set and audit trail, and at least one LLM workload to instrument."
- "This is not a free trial with a sales timer. This is a structured evaluation designed to give your team the evidence they need to make a procurement decision. We have a dedicated deployment engineer assigned to every POC."
- "I'd like to schedule the POC kickoff within the next two weeks. Who on your side would be the right person to coordinate?"

**Key message:**  
The next step is a structured 30-day POC in your environment — we deploy Trace, instrument one workload, and deliver independently verifiable latency, audit, and policy evaluation data.

---

### Slide 22 — Appendix Index

**Slide title:**  
Appendix — Available on Request

**Visual / layout direction:**  
A clean numbered list of appendix documents, each with a one-sentence description. This is a table of contents for deep-dive materials that can be shared after the meeting or used during due diligence. Include document classification markings (e.g., "Technical — under NDA").

**Speaker notes:**  
- "We have extensive supplementary materials available for your due diligence process. I won't walk through these now, but I want you to know they exist and are available immediately."
- "**Appendix A: Compliance Mapping Table** — a detailed matrix mapping each Trace capability to specific provisions in SR 11-7, SEC Rule 17a-4, MiFID II, and DORA."
- "**Appendix B: Objection Handling / FAQ** — structured responses to the most common questions we receive from CROs, CISOs, and engineering leaders."
- "**Appendix C: Competitive Displacement Matrix** — a scored comparison of Trace against SaaS guardrails, building in-house, and provider safety features across eight evaluation criteria."
- "**Additional deep-dive materials available:** benchmark methodology and reproduction guide, Wasm policy authoring guide, Policy Studio walkthrough, network architecture and threat model documentation."
- "We will send the full appendix package to your team after this meeting. Please don't hesitate to share it with your vendor risk, compliance, and security teams."

**Key message:**  
Comprehensive technical, compliance, and competitive due diligence materials are available immediately and will be provided to your evaluation team.

---

---

## APPENDIX A: Compliance Mapping Table

**Trace Capability → Regulatory Requirement Mapping**

| Trace Capability | SR 11-7 (Model Risk Management) | SEC/FINRA Rule 17a-4 (Recordkeeping) | MiFID II Article 25 (Suitability & Advice) | EU DORA (Digital Operational Resilience) |
|---|---|---|---|---|
| **Pre-request policy evaluation** | Satisfies requirement for "effective challenge" and independent control of model outputs (SR 11-7 §IV.C) | N/A | Supports suitability control by preventing unsupervised prescriptive output (Art. 25(2)) | Supports ICT risk management framework requirement for controls over third-party services (Art. 28) |
| **Deterministic Pass/Block/Modify verdicts** | Provides documented, reproducible control decisions required for model validation (SR 11-7 §IV.A) | N/A | Enables demonstration that advice boundaries are enforced programmatically (Art. 25(6)) | Demonstrates deterministic control in ICT service chain (Art. 9) |
| **Immutable signed audit log** | Satisfies ongoing monitoring and documentation requirements (SR 11-7 §IV.D) | Directly satisfies immutable recordkeeping requirement for business communications (17a-4(b)(4)) | Provides auditable record of advice/non-advice classification decisions (Art. 25(5)) | Satisfies ICT-related incident logging and evidence preservation (Art. 17) |
| **WORM-compatible export** | Supports examination and audit requirements (SR 11-7 §V) | Directly satisfies WORM storage requirement (17a-4(f)) | Supports regulatory audit access to historical records (Art. 25(6)) | Supports reporting obligations to competent authorities (Art. 19) |
| **PII leakage prevention (regex/semantic)** | Reduces data-related model risk from training data contamination (SR 11-7 §III.B) | Prevents creation of non-compliant records containing improperly handled PII | N/A | Satisfies data protection controls within ICT risk framework (Art. 9(4)) |
| **Unauthorized trading signal detection** | Directly addresses output risk control for models used in trading contexts (SR 11-7 §IV.C) | Prevents generation of unrecorded trade communications | Prevents unsupervised generation of buy/sell recommendations (Art. 25(2)) | N/A |
| **Investment advice boundary detection** | Controls for model output drift into unauthorized advisory functions (SR 11-7 §IV.B) | Ensures advisory-adjacent communications are properly classified and retained | Directly enforces the advice/information boundary (Art. 25(1)) | N/A |
| **Rate limiting & prompt length guards** | Supports operational risk controls for model resource consumption (SR 11-7 §III.C) | N/A | N/A | Supports capacity management and resilience testing requirements (Art. 11) |
| **Air-gapped deployment** | Supports model risk isolation requirements (SR 11-7 §III.A) | Supports data residency requirements for retained records | N/A | Directly supports ICT third-party risk management by eliminating external dependency (Art. 28) |
| **Multi-tenant namespace isolation** | Supports independent validation by isolating control environments (SR 11-7 §IV.E) | Supports organizational requirements for business unit record segregation | Supports client categorization and per-segment suitability controls (Art. 25(3)) | Supports proportionate ICT risk management across business functions (Art. 5) |
| **OpenTelemetry observability** | Enables continuous monitoring and reporting required for model oversight (SR 11-7 §IV.D) | Supports supervisory review of communication patterns | N/A | Supports ICT monitoring and anomaly detection requirements (Art. 10) |
| **Policy versioning & governance** | Directly satisfies change management and documentation requirements for model controls (SR 11-7 §IV.A) | Supports demonstration that recordkeeping controls are current and maintained | Supports demonstration that suitability controls are current (Art. 25(6)) | Satisfies ICT change management requirements (Art. 9(4)(e)) |
| **RBAC and SAML/SSO** | Supports access control and segregation of duties for model risk functions (SR 11-7 §IV.E) | Supports access controls over record modification | N/A | Satisfies ICT access control and identity management requirements (Art. 9(4)(c)) |
| **SOC 2 Type II compliance package** | Supports vendor due diligence for third-party model risk tools (SR 11-7 §V) | Supports vendor assessment for recordkeeping infrastructure | N/A | Directly supports ICT third-party risk assessment requirements (Art. 28(4)) |

---

## APPENDIX B: Objection Handling Guide

### Objection 1: "We're building this in-house."

**Response framework:**
- "That's the most intellectually honest alternative, and we respect it. Let us share what we've learned from institutions that started down that path."
- The core proxy and policy engine — written in Rust for the performance guarantees required — represents 6–18 months of senior systems engineering effort. This is not application code; it's infrastructure code with hard latency constraints, lock-free concurrency, and memory safety requirements.
- Beyond the initial build, there is ongoing maintenance: policy engine updates, new evaluation layer development, security patching, performance regression testing, observability integration, and compliance documentation.
- The Wasm sandbox runtime, the semantic vector cache, and the WORM-compliant audit log are each non-trivial subsystems. Most teams underestimate the effort required for the audit trail alone.
- "Our suggestion: evaluate Trace against your internal build timeline. If your team can deliver equivalent capabilities faster and cheaper, you should build it. If not, Trace gives you production coverage now while your team focuses on the differentiated ML infrastructure that is actually core to your business."

### Objection 2: "We already use [OpenAI / Anthropic] moderation features."

**Response framework:**
- Provider-side moderation operates after your data has left the perimeter. From a data residency perspective, the control is applied too late — the exposure has already occurred.
- You have no visibility into the provider's moderation logic, no ability to customize policies, no audit trail that you own, and no SLA on the moderation decision itself.
- Provider moderation is a product safety feature designed to protect the provider. It is not a governance control designed to protect your firm.
- "Provider moderation and Trace are complementary, not competing. Provider moderation reduces model-level risk. Trace reduces firm-level risk. They operate at different points in the architecture and serve different stakeholders."

### Objection 3: "15ms sounds too good to be true."

**Response framework:**
- "We understand the skepticism. Every vendor claims low latency. Here's why our claim is structurally different."
- The 15ms ceiling is not a benchmark result — it is a consequence of the architecture. There are no external API calls. There is no model inference. There is no lock contention. There are no garbage collection pauses.
- We publish our benchmark methodology and provide a reproduction guide. You can validate the latency claim on your own hardware, with your own traffic, before making any commitment.
- "During the POC, you will measure latency from your own monitoring infrastructure. If the P99 exceeds 15ms under your production traffic profile, we want to know — and we will diagnose it together."

### Objection 4: "We need to see the SOC 2 Type II report before we can proceed."

**Response framework:**
- "Absolutely. Our SOC 2 Type II [report is available under NDA / engagement is in progress with completion expected by DATE]."
- We provide the full compliance package including the audit report, control narratives, evidence artifacts, and a pre-populated vendor risk questionnaire in SIG Lite, CAIQ, and custom formats.
- "We can schedule a call between your vendor risk team and our compliance lead to walk through the report and answer specific questions. We've done this with [X] institutions and are prepared for the level of scrutiny you require."

### Objection 5: "What happens when Trace goes down?"

**Response framework:**
- Trace is deployed as a stateless cluster behind your load balancer. Individual node failures are handled by standard Kubernetes or infrastructure-level health checks and replacement.
- In the event of a complete Trace outage, the fail-open/fail-closed behavior is configurable per policy. For most institutions, the default is fail-open with enhanced logging — requests are forwarded directly to the LLM provider, and the bypass event is logged for compliance review.
- "The 99.99% SLA means less than 53 minutes of downtime per year. This is backed by service credits and measured against your monitoring, not ours."
- The single-binary Rust architecture with no external dependencies means the failure surface is minimal. There is no database to crash, no message queue to back up, no external service to time out.

### Objection 6: "We can't justify another vendor in the stack."

**Response framework:**
- "We hear this, and it's a legitimate procurement concern. Let me reframe the calculus."
- Trace is not adding complexity to your stack. It is replacing an uncontrolled gap with a controlled component. The alternative is not 'no vendor' — the alternative is 'no control,' which is itself a risk position your firm is taking.
- The open-core model means you can self-host and self-operate if you prefer to avoid a vendor dependency. The Enterprise contract provides support and SLA, but the core technology runs independently.
- From a vendor risk perspective: single Rust binary, no external dependencies, no data egress, SOC 2 Type II, air-gappable. The vendor risk surface is about as small as it gets for an infrastructure component.

### Objection 7: "Our compliance team hasn't flagged LLM governance as a priority."

**Response framework:**
- "With respect, we'd encourage you to confirm that directly with your Head of Compliance. In our conversations with [X] institutions, compliance teams have consistently identified LLM governance as a near-term audit focus — they simply haven't always communicated it as a technology procurement request."
- SEC, FINRA, and FCA have all issued guidance or enforcement actions in 2024–2025 specifically referencing AI-generated communications and the application of existing supervisory frameworks to AI tools.
- "The risk is not that your compliance team hasn't flagged it. The risk is that they flag it after an incident or an examination finding, and the firm is in a reactive posture rather than a proactive one."
- "We'd welcome the opportunity to present the regulatory exposure mapping (Slide 4 / Appendix A) directly to your compliance team. In every case where we've done this, the conversation has moved forward."

### Objection 8: "We need to evaluate multiple vendors before making a decision."

**Response framework:**
- "We'd expect nothing less, and we welcome the comparison. We'd ask only that the evaluation criteria be the five requirements we outlined: data residency, latency neutrality, deterministic verdicts, immutable audit trail, and policy governance."
- We provide a structured evaluation framework document that you can use to score all vendors consistently — including us. We are confident in the outcome when the criteria are aligned with your actual regulatory and operational requirements.
- "We'd also suggest including a latency benchmark as part of the evaluation. Ask every vendor to demonstrate P99 latency under sustained load with a representative policy set. The results will be clarifying."
- "In the meantime, our POC offer stands. A 30-day proof of concept with no commitment gives your team real data to include in the evaluation."

---

## APPENDIX C: Competitive Displacement Matrix

**Evaluation criteria scored on a 1–5 scale:**  
1 = Does not satisfy requirement  
2 = Partially satisfies with significant gaps  
3 = Satisfies with notable limitations  
4 = Satisfies with minor limitations  
5 = Fully satisfies requirement

| Criterion | Trace (VPC Deploy) | SaaS Guardrails (e.g., Guardrails.ai, NeMo) | Build In-House | LLM Provider Safety Features |
|---|---|---|---|---|
| **Data residency (zero egress)** | 5 — Runs entirely in customer VPC/on-prem. No data leaves perimeter. Air-gappable. | 1 — Request content must be sent to SaaS vendor API for evaluation. New data egress path. | 5 — If built correctly, runs in customer infrastructure. | 1 — Evaluation occurs on provider infrastructure after data has already left perimeter. |
| **Latency (<50ms P99)** | 5 — <15ms P99 hard ceiling. No external calls, no model inference, no GC. | 2 — Typically 200–800ms due to external API roundtrip and/or internal model inference for classification. | 3 — Achievable in theory, but requires significant Rust/C++ systems engineering expertise. Most teams build in Python/Go and achieve 50–200ms. | 4 — Provider-side evaluation adds minimal overhead to provider response, but total round-trip already includes provider latency. |
| **Deterministic verdicts** | 5 — Pass/Block/Modify based on explicit policy logic. No probabilistic model in verdict path. | 3 — Some platforms use LLM-based classification, which is probabilistic and non-reproducible. | 4 — Depends on implementation. Deterministic if built with rule-based systems. Often tempted to use ML classifiers. | 2 — Provider moderation is opaque, probabilistic, and not configurable by the customer. |
| **Immutable audit trail** | 5 — Cryptographically signed, WORM-compatible export. Every verdict logged with full evaluation context. | 3 — Typically provides logs, but not WORM-compatible, not cryptographically signed, and audit data is stored on vendor infrastructure. | 3 — Must be purpose-built. Most internal implementations log verdicts but do not implement tamper-evidence or WORM compatibility. | 1 — No customer-owned audit trail. Provider may log for their purposes, but customer has no access or control. |
| **Policy governance (versioning, RBAC, review)** | 5 — Policy Studio UI, version control, RBAC, structured review/deploy workflow. | 3 — Varies. Some platforms offer basic policy management. Few offer RBAC or formal versioning. | 2 — Must be built from scratch. Typically deprioritized in favor of evaluation engine development. | 1 — No customer-configurable policies. Provider controls the safety configuration. |
| **Air-gap capability** | 5 — Fully air-gappable. No internet required post-deployment. Policy updates via signed artifacts. | 1 — SaaS model requires internet connectivity by definition. | 5 — If built correctly, can be air-gapped. | 1 — Requires active internet connection to provider API. |
| **Time to production** | 4 — Day 1 deploy, Week 1 first policies, Month 1 full coverage. Enterprise contract + POC. | 3 — SaaS onboarding can be fast, but integration and policy tuning varies. Data residency concerns may extend timeline. | 1 — 6–18 months for initial build. Ongoing maintenance indefinitely. | 4 — Already integrated if using the provider's API. But no customization, no audit, no governance. |
| **Total cost of ownership (3-year)** | 4 — Fixed annual infrastructure fee. No per-request metering. No scaling cost surprise. | 3 — Usage-based pricing creates variable cost exposure that grows with LLM adoption. | 2 — High upfront engineering cost (6–18 months of senior Rust eng), ongoing maintenance, opportunity cost of engineers not building core product. | 5 — Included in provider pricing (but provides minimal actual governance value). |

**Summary scores:**

| Solution | Total (out of 40) |
|---|---|
| **Trace (VPC Deploy)** | **38** |
| SaaS Guardrails | 19 |
| Build In-House | 25 |
| LLM Provider Safety Features | 19 |

**Interpretation:**
- **Trace** scores highest because it was purpose-built to satisfy all eight criteria simultaneously. The two areas where it does not score a perfect 5 — time to production (4) and total cost of ownership (4) — reflect the reality that enterprise procurement takes time and that Trace is not free.
- **Build In-House** is the second-strongest option on technical merit but scores poorly on time to production and total cost of ownership — the two criteria that determine whether the control exists when the regulator asks.
- **SaaS Guardrails** and **Provider Safety Features** fail on the two criteria that are non-negotiable for regulated institutions: data residency and audit trail ownership.

---

*End of Pitch Deck Outline*

*Document prepared by Stria Systems — Sales Engineering*  
*For internal use. Do not distribute without approval from [APPROVER].*
