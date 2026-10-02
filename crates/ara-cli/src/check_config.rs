//! `.ara-check.toml`: optional per-rule configuration for `ara check` (#40).
//!
//! The file selects which rules run, which of them `--fix` may apply, and at
//! what severity each is reported. Every key names rules by **selector**: a rule
//! code (`ARA107`) or a code prefix (`ARA1` = every `ARA1xx`, `ARA` = every
//! rule), in the style of `ruff`. When several selectors in one key pair match a
//! rule, the **longest** one decides; on a tie the negative key (`ignore` /
//! `unfixable`) wins.
//!
//! ```toml
//! select    = ["ARA"]          # default: every rule
//! ignore    = ["ARA212"]       # default: none
//! fixable   = ["ARA"]          # default: every fixable rule
//! unfixable = ["ARA001"]       # default: none
//!
//! [severity]                   # default: each rule's own severity
//! ARA2   = "error"             # promote every ARA2xx warning
//! ARA207 = "warning"           # ...except this one
//! ```
//!
//! Unknown keys, malformed selectors, and selectors that match no rule are
//! errors, so a typo never silently disables a check. With no config file the
//! resolved [`CheckConfig`] is [`CheckConfig::default`], which reproduces the
//! built-in (v1) behavior exactly.
//!
//! # Discovery
//!
//! [`discover`] walks up from the ARA directory toward the enclosing git
//! repository root (the first ancestor containing `.git`), inclusive, and
//! returns the first `.ara-check.toml` it finds. When the ARA directory is not
//! inside a git repository, only the ARA directory itself is searched.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use ara_core::{RuleCode, Severity};
use serde::Deserialize;

/// The config file name `ara check` discovers.
pub const CONFIG_FILE_NAME: &str = ".ara-check.toml";

/// Effective settings for one rule after the config is applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RuleSettings {
    /// Whether the rule's findings are reported at all.
    enabled: bool,
    /// Whether `--fix` may apply the rule's fix (always false for a rule that
    /// has no fix).
    fixable: bool,
    /// The severity the rule's findings are reported at.
    severity: Severity,
}

impl RuleSettings {
    fn builtin(rule: RuleCode) -> Self {
        Self {
            enabled: true,
            fixable: rule.fixable(),
            severity: rule.default_severity(),
        }
    }
}

/// Resolved per-rule configuration for one `ara check` run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckConfig {
    /// Settings for every rule in [`RuleCode::ALL`].
    rules: HashMap<RuleCode, RuleSettings>,
}

impl Default for CheckConfig {
    /// The built-in behavior: every rule enabled at its default severity, every
    /// fixable rule fixable.
    fn default() -> Self {
        Self {
            rules: RuleCode::ALL
                .iter()
                .map(|&r| (r, RuleSettings::builtin(r)))
                .collect(),
        }
    }
}

impl CheckConfig {
    fn settings(&self, rule: RuleCode) -> RuleSettings {
        self.rules
            .get(&rule)
            .copied()
            .unwrap_or_else(|| RuleSettings::builtin(rule))
    }

    /// Whether `rule`'s findings are reported.
    pub fn is_enabled(&self, rule: RuleCode) -> bool {
        self.settings(rule).enabled
    }

    /// Whether `--fix` may apply `rule`'s fix. False for a disabled rule.
    pub fn is_fixable(&self, rule: RuleCode) -> bool {
        let s = self.settings(rule);
        s.enabled && s.fixable
    }

    /// The severity `rule`'s findings are reported at.
    pub fn severity(&self, rule: RuleCode) -> Severity {
        self.settings(rule).severity
    }

    /// Parses and resolves config text. `origin` names the source in errors.
    pub fn from_toml(text: &str, origin: &Path) -> Result<Self, ConfigError> {
        let raw: RawConfig = toml::from_str(text).map_err(|e| ConfigError {
            path: origin.to_path_buf(),
            message: e.to_string(),
        })?;
        raw.resolve().map_err(|message| ConfigError {
            path: origin.to_path_buf(),
            message,
        })
    }

    /// Reads and resolves the config file at `path`.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path).map_err(|e| ConfigError {
            path: path.to_path_buf(),
            message: format!("cannot read config: {e}"),
        })?;
        Self::from_toml(&text, path)
    }
}

/// A config file that could not be read, parsed, or resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    /// The config file the error is about.
    pub path: PathBuf,
    /// What went wrong.
    pub message: String,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.message.trim_end())
    }
}

/// Finds the `.ara-check.toml` that applies to `ara_dir` (see the module docs
/// for the search order), or `None` when there is none.
pub fn discover(ara_dir: &Path) -> Option<PathBuf> {
    let start = ara_dir
        .canonicalize()
        .unwrap_or_else(|_| ara_dir.to_path_buf());
    let ancestors: Vec<&Path> = start.ancestors().collect();
    // Search up to and including the git root; outside a repo, only `ara_dir`.
    let last = ancestors
        .iter()
        .position(|d| d.join(".git").exists())
        .unwrap_or(0);
    ancestors[..=last]
        .iter()
        .map(|d| d.join(CONFIG_FILE_NAME))
        .find(|p| p.is_file())
}

// ---- raw file shape -------------------------------------------------------

/// The on-disk shape of `.ara-check.toml`. Unknown keys are rejected.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    select: Option<Vec<String>>,
    #[serde(default)]
    ignore: Vec<String>,
    fixable: Option<Vec<String>>,
    #[serde(default)]
    unfixable: Vec<String>,
    #[serde(default)]
    severity: BTreeMap<String, SeverityName>,
}

/// A severity as spelled in the config file.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum SeverityName {
    Error,
    Warning,
}

impl From<SeverityName> for Severity {
    fn from(s: SeverityName) -> Self {
        match s {
            SeverityName::Error => Severity::Error,
            SeverityName::Warning => Severity::Warning,
        }
    }
}

impl RawConfig {
    /// Validates every selector and computes the per-rule settings.
    fn resolve(self) -> Result<CheckConfig, String> {
        let all = ["ARA".to_string()];
        let select = selectors("select", self.select.as_deref().unwrap_or(&all))?;
        let ignore = selectors("ignore", &self.ignore)?;
        let fixable = selectors("fixable", self.fixable.as_deref().unwrap_or(&all))?;
        let unfixable = selectors("unfixable", &self.unfixable)?;
        let severity: Vec<(Selector, Severity)> = self
            .severity
            .into_iter()
            .map(|(key, sev)| Ok((Selector::parse("severity", &key)?, sev.into())))
            .collect::<Result<_, String>>()?;

        let rules = RuleCode::ALL
            .iter()
            .map(|&rule| {
                let builtin = RuleSettings::builtin(rule);
                let settings = RuleSettings {
                    enabled: decide(rule, &select, &ignore),
                    fixable: builtin.fixable && decide(rule, &fixable, &unfixable),
                    severity: severity
                        .iter()
                        .filter(|(sel, _)| sel.matches(rule))
                        .max_by_key(|(sel, _)| sel.specificity())
                        .map_or(builtin.severity, |&(_, sev)| sev),
                };
                (rule, settings)
            })
            .collect();
        Ok(CheckConfig { rules })
    }
}

/// The positive/negative decision for one rule: the longest matching selector
/// wins, the negative list wins a tie, and no positive match means "off".
fn decide(rule: RuleCode, positive: &[Selector], negative: &[Selector]) -> bool {
    let best = |list: &[Selector]| {
        list.iter()
            .filter(|s| s.matches(rule))
            .map(Selector::specificity)
            .max()
    };
    match (best(positive), best(negative)) {
        (Some(p), Some(n)) => p > n,
        (Some(_), None) => true,
        (None, _) => false,
    }
}

fn selectors(key: &str, raw: &[String]) -> Result<Vec<Selector>, String> {
    raw.iter().map(|s| Selector::parse(key, s)).collect()
}

/// A rule selector: `ARA` followed by up to three digits, matching every rule
/// whose code starts with it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Selector(String);

impl Selector {
    /// Parses `raw` (from config key `key`), rejecting malformed selectors and
    /// selectors that match no known rule.
    fn parse(key: &str, raw: &str) -> Result<Self, String> {
        let well_formed = raw
            .strip_prefix("ARA")
            .is_some_and(|digits| digits.len() <= 3 && digits.bytes().all(|b| b.is_ascii_digit()));
        if !well_formed {
            return Err(format!(
                "`{key}`: invalid rule selector `{raw}` (expected a rule code like `ARA107` \
                 or a code prefix like `ARA1`)"
            ));
        }
        let sel = Self(raw.to_string());
        if !RuleCode::ALL.iter().any(|&r| sel.matches(r)) {
            return Err(format!(
                "`{key}`: unknown rule code `{raw}` (matches no rule)"
            ));
        }
        Ok(sel)
    }

    fn matches(&self, rule: RuleCode) -> bool {
        rule.as_str().starts_with(&self.0)
    }

    fn specificity(&self) -> usize {
        self.0.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<CheckConfig, ConfigError> {
        CheckConfig::from_toml(text, Path::new(CONFIG_FILE_NAME))
    }

    #[test]
    fn empty_file_equals_builtin_defaults() {
        assert_eq!(parse("").unwrap(), CheckConfig::default());
        let cfg = CheckConfig::default();
        for &r in RuleCode::ALL {
            assert!(cfg.is_enabled(r));
            assert_eq!(cfg.is_fixable(r), r.fixable());
            assert_eq!(cfg.severity(r), r.default_severity());
        }
    }

    #[test]
    fn ignore_exact_code_and_prefix() {
        let cfg = parse(r#"ignore = ["ARA212", "ARA0"]"#).unwrap();
        assert!(!cfg.is_enabled(RuleCode::RelatedWorkMissingDoi));
        assert!(!cfg.is_enabled(RuleCode::RootDialect));
        assert!(!cfg.is_enabled(RuleCode::PivotTriggerAlias));
        assert!(cfg.is_enabled(RuleCode::ConceptMissingDefinition));
        assert!(cfg.is_enabled(RuleCode::UnknownEvidenceClaim));
        // A disabled rule is never fixable.
        assert!(!cfg.is_fixable(RuleCode::RootDialect));
    }

    #[test]
    fn select_restricts_and_longest_selector_wins() {
        let cfg = parse(r#"select = ["ARA1", "ARA212"]"#).unwrap();
        assert!(cfg.is_enabled(RuleCode::DependencyCycle));
        assert!(cfg.is_enabled(RuleCode::RelatedWorkMissingDoi));
        assert!(!cfg.is_enabled(RuleCode::ConceptMissingDefinition));
        assert!(!cfg.is_enabled(RuleCode::RootDialect));

        // A more specific select re-enables a rule a broader ignore turned off.
        let cfg = parse("select = [\"ARA\", \"ARA211\"]\nignore = [\"ARA2\"]").unwrap();
        assert!(cfg.is_enabled(RuleCode::ConceptMissingDefinition));
        assert!(!cfg.is_enabled(RuleCode::RelatedWorkMissingDoi));

        // Equal specificity: ignore wins.
        let cfg = parse("select = [\"ARA212\"]\nignore = [\"ARA212\"]").unwrap();
        assert!(!cfg.is_enabled(RuleCode::RelatedWorkMissingDoi));

        // An explicit empty select turns everything off.
        let cfg = parse("select = []").unwrap();
        assert!(RuleCode::ALL.iter().all(|&r| !cfg.is_enabled(r)));
    }

    #[test]
    fn fixable_and_unfixable() {
        let cfg = parse(r#"unfixable = ["ARA001"]"#).unwrap();
        assert!(!cfg.is_fixable(RuleCode::RootDialect));
        assert!(cfg.is_enabled(RuleCode::RootDialect));
        assert!(cfg.is_fixable(RuleCode::DeadEndReasonAlias));

        let cfg = parse(r#"fixable = ["ARA004"]"#).unwrap();
        assert!(cfg.is_fixable(RuleCode::ClaimHeaderStyle));
        assert!(!cfg.is_fixable(RuleCode::RootDialect));

        // `fixable` cannot make a rule without a fix fixable.
        let cfg = parse(r#"fixable = ["ARA107"]"#).unwrap();
        assert!(!cfg.is_fixable(RuleCode::UnknownEvidenceClaim));
    }

    #[test]
    fn severity_overrides_with_prefix_precedence() {
        let cfg =
            parse("[severity]\nARA2 = \"error\"\nARA207 = \"warning\"\nARA105 = \"warning\"\n")
                .unwrap();
        assert_eq!(
            cfg.severity(RuleCode::RelatedWorkMissingDoi),
            Severity::Error
        );
        assert_eq!(
            cfg.severity(RuleCode::UnresolvedClaimReference),
            Severity::Warning
        );
        assert_eq!(cfg.severity(RuleCode::DuplicateNodeId), Severity::Warning);
        assert_eq!(cfg.severity(RuleCode::DependencyCycle), Severity::Error);
    }

    #[test]
    fn unknown_codes_and_keys_are_errors() {
        let err = parse(r#"ignore = ["ARA999"]"#).unwrap_err();
        assert!(err.message.contains("unknown rule code `ARA999`"), "{err}");
        let err = parse(r#"select = ["ARA3"]"#).unwrap_err();
        assert!(err.message.contains("unknown rule code `ARA3`"), "{err}");
        let err = parse(r#"ignore = ["ara107"]"#).unwrap_err();
        assert!(
            err.message.contains("invalid rule selector `ara107`"),
            "{err}"
        );
        let err = parse(r#"ignore = ["ARA1070"]"#).unwrap_err();
        assert!(err.message.contains("invalid rule selector"), "{err}");
        let err = parse("[severity]\nARA9 = \"error\"").unwrap_err();
        assert!(
            err.message.contains("`severity`: unknown rule code"),
            "{err}"
        );

        let err = parse(r#"ignores = ["ARA212"]"#).unwrap_err();
        assert!(err.message.contains("ignores"), "{err}");
        let err = parse("[severity]\nARA212 = \"fatal\"").unwrap_err();
        assert!(err.message.contains("fatal"), "{err}");
        assert!(err.to_string().starts_with(CONFIG_FILE_NAME), "{err}");
    }

    #[test]
    fn discover_walks_up_to_git_root() {
        let root = tempfile::TempDir::new().unwrap();
        let repo = root.path().join("repo");
        let ara = repo.join("papers/my-ara");
        std::fs::create_dir_all(&ara).unwrap();
        std::fs::create_dir_all(repo.join(".git")).unwrap();

        assert_eq!(discover(&ara), None);

        // Above the git root: not found.
        std::fs::write(root.path().join(CONFIG_FILE_NAME), "").unwrap();
        assert_eq!(discover(&ara), None);

        // At the git root: found.
        let at_root = repo.join(CONFIG_FILE_NAME);
        std::fs::write(&at_root, "").unwrap();
        assert_eq!(discover(&ara), Some(at_root.canonicalize().unwrap()));

        // In the ARA dir: the nearest one wins.
        let in_ara = ara.join(CONFIG_FILE_NAME);
        std::fs::write(&in_ara, "").unwrap();
        assert_eq!(discover(&ara), Some(in_ara.canonicalize().unwrap()));
    }

    #[test]
    fn discover_outside_a_repo_checks_only_the_ara_dir() {
        let root = tempfile::TempDir::new().unwrap();
        let ara = root.path().join("my-ara");
        std::fs::create_dir_all(&ara).unwrap();
        std::fs::write(root.path().join(CONFIG_FILE_NAME), "").unwrap();
        // The tempdir may itself live inside some repo on the host; only assert
        // the no-repo behavior when it does not.
        let in_repo = ara.ancestors().any(|d| d.join(".git").exists());
        if !in_repo {
            assert_eq!(discover(&ara), None);
        }
        let in_ara = ara.join(CONFIG_FILE_NAME);
        std::fs::write(&in_ara, "").unwrap();
        assert_eq!(discover(&ara), Some(in_ara.canonicalize().unwrap()));
    }
}
