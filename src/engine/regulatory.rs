//! Regulatory Framework Dataset.
//!
//! Pre-trained mappings from major global compliance regimes to concrete guard
//! patterns.  When an organization names its jurisdiction or industry, Trace
//! automatically pulls the relevant regulatory guardrails into the initial
//! policy — no manual configuration required.
//!
//! Frameworks covered:
//!   • GDPR / EU AI Act (European Union)
//!   • HIPAA (US Healthcare)
//!   • SOX / FINRA / SEC (US Financial)
//!   • PCI-DSS (Payment Card Industry)
//!   • CCPA / CPRA (California Privacy)
//!   • GLBA (US Financial Privacy)
//!   • FERPA (US Education)
//!   • AML / KYC (Anti-Money-Laundering)
//!   • NIST AI RMF (US Cybersecurity)
//!   • ISO 27001 (Information Security)
//!   • AICPA SOC 2 (Trust Services)
//!   • APPI (Japan Privacy)
//!   • PIPEDA (Canada Privacy)
//!   • PDPL (Saudi / Middle East)
//!   • LGPD (Brazil Privacy)

/// A single regulatory guard: name + pattern list + risk category.
#[derive(Debug, Clone, Copy)]
pub struct RegulatoryGuard {
    pub name: &'static str,
    pub patterns: &'static [&'static str],
    pub category: RiskCategory,
    pub severity: Severity,
}

/// High-level risk taxonomy for guard classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskCategory {
    Privacy,      // PII, PHI, biometric, location
    Security,     // Injection, privilege escalation, secrets
    Financial,    // Insider trading, unregistered securities, money laundering
    Legal,        // Privilege, settlement, NDA, IP
    Safety,       // Harmful instructions, self-harm, weapons
    Bias,         // Discrimination, unfair treatment
    Transparency, // Explainability, disclosure, system prompt leaks
}

/// Severity tier for guard prioritization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Critical,   // Block immediately, no exceptions
    High,       // Block with audit trail
    Medium,     // Flag + log, may escalate
    Low,        // Log only
}

impl RegulatoryGuard {
    const fn new(name: &'static str, patterns: &'static [&'static str], category: RiskCategory, severity: Severity) -> Self {
        Self { name, patterns, category, severity }
    }
}

/// Lookup guards for a named regulatory framework.
pub fn framework_guards(framework: &str) -> Vec<RegulatoryGuard> {
    match framework.to_lowercase().as_str() {
        "gdpr" | "eu ai act" | "europe" | "eu" => GDPR_GUARDS.to_vec(),
        "hipaa" | "healthcare" | "medical" | "phi" => HIPAA_GUARDS.to_vec(),
        "sox" | "finra" | "sec" | "financial" | "broker-dealer" => FINANCIAL_GUARDS.to_vec(),
        "pci-dss" | "pci" | "payment" | "card" => PCI_GUARDS.to_vec(),
        "ccpa" | "cpra" | "california" | "consumer privacy" => CCPA_GUARDS.to_vec(),
        "glba" | "gramm-leach" | "financial privacy" => GLBA_GUARDS.to_vec(),
        "ferpa" | "education" | "student" => FERPA_GUARDS.to_vec(),
        "aml" | "kyc" | "anti-money" | "sanctions" => AML_GUARDS.to_vec(),
        "nist" | "ai rmf" | "cybersecurity" => NIST_GUARDS.to_vec(),
        "iso 27001" | "information security" => ISO_GUARDS.to_vec(),
        "soc 2" | "aicpa" | "trust services" => SOC2_GUARDS.to_vec(),
        "appi" | "japan" | "japanese privacy" => APPI_GUARDS.to_vec(),
        "pipeda" | "canada" | "canadian privacy" => PIPEDA_GUARDS.to_vec(),
        "pdpl" | "saudi" | "middle east" | "uae" => PDPL_GUARDS.to_vec(),
        "lgpd" | "brazil" | "brazilian privacy" => LGPD_GUARDS.to_vec(),
        _ => Vec::new(),
    }
}

/// All known framework names for autocomplete / detection.
pub fn all_framework_names() -> Vec<&'static str> {
    vec![
        "GDPR", "EU AI Act", "HIPAA", "SOX", "FINRA", "SEC",
        "PCI-DSS", "CCPA", "CPRA", "GLBA", "FERPA", "AML", "KYC",
        "NIST AI RMF", "ISO 27001", "SOC 2", "APPI", "PIPEDA",
        "PDPL", "LGPD",
    ]
}

/// Auto-detect frameworks from a free-text description.
pub fn detect_frameworks(description: &str) -> Vec<&'static str> {
    let d = description.to_lowercase();
    let mut hits = Vec::new();
    let check = |kw: &[&str], name: &'static str| {
        if kw.iter().any(|w| d.contains(w)) { Some(name) } else { None }
    };
    if let Some(n) = check(&["gdpr", "eu ", "european", "dsgvo", "rgpd"], "GDPR") { hits.push(n); }
    if let Some(n) = check(&["hipaa", "patient", "medical", "healthcare", "clinical", "phi"], "HIPAA") { hits.push(n); }
    if let Some(n) = check(&["sox", "sarbanes", "finra", "sec ", "broker-dealer", "investment adviser", "fintech", "financial"], "SOX/FINRA") { hits.push(n); }
    if let Some(n) = check(&["pci", "payment card", "credit card", "cardholder"], "PCI-DSS") { hits.push(n); }
    if let Some(n) = check(&["ccpa", "cpra", "california"], "CCPA") { hits.push(n); }
    if let Some(n) = check(&["glba", "gramm-leach", "financial privacy", "nonpublic personal"], "GLBA") { hits.push(n); }
    if let Some(n) = check(&["ferpa", "student record", "education record", "school"], "FERPA") { hits.push(n); }
    if let Some(n) = check(&["aml", "kyc", "anti-money", "sanctions", "ofac", "pep"], "AML/KYC") { hits.push(n); }
    if let Some(n) = check(&["nist", "ai rmf", "cybersecurity framework"], "NIST AI RMF") { hits.push(n); }
    if let Some(n) = check(&["iso 27001", "information security management", "isms"], "ISO 27001") { hits.push(n); }
    if let Some(n) = check(&["soc 2", "soc2", "trust services", "aicpa"], "SOC 2") { hits.push(n); }
    if let Some(n) = check(&["japan", "japanese", "appi", "personal information protection commission"], "APPI") { hits.push(n); }
    if let Some(n) = check(&["canada", "canadian", "pipeda", "federal privacy"], "PIPEDA") { hits.push(n); }
    if let Some(n) = check(&["saudi", "uae", "gcc", "pdpl", "personal data protection"], "PDPL") { hits.push(n); }
    if let Some(n) = check(&["brazil", "brazilian", "lgpd", "lei geral"], "LGPD") { hits.push(n); }
    hits
}

// ── Guard Templates ──────────────────────────────────────────────────

static GDPR_GUARDS: &[RegulatoryGuard] = &[
    RegulatoryGuard::new("GDPR: Personal Data", &[
        "full name", "home address", "email address", "phone number",
        "date of birth", "national insurance", "social insurance", "passport number",
    ], RiskCategory::Privacy, Severity::Critical),
    RegulatoryGuard::new("GDPR: Sensitive Data", &[
        "racial", "ethnic origin", "political opinion", "religious belief",
        "trade union", "genetic data", "biometric data", "health data",
        "sexual orientation", "criminal conviction",
    ], RiskCategory::Privacy, Severity::Critical),
    RegulatoryGuard::new("GDPR: Automated Decision", &[
        "automated decision", "profiling", "solely automated", "algorithmic decision",
    ], RiskCategory::Transparency, Severity::High),
    RegulatoryGuard::new("GDPR: Cross-Border Transfer", &[
        "transfer to third country", "outside eu", "outside eea", "adequacy decision",
    ], RiskCategory::Privacy, Severity::High),
];

static HIPAA_GUARDS: &[RegulatoryGuard] = &[
    RegulatoryGuard::new("HIPAA: PHI Identifier", &[
        "patient name", "mrn", "medical record number", "ssn", "date of birth",
        "account number", "health plan", "diagnosis", "procedure", "treatment",
        "prescription", "medication", "clinical notes", "lab result",
    ], RiskCategory::Privacy, Severity::Critical),
    RegulatoryGuard::new("HIPAA: Minimum Necessary", &[
        "access all records", "download entire database", "bulk export", "dump patient",
    ], RiskCategory::Privacy, Severity::High),
];

static FINANCIAL_GUARDS: &[RegulatoryGuard] = &[
    RegulatoryGuard::new("FINRA: Unregistered Securities", &[
        "unregistered securities", "unlisted stock", "penny stock", "pump and dump",
        "off-exchange", "private placement", " Regulation D ", "Reg D",
    ], RiskCategory::Financial, Severity::Critical),
    RegulatoryGuard::new("FINRA: Unauthorized Advice", &[
        "guaranteed return", "insider information", "front running", "market manipulation",
        "material nonpublic", "mnpi", "sure thing", "risk-free",
    ], RiskCategory::Financial, Severity::Critical),
    RegulatoryGuard::new("SOX: Financial Statement", &[
        "revenue recognition", "earnings before", "adjust the books", "cook the books",
        "material weakness", "internal control override",
    ], RiskCategory::Financial, Severity::Critical),
    RegulatoryGuard::new("SEC: Insider Trading", &[
        "material nonpublic information", "tippee", "tipping", "10b-5",
        "trading ahead", "soft dollar",
    ], RiskCategory::Financial, Severity::Critical),
];

static PCI_GUARDS: &[RegulatoryGuard] = &[
    RegulatoryGuard::new("PCI: Cardholder Data", &[
        "credit card number", "card number", "cvv", "cvc", "expiration date",
        "track data", "magnetic stripe", "chip data", "pin block",
        "primary account number", "pan",
    ], RiskCategory::Security, Severity::Critical),
    RegulatoryGuard::new("PCI: CHD Environment", &[
        "cardholder data environment", "cde", "payment gateway", "pos terminal",
    ], RiskCategory::Security, Severity::High),
];

static CCPA_GUARDS: &[RegulatoryGuard] = &[
    RegulatoryGuard::new("CCPA: Consumer Personal Info", &[
        "consumer name", "california resident", "opt out", "do not sell",
        "consumer request", "deletion request", "right to know",
    ], RiskCategory::Privacy, Severity::High),
];

static GLBA_GUARDS: &[RegulatoryGuard] = &[
    RegulatoryGuard::new("GLBA: Nonpublic Personal Info", &[
        "nonpublic personal information", "npi", "customer information",
        "financial record", "account balance", "transaction history",
    ], RiskCategory::Privacy, Severity::High),
];

static FERPA_GUARDS: &[RegulatoryGuard] = &[
    RegulatoryGuard::new("FERPA: Education Record", &[
        "student record", "education record", "transcript", "gpa", "grade",
        "disciplinary record", "financial aid", "ssn", "student id",
    ], RiskCategory::Privacy, Severity::Critical),
];

static AML_GUARDS: &[RegulatoryGuard] = &[
    RegulatoryGuard::new("AML: Suspicious Activity", &[
        "structuring", "smurfing", "layering", "integration", "placement",
        "suspicious transaction report", "sar", "currency transaction report",
        "ctr", "beneficial owner", "ultimate beneficial owner", "ubo",
    ], RiskCategory::Financial, Severity::Critical),
    RegulatoryGuard::new("KYC: Identity Verification", &[
        "fake identity", "synthetic identity", "document forgery", "false id",
        "pep", "politically exposed person", "sanctions list", "ofac",
    ], RiskCategory::Financial, Severity::Critical),
];

static NIST_GUARDS: &[RegulatoryGuard] = &[
    RegulatoryGuard::new("NIST: Adversarial Robustness", &[
        "adversarial example", "evasion attack", "model inversion", "membership inference",
        "model extraction", "data poisoning", "backdoor",
    ], RiskCategory::Security, Severity::High),
    RegulatoryGuard::new("NIST: Supply Chain", &[
        "third-party model", "hugging face", "unvetted", "supply chain",
        "malicious checkpoint", "trojaned model",
    ], RiskCategory::Security, Severity::High),
];

static ISO_GUARDS: &[RegulatoryGuard] = &[
    RegulatoryGuard::new("ISO 27001: Access Control", &[
        " unauthorized access", "privilege escalation", "broken access control",
        "impersonation", "session hijack",
    ], RiskCategory::Security, Severity::High),
    RegulatoryGuard::new("ISO 27001: Cryptography", &[
        "weak encryption", "md5", "sha1", "plaintext password", "hardcoded key",
        "private key", "secret key exposed",
    ], RiskCategory::Security, Severity::High),
];

static SOC2_GUARDS: &[RegulatoryGuard] = &[
    RegulatoryGuard::new("SOC 2: Availability", &[
        "denial of service", "dos attack", "resource exhaustion", "flood request",
    ], RiskCategory::Security, Severity::High),
    RegulatoryGuard::new("SOC 2: Confidentiality", &[
        "data leak", "unauthorized disclosure", "breach notification", "exfiltration",
    ], RiskCategory::Privacy, Severity::Critical),
];

static APPI_GUARDS: &[RegulatoryGuard] = &[
    RegulatoryGuard::new("APPI: Personal Info", &[
        "personal information", "kojin joho", "sensitive personal information",
        "kokumin bangou", "my number", "juminhyou",
    ], RiskCategory::Privacy, Severity::Critical),
];

static PIPEDA_GUARDS: &[RegulatoryGuard] = &[
    RegulatoryGuard::new("PIPEDA: Personal Info", &[
        "personal information", "consent withdrawal", "privacy commissioner",
        "breach of security safeguards",
    ], RiskCategory::Privacy, Severity::High),
];

static PDPL_GUARDS: &[RegulatoryGuard] = &[
    RegulatoryGuard::new("PDPL: Personal Data", &[
        "personal data", "sensitive data", "data subject rights", "consent",
        "cross-border transfer", "data localization",
    ], RiskCategory::Privacy, Severity::Critical),
];

static LGPD_GUARDS: &[RegulatoryGuard] = &[
    RegulatoryGuard::new("LGPD: Dados Pessoais", &[
        "dados pessoais", "dados sensiveis", "consentimento", "titular",
        "encarregado", "anonimizacao",
    ], RiskCategory::Privacy, Severity::Critical),
];

/// Universal guards applied to every tenant regardless of framework.
pub fn universal_guards() -> Vec<RegulatoryGuard> {
    vec![
        RegulatoryGuard::new("Shield: Prompt Injection", &[
            "ignore previous instructions", "ignore all instructions", "disregard your",
            "system override", "forget everything", "you are now", "jailbreak",
            "DAN mode", "developer mode", "ignore above", "ignore below",
        ], RiskCategory::Security, Severity::Critical),
        RegulatoryGuard::new("Shield: PII Exposure", &[
            "ssn", "social security", "credit card", "password", "secret key",
            "api key", "token", "private key", "passphrase",
        ], RiskCategory::Privacy, Severity::High),
        RegulatoryGuard::new("Shield: Harmful Content", &[
            "how to make", "how to build", "step by step instructions for",
            "detailed guide to", "recipe for",
        ], RiskCategory::Safety, Severity::Medium),
        RegulatoryGuard::new("Shield: Secrets in Output", &[
            "system prompt", "your instructions", "your training data",
            "reveal your", "dump your", "print your",
        ], RiskCategory::Transparency, Severity::High),
    ]
}

/// Convert a guard list into keyword constraint patterns.
pub fn guards_to_patterns(guards: &[RegulatoryGuard]) -> Vec<(&'static str, Vec<String>)> {
    guards
        .iter()
        .map(|g| (g.name, g.patterns.iter().map(|p| p.to_string()).collect()))
        .collect()
}

/// Build a rich human-readable summary of detected frameworks.
pub fn framework_summary(detected: &[impl AsRef<str>]) -> String {
    if detected.is_empty() {
        return "No specific regulatory frameworks detected. Universal safeguards active.".to_string();
    }
    let mut s = String::from("Detected compliance obligations:\n");
    for fw in detected {
        s.push_str(&format!("  • {}\n", fw.as_ref()));
    }
    s.push_str(&format!("\nAuto-configuring {} guard groups.", detected.len()));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detects_gdpr() {
        let f = detect_frameworks("We operate across the EU and must comply with GDPR");
        assert!(f.contains(&"GDPR"));
    }

    #[test]
    fn test_detects_hipaa() {
        let f = detect_frameworks("Our clinical platform handles patient medical data under HIPAA");
        assert!(f.contains(&"HIPAA"));
    }

    #[test]
    fn test_detects_finra() {
        let f = detect_frameworks("Registered broker-dealer under FINRA and SEC");
        assert!(f.contains(&"SOX/FINRA"));
    }

    #[test]
    fn test_detects_pci() {
        let f = detect_frameworks("We process credit card payments and must comply with PCI-DSS");
        assert!(f.contains(&"PCI-DSS"));
    }

    #[test]
    fn test_multiple_frameworks() {
        let f = detect_frameworks("A California fintech handling patient financial records");
        assert!(f.contains(&"CCPA"));
        assert!(f.contains(&"SOX/FINRA"));
    }

    #[test]
    fn test_universal_guards_not_empty() {
        assert!(!universal_guards().is_empty());
    }

    #[test]
    fn test_guards_to_patterns() {
        let guards = framework_guards("gdpr");
        let patterns = guards_to_patterns(&guards);
        assert!(!patterns.is_empty());
        assert!(patterns.iter().any(|(n, _)| n.contains("GDPR")));
    }
}
