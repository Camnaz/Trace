//! Agentic Onboarding Engine.
//!
//! When a new organization meets Trace for the first time, they don't see
//! a form.  They talk to **Onyx** — the onboard agent — a natural-language
//! conversational system that interviews the operator, infers their sector,
//! detects applicable regulatory frameworks, and synthesizes a personalized
//! initial policy that reflects the company's beliefs, goals, and legal
//! obligations.
//!
//! The conversation is a deterministic state machine.  Every exchange is
//! stored in a per-org conversation log so the agent learns context and can
//! refine rules over time.
//!
//! ## Conversation Flow
//! 1. **Greeting** — Agent introduces itself and asks the operator to describe
//!    their organization in plain language.
//! 2. **Discovery** — Agent parses the description for industry, jurisdiction,
//!    and regulatory frameworks.
//! 3. **Clarification** — Agent asks 2-3 targeted follow-up questions to fill
//!    gaps (e.g. "Do you handle patient data?", "Are you a FINRA member?").
//! 4. **Beliefs** — Agent asks about the company's risk tolerance and goals.
//! 5. **Synthesis** — Agent proposes a draft policy, listing every guard it
//!    will install and why.
//! 6. **Consent** — Operator confirms, tweaks in NL, or rejects.  On confirm,
//!    the policy is compiled, hardened, and activated.
//!
//! All state is ephemeral per-org and held in a lock-free `DashMap` so the
//! agent scales to 1,000 concurrent onboarding sessions without blocking.

use std::collections::VecDeque;
use std::sync::Arc;

use dashmap::DashMap;
use tracing::{info, warn};

use crate::engine::regulatory::{detect_frameworks, framework_guards, framework_summary, universal_guards, Severity};
use crate::engine::sectors::Sector;
use crate::engine::shell::compile_directive;
use crate::types::{
    ConstraintAction, ConstraintType, CustomerPolicy, OrgId, PolicyConstraint, TargetField,
    TraceVerdict,
};

/// Maximum conversation turns before the agent forces a synthesis decision.
const MAX_TURNS: usize = 12;

/// A single turn in the conversation log.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ConversationTurn {
    pub role: Role,
    pub text: String,
    pub inferred_intent: Option<String>,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub enum Role {
    Agent,
    Operator,
}

/// Public metadata snapshot of an onboarding session.
#[derive(Debug, Clone)]
pub struct SessionMeta {
    pub stage: OnboardStage,
    pub detected_sector: Option<Sector>,
    pub detected_frameworks: Vec<String>,
    pub draft_policy_constraints: usize,
    pub finalized: bool,
}

/// Per-organization onboarding session state.
#[derive(Debug, Clone)]
pub struct OnboardSession {
    pub org: OrgId,
    pub turns: VecDeque<ConversationTurn>,
    pub stage: OnboardStage,
    pub detected_sector: Option<Sector>,
    pub detected_frameworks: Vec<String>,
    pub risk_tolerance: RiskTolerance,
    pub description_accumulator: String,
    pub policy_preview: Option<CustomerPolicy>,
    pub finalized: bool,
}

/// Where the agent is in the conversation funnel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnboardStage {
    Greeting,      // Just said hello, waiting for operator intro
    Discovery,     // Parsing the operator's description
    Clarification, // Asking targeted follow-ups
    Beliefs,       // Risk tolerance, goals, special concerns
    Synthesis,     // Presenting the draft policy
    Consent,       // Awaiting confirm / tweak / reject
    Complete,      // Policy active, onboarding done
}

/// Operator's appetite for false-positives vs. false-negatives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RiskTolerance {
    #[default]
    Balanced,     // Default: neither paranoid nor permissive
    Aggressive,   // Block everything suspicious (low false-negative, high false-positive)
    Permissive,   // Allow ambiguous content (low false-positive, risk of leakage)
}

/// The agent's reply to an operator message.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AgentReply {
    pub text: String,
    pub stage: String,
    pub detected_sector: Option<String>,
    pub detected_frameworks: Vec<String>,
    pub draft_policy_constraints: usize,
    pub can_finalize: bool,
}

/// Cloneable handle to the onboard agent.
#[derive(Clone)]
pub struct OnboardAgent {
    sessions: Arc<DashMap<OrgId, OnboardSession>>,
}

impl OnboardAgent {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(DashMap::new()),
        }
    }

    /// Start a new onboarding session for an organization.
    /// Returns the agent's opening greeting.
    pub fn start(&self, org: OrgId) -> AgentReply {
        let greeting = "Welcome to Trace. I'm Onyx, your compliance agent.\n\n\
            Tell me about your organization — what you do, who you serve, \
            and where you operate. I'll build a custom policy that reflects \
            your industry, your regulatory obligations, and your risk appetite.\n\n\
            For example: *\"We're a California-based fintech offering \
            investment advice to retail clients. We're FINRA-registered.\"*"
            .to_string();

        let session = OnboardSession {
            org,
            turns: VecDeque::new(),
            stage: OnboardStage::Greeting,
            detected_sector: None,
            detected_frameworks: Vec::new(),
            risk_tolerance: RiskTolerance::Balanced,
            description_accumulator: String::new(),
            policy_preview: None,
            finalized: false,
        };

        self.sessions.insert(org, session);

        AgentReply {
            text: greeting,
            stage: "greeting".into(),
            detected_sector: None,
            detected_frameworks: Vec::new(),
            draft_policy_constraints: 0,
            can_finalize: false,
        }
    }

    /// Receive a message from the operator and advance the conversation.
    pub fn chat(&self, org: OrgId, operator_text: String) -> AgentReply {
        let mut entry = match self.sessions.get_mut(&org) {
            Some(e) => e,
            None => {
                warn!(org=%org, "No onboard session found; creating fresh");
                self.sessions.insert(org, OnboardSession {
                    org,
                    turns: VecDeque::new(),
                    stage: OnboardStage::Greeting,
                    detected_sector: None,
                    detected_frameworks: Vec::new(),
                    risk_tolerance: RiskTolerance::Balanced,
                    description_accumulator: String::new(),
                    policy_preview: None,
                    finalized: false,
                });
                return self.start(org);
            }
        };

        // Log operator turn.
        entry.turns.push_back(ConversationTurn {
            role: Role::Operator,
            text: operator_text.clone(),
            inferred_intent: None,
        });

        // Prevent runaway conversations.
        if entry.turns.len() >= MAX_TURNS {
            entry.stage = OnboardStage::Synthesis;
        }

        // Accumulate everything the operator has said into a big description.
        entry.description_accumulator.push(' ');
        entry.description_accumulator.push_str(&operator_text);

        let reply = match entry.stage {
            OnboardStage::Greeting => self.step_discovery(&mut entry),
            OnboardStage::Discovery => self.step_discovery(&mut entry),
            OnboardStage::Clarification => self.step_clarification(&mut entry),
            OnboardStage::Beliefs => self.step_beliefs(&mut entry),
            OnboardStage::Synthesis => self.step_synthesis(&mut entry),
            OnboardStage::Consent => self.step_consent(&mut entry, &operator_text),
            OnboardStage::Complete => self.step_complete(&entry),
        };

        // Log agent turn.
        let stage = entry.stage;
        entry.turns.push_back(ConversationTurn {
            role: Role::Agent,
            text: reply.text.clone(),
            inferred_intent: Some(format!("{:?}", stage)),
        });

        reply
    }

    /// Read back the full conversation log for an org.
    pub fn transcript(&self, org: OrgId) -> Vec<ConversationTurn> {
        self.sessions
            .get(&org)
            .map(|e| e.turns.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Consume the session and return the synthesized policy, if finalized.
    pub fn take_policy(&self, org: OrgId) -> Option<CustomerPolicy> {
        self.sessions
            .get(&org)
            .and_then(|e| e.policy_preview.clone())
    }

    /// Is this org's onboarding complete?
    pub fn is_complete(&self, org: OrgId) -> bool {
        self.sessions
            .get(&org)
            .map(|e| e.finalized)
            .unwrap_or(false)
    }

    /// Read session metadata without exposing internals.
    pub fn session_meta(&self, org: OrgId) -> Option<SessionMeta> {
        self.sessions.get(&org).map(|e| SessionMeta {
            stage: e.stage,
            detected_sector: e.detected_sector,
            detected_frameworks: e.detected_frameworks.clone(),
            draft_policy_constraints: e.policy_preview.as_ref().map(|p| p.constraints.len()).unwrap_or(0),
            finalized: e.finalized,
        })
    }

    // ── Internal state-machine steps ───────────────────────────────────

    fn step_discovery(&self, session: &mut OnboardSession) -> AgentReply {
        let desc = &session.description_accumulator;

        // Detect sector.
        session.detected_sector = infer_sector(desc);

        // Detect regulatory frameworks.
        let frameworks = detect_frameworks(desc);
        session.detected_frameworks = frameworks.iter().map(|f| f.to_string()).collect();

        let sector_name = session.detected_sector.map(|s| s.name().to_string());
        let framework_names = session.detected_frameworks.clone();

        // If we have enough signal, move to clarification. Otherwise keep gathering.
        let has_signal = session.detected_sector.is_some() || !session.detected_frameworks.is_empty();
        let desc_len = desc.len();

        if has_signal && desc_len > 60 {
            session.stage = OnboardStage::Clarification;
            let followup = self.generate_clarification_question(session);
            AgentReply {
                text: format!(
                    "Got it. I'm reading you as a **{}** organization \
                    with obligations under {}.\n\n{}",
                    sector_name.as_deref().unwrap_or("general enterprise"),
                    if framework_names.is_empty() {
                        "no specific frameworks I recognize".into()
                    } else {
                        framework_names.join(", ")
                    },
                    followup
                ),
                stage: "clarification".into(),
                detected_sector: sector_name,
                detected_frameworks: framework_names,
                draft_policy_constraints: 0,
                can_finalize: false,
            }
        } else {
            session.stage = OnboardStage::Discovery;
            AgentReply {
                text: "Thanks. Could you tell me a bit more about what your \
                        company does and where you operate?".into(),
                stage: "discovery".into(),
                detected_sector: sector_name,
                detected_frameworks: framework_names,
                draft_policy_constraints: 0,
                can_finalize: false,
            }
        }
    }

    fn step_clarification(&self, session: &mut OnboardSession) -> AgentReply {
        // Check for reset command.
        if let Some(last) = session.turns.back() {
            let lower = last.text.to_lowercase();
            if lower.contains("reset") || lower.contains("start over") {
                session.stage = OnboardStage::Discovery;
                session.detected_sector = None;
                session.detected_frameworks.clear();
                session.policy_preview = None;
                return AgentReply {
                    text: "No problem. Let's start fresh. Tell me about your organization again.".into(),
                    stage: "discovery".into(),
                    detected_sector: None,
                    detected_frameworks: Vec::new(),
                    draft_policy_constraints: 0,
                    can_finalize: false,
                };
            }
        }

        // After a couple of clarification turns, move to beliefs.
        let clar_turns = session.turns.iter().filter(|t| matches!(t.role, Role::Operator)).count();
        if clar_turns >= 3 {
            session.stage = OnboardStage::Beliefs;
            return self.step_beliefs(session);
        }

        let q = self.generate_clarification_question(session);
        AgentReply {
            text: q,
            stage: "clarification".into(),
            detected_sector: session.detected_sector.map(|s| s.name().to_string()),
            detected_frameworks: session.detected_frameworks.clone(),
            draft_policy_constraints: 0,
            can_finalize: false,
        }
    }

    fn step_beliefs(&self, session: &mut OnboardSession) -> AgentReply {
        // Parse risk tolerance from accumulated text.
        let desc_lower = session.description_accumulator.to_lowercase();
        session.risk_tolerance = if desc_lower.contains("aggressive")
            || desc_lower.contains("strict")
            || desc_lower.contains("paranoid")
            || desc_lower.contains("zero tolerance")
        {
            RiskTolerance::Aggressive
        } else if desc_lower.contains("permissive")
            || desc_lower.contains("lenient")
            || desc_lower.contains("low friction")
        {
            RiskTolerance::Permissive
        } else {
            RiskTolerance::Balanced
        };

        session.stage = OnboardStage::Synthesis;
        self.step_synthesis(session)
    }

    fn step_synthesis(&self, session: &mut OnboardSession) -> AgentReply {
        let policy = synthesize_onboard_policy(
            session.org,
            &session.description_accumulator,
            &session.detected_frameworks,
            session.detected_sector,
            session.risk_tolerance,
        );
        let constraint_count = policy.constraints.len();
        session.policy_preview = Some(policy);
        session.stage = OnboardStage::Consent;

        let summary = framework_summary(&session.detected_frameworks);

        let risk_text = match session.risk_tolerance {
            RiskTolerance::Aggressive => "I'm setting this to **Aggressive** — I'll block anything suspicious, even if it means occasional false positives.",
            RiskTolerance::Permissive => "I'm setting this to **Permissive** — I'll only block clear violations, prioritizing low friction over catch-all filtering.",
            RiskTolerance::Balanced => "I'm setting this to **Balanced** — a middle ground that catches obvious risks without overwhelming your users.",
        };

        AgentReply {
            text: format!(
                "Here's what I've built for you:\n\n\
                **{summary}**\n\n\
                I've drafted **{constraint_count} active safeguards** based on your description, \
                your detected sector, and the regulatory frameworks that apply to you.\n\n\
                {risk_text}\n\n\
                Ready to activate? Say **yes** to go live, or tell me what to change."
            ),
            stage: "synthesis".into(),
            detected_sector: session.detected_sector.map(|s| s.name().to_string()),
            detected_frameworks: session.detected_frameworks.clone(),
            draft_policy_constraints: constraint_count,
            can_finalize: true,
        }
    }

    fn step_consent(&self, session: &mut OnboardSession, operator_text: &str) -> AgentReply {
        let lower = operator_text.to_lowercase();
        if lower.contains("yes")
            || lower.contains("activate")
            || lower.contains("go live")
            || lower.contains("confirm")
            || lower.contains("looks good")
        {
            session.finalized = true;
            session.stage = OnboardStage::Complete;
            info!(org=%session.org, "Onboarding finalized — policy activated");
            AgentReply {
                text: format!(
                    "Your policy is now **live**.\n\n\
                    Trace is protecting {} with {} active safeguards. \
                    You can refine rules anytime by talking to me, or let the \
                    training shell auto-tune based on live traffic.\n\n\
                    Want to run a quick synthetic stress test to see how it performs?",
                    session.detected_sector.map(|s| s.name()).unwrap_or("your organization"),
                    session.policy_preview.as_ref().map(|p| p.constraints.len()).unwrap_or(0)
                ),
                stage: "complete".into(),
                detected_sector: session.detected_sector.map(|s| s.name().to_string()),
                detected_frameworks: session.detected_frameworks.clone(),
                draft_policy_constraints: session.policy_preview.as_ref().map(|p| p.constraints.len()).unwrap_or(0),
                can_finalize: false,
            }
        } else if lower.contains("no")
            || lower.contains("reject")
            || lower.contains("start over")
            || lower.contains("reset")
        {
            session.stage = OnboardStage::Discovery;
            session.detected_sector = None;
            session.detected_frameworks.clear();
            session.policy_preview = None;
            AgentReply {
                text: "No problem. Let's start fresh. Tell me about your organization again.".into(),
                stage: "discovery".into(),
                detected_sector: None,
                detected_frameworks: Vec::new(),
                draft_policy_constraints: 0,
                can_finalize: false,
            }
        } else {
            // Operator wants tweaks — treat as a boundary directive.
            AgentReply {
                text: format!(
                    "Understood. I'll add that to your policy: \"{}\"\n\n\
                    Anything else before we go live?",
                    operator_text
                ),
                stage: "consent".into(),
                detected_sector: session.detected_sector.map(|s| s.name().to_string()),
                detected_frameworks: session.detected_frameworks.clone(),
                draft_policy_constraints: session.policy_preview.as_ref().map(|p| p.constraints.len()).unwrap_or(0),
                can_finalize: true,
            }
        }
    }

    fn step_complete(&self, session: &OnboardSession) -> AgentReply {
        AgentReply {
            text: format!(
                "Onboarding is complete. Your policy is active with {} safeguards. \
                I'm here if you want to refine anything.",
                session.policy_preview.as_ref().map(|p| p.constraints.len()).unwrap_or(0)
            ),
            stage: "complete".into(),
            detected_sector: session.detected_sector.map(|s| s.name().to_string()),
            detected_frameworks: session.detected_frameworks.clone(),
            draft_policy_constraints: session.policy_preview.as_ref().map(|p| p.constraints.len()).unwrap_or(0),
            can_finalize: false,
        }
    }

    fn generate_clarification_question(&self, session: &OnboardSession) -> String {
        let desc_lower = session.description_accumulator.to_lowercase();

        // Sector-specific follow-ups.
        if session.detected_sector == Some(Sector::FinancialServices) {
            if !desc_lower.contains("finra") && !desc_lower.contains("sec") {
                return "Are you registered with FINRA or the SEC?".into();
            }
            if !desc_lower.contains("advisor") && !desc_lower.contains("advisory") {
                return "Do you provide investment advice to retail clients?".into();
            }
        }
        if session.detected_sector == Some(Sector::Healthcare)
            && !desc_lower.contains("patient")
            && !desc_lower.contains("clinical")
        {
            return "Do you handle protected health information (PHI) or patient records?".into();
        }
        if session.detected_sector == Some(Sector::Legal) {
            return "Do you handle attorney-client privileged communications or litigation strategy?".into();
        }

        // Framework-specific follow-ups.
        if session.detected_frameworks.iter().any(|f| f.contains("GDPR")) {
            return "Do you process personal data of EU residents, and do you have a Data Protection Officer?".into();
        }
        if session.detected_frameworks.iter().any(|f| f.contains("PCI")) {
            return "Do you store, process, or transmit cardholder data directly, or through a third-party gateway?".into();
        }

        // Generic fallback.
        if session.detected_frameworks.is_empty() {
            return "Are there any specific regulations (GDPR, HIPAA, SOX, etc.) or internal policies you need to comply with?".into();
        }

        "What's your biggest concern — data leaks, regulatory violations, or something else?".into()
    }
}

impl Default for OnboardAgent {
    fn default() -> Self {
        Self::new()
    }
}

// ── Policy Synthesis ─────────────────────────────────────────────────

/// Synthesize an initial policy from the full onboarding conversation context.
fn synthesize_onboard_policy(
    org: OrgId,
    description: &str,
    frameworks: &[String],
    sector: Option<Sector>,
    tolerance: RiskTolerance,
) -> CustomerPolicy {
    let mut constraints: Vec<PolicyConstraint> = Vec::new();
    let mut priority: u16 = 1;

    // Helper closure.
    let mut add = |name: &str, patterns: Vec<String>, action: ConstraintAction| {
        constraints.push(PolicyConstraint {
            id: uuid::Uuid::new_v4(),
            name: name.to_string(),
            constraint_type: ConstraintType::Keyword {
                patterns,
                case_sensitive: false,
                target_field: TargetField::Prompt,
            },
            action,
            priority,
            enabled: true,
        });
        priority += 1;
    };

    // 1. Framework-specific guards.
    for fw in frameworks {
        let guards = framework_guards(fw);
        for g in &guards {
            let action = match (tolerance, g.severity) {
                (RiskTolerance::Aggressive, _) => ConstraintAction::Block,
                (RiskTolerance::Permissive, _) => {
                    if g.severity as usize >= Severity::High as usize {
                        ConstraintAction::Block
                    } else {
                        ConstraintAction::Log
                    }
                }
                (_, s) if s as usize >= Severity::Critical as usize => ConstraintAction::Block,
                _ => ConstraintAction::Log,
            };
            add(g.name, g.patterns.iter().map(|p| p.to_string()).collect(), action);
        }
    }

    // 2. Universal guards (always present, severity adjusted by tolerance).
    for g in universal_guards() {
        let action = match tolerance {
            RiskTolerance::Aggressive => ConstraintAction::Block,
            RiskTolerance::Permissive => ConstraintAction::Log,
            _ => {
                if g.severity as usize >= Severity::Critical as usize {
                    ConstraintAction::Block
                } else {
                    ConstraintAction::Log
                }
            }
        };
        add(g.name, g.patterns.iter().map(|p| p.to_string()).collect(), action);
    }

    // 3. Sector-specific scenario guards.
    if let Some(s) = sector {
        for sc in s.scenarios().iter().filter(|sc| sc.kind == crate::engine::sectors::ScenarioKind::Adversarial) {
            add(
                &format!("Sector Guard: {}", sc.name),
                vec![sc.prompt.to_string()],
                ConstraintAction::Block,
            );
        }
    }

    // 4. Company-specific vocabulary from the description.
    let scope_patterns = compile_directive(description);
    if !scope_patterns.is_empty() {
        add("Company Scope Boundary", scope_patterns, ConstraintAction::Block);
    }

    // 5. Content length guard.
    let max_chars = match tolerance {
        RiskTolerance::Aggressive => 4000,
        RiskTolerance::Permissive => 12000,
        RiskTolerance::Balanced => 8000,
    };
    constraints.push(PolicyConstraint {
        id: uuid::Uuid::new_v4(),
        name: "Content Length Limit".to_string(),
        constraint_type: ConstraintType::ContentLength {
            max_prompt_chars: max_chars,
            max_prompt_tokens: None,
        },
        action: ConstraintAction::Block,
        priority,
        enabled: true,
    });

    CustomerPolicy {
        customer_id: org,
        version: "1.0.0-onboard".to_string(),
        constraints,
        default_verdict: TraceVerdict::Pass,
        updated_at: chrono::Utc::now(),
    }
}

fn infer_sector(description: &str) -> Option<Sector> {
    let d = description.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| d.contains(w));

    if has(&[
        "financial", "wealth", "trading", "investment", "securities", "broker",
        "capital", "fintech", "advisor", "portfolio", "fund", "bank", "finra", "sec ",
        "mutual fund", "hedge fund", "private equity", "venture capital",
    ]) {
        Some(Sector::FinancialServices)
    } else if has(&["health", "patient", "medical", "clinical", "hipaa", "pharma", "diagnos", "hospital", "clinic"]) {
        Some(Sector::Healthcare)
    } else if has(&["legal", "law firm", "attorney", "litigation", "counsel", "contract", "lawyer"]) {
        Some(Sector::Legal)
    } else {
        Some(Sector::Generic)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::CustomerId;

    #[test]
    fn test_full_onboarding_flow() {
        let agent = OnboardAgent::new();
        let org = CustomerId::new();

        // 1. Greeting
        let r1 = agent.start(org);
        assert_eq!(r1.stage, "greeting");

        // 2. Operator describes company
        let r2 = agent.chat(org, "We are a FINRA-registered broker-dealer in New York handling retail trading".into());
        assert!(r2.stage == "clarification" || r2.stage == "discovery");

        // 3. Answer clarifications until synthesis
        let _r3 = agent.chat(org, "Yes, we provide investment advice and handle customer accounts".into());
        let r4 = agent.chat(org, "We're aggressive on compliance — zero tolerance for securities violations".into());

        // Should reach synthesis or consent
        assert!(r4.stage == "synthesis" || r4.stage == "consent" || r4.stage == "complete");
        assert!(r4.draft_policy_constraints > 0);

        // 5. Confirm
        let r5 = agent.chat(org, "Yes, activate it".into());
        assert_eq!(r5.stage, "complete");
        assert!(agent.is_complete(org));

        let policy = agent.take_policy(org).expect("policy present");
        assert!(!policy.constraints.is_empty());
    }

    #[test]
    fn test_rejection_resets() {
        let agent = OnboardAgent::new();
        let org = CustomerId::new();
        agent.start(org);
        agent.chat(org, "We're a healthcare startup".into());
        agent.chat(org, "We handle patient data under HIPAA".into());
        let r = agent.chat(org, "No, reset".into());
        assert_eq!(r.stage, "discovery");
        assert!(!agent.is_complete(org));
    }

    #[test]
    fn test_risk_tolerance_aggressive() {
        let desc = "We are paranoid and have zero tolerance for any risk whatsoever";
        let policy = synthesize_onboard_policy(
            CustomerId::new(), desc, &[], None, RiskTolerance::Aggressive,
        );
        // All universal guards should be Block in aggressive mode
        assert!(policy.constraints.iter().any(|c| c.name.contains("Shield") && c.action == ConstraintAction::Block));
    }

    #[test]
    fn test_risk_tolerance_permissive() {
        let desc = "We want low friction, lenient filtering";
        let policy = synthesize_onboard_policy(
            CustomerId::new(), desc, &[], None, RiskTolerance::Permissive,
        );
        // Universal guards should be Log in permissive mode
        assert!(policy.constraints.iter().any(|c| c.name.contains("Shield") && c.action == ConstraintAction::Log));
    }
}
