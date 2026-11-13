use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use regex::Regex;
use serde::{Deserialize, Serialize};

use super::config::{self, RouteMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RouteDecision {
    Direct,
    Proxy,
}

#[derive(Debug, Clone, Default)]
struct RuleSet {
    patterns: Vec<Regex>,
    raw: HashSet<String>,
}

impl RuleSet {
    fn parse(text: &str) -> Self {
        let lines: Vec<&str> = text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .collect();
        let patterns = lines
            .iter()
            .filter_map(|line| Regex::new(&format!("(?i){line}")).ok())
            .collect();
        let raw = lines.into_iter().map(str::to_ascii_lowercase).collect();
        Self { patterns, raw }
    }

    fn matches(&self, host: &str) -> bool {
        self.patterns.iter().any(|pattern| pattern.is_match(host))
    }

    fn has_exact(&self, host: &str) -> bool {
        self.raw
            .contains(&format!("^{}$", regex::escape(host)).to_ascii_lowercase())
    }
}

#[derive(Debug, Clone)]
pub struct RuleEngine {
    user_proxy: RuleSet,
    user_local: RuleSet,
    system_proxy: RuleSet,
    system_local: RuleSet,
    default: RouteDecision,
    mode: RouteMode,
}

impl Default for RuleEngine {
    fn default() -> Self {
        Self {
            user_proxy: RuleSet::default(),
            user_local: RuleSet::default(),
            system_proxy: RuleSet::default(),
            system_local: RuleSet::default(),
            default: RouteDecision::Proxy,
            mode: RouteMode::Smart,
        }
    }
}

impl RuleEngine {
    pub fn from_text(
        system_proxy: &str,
        system_local: &str,
        user_proxy: &str,
        user_local: &str,
        default: RouteDecision,
    ) -> Self {
        Self {
            user_proxy: RuleSet::parse(user_proxy),
            user_local: RuleSet::parse(user_local),
            system_proxy: RuleSet::parse(system_proxy),
            system_local: RuleSet::parse(system_local),
            default,
            mode: RouteMode::Smart,
        }
    }

    pub fn with_mode(mut self, mode: RouteMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn set_mode(&mut self, mode: RouteMode) {
        self.mode = mode;
    }

    pub fn from_files(
        system_proxy: &Path,
        system_local: &Path,
        user_proxy: &Path,
        user_local: &Path,
        default: RouteDecision,
    ) -> Self {
        Self::from_text(
            &std::fs::read_to_string(system_proxy).unwrap_or_default(),
            &std::fs::read_to_string(system_local).unwrap_or_default(),
            &std::fs::read_to_string(user_proxy).unwrap_or_default(),
            &std::fs::read_to_string(user_local).unwrap_or_default(),
            default,
        )
    }

    pub fn decide(&self, host: &str) -> RouteDecision {
        let host = host.trim_end_matches('.').to_ascii_lowercase();
        if is_local_destination(&host) || is_infrastructure_direct(&host) {
            return RouteDecision::Direct;
        }
        match self.mode {
            RouteMode::Direct => return RouteDecision::Direct,
            RouteMode::Global => return RouteDecision::Proxy,
            RouteMode::Smart => {}
        }
        if self.user_proxy.has_exact(&host) {
            return RouteDecision::Proxy;
        }
        if self.user_local.has_exact(&host) {
            return RouteDecision::Direct;
        }
        if self.user_proxy.matches(&host) {
            return RouteDecision::Proxy;
        }
        if self.user_local.matches(&host) {
            return RouteDecision::Direct;
        }
        if self.system_local.matches(&host) {
            return RouteDecision::Direct;
        }
        if self.system_proxy.matches(&host) {
            return RouteDecision::Proxy;
        }
        self.default
    }

    pub fn load_cached(mode: RouteMode) -> Self {
        let paths = rule_paths();
        Self::from_files(
            &paths.system_proxy,
            &paths.system_local,
            &paths.user_proxy,
            &paths.user_local,
            RouteDecision::Proxy,
        )
        .with_mode(mode)
    }

    pub async fn load_or_refresh(
        system_proxy_url: &str,
        system_local_url: &str,
        default: RouteDecision,
    ) -> Result<Self, String> {
        let paths = rule_paths();
        std::fs::create_dir_all(
            paths
                .system_proxy
                .parent()
                .ok_or_else(|| String::from("rule path has no parent"))?,
        )
        .map_err(|e| format!("create rule directory: {e}"))?;

        refresh_rule(system_proxy_url, &paths.system_proxy).await;
        refresh_rule(system_local_url, &paths.system_local).await;
        for user_path in [&paths.user_proxy, &paths.user_local] {
            if !user_path.exists() {
                std::fs::write(user_path, b"")
                    .map_err(|e| format!("create {}: {e}", user_path.display()))?;
            }
        }
        Ok(Self::from_files(
            &paths.system_proxy,
            &paths.system_local,
            &paths.user_proxy,
            &paths.user_local,
            default,
        ))
    }
}

fn is_infrastructure_direct(host: &str) -> bool {
    const WINDOWS_CONNECTIVITY: &[&str] = &[
        "dns.msftncsi.com",
        "www.msftconnecttest.com",
        "www.msftncsi.com",
        "ipv6.msftconnecttest.com",
    ];
    const CERTIFICATE_SUFFIXES: &[&str] =
        &[".o.lencr.org", ".c.lencr.org", ".pki.goog", ".msocsp.com"];

    if WINDOWS_CONNECTIVITY
        .iter()
        .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
        || CERTIFICATE_SUFFIXES
            .iter()
            .any(|suffix| host.ends_with(suffix))
    {
        return true;
    }

    host.split('.').next().is_some_and(|label| {
        label == "ocsp"
            || label.starts_with("ocsp-")
            || label == "crl"
            || label.starts_with("crl-")
            || label.strip_prefix("crl").is_some_and(|suffix| {
                !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit())
            })
    }) || host.contains(".ocsp.")
        || host.contains(".crl.")
}

fn is_local_destination(host: &str) -> bool {
    if host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.ends_with(".lan")
        || host.ends_with(".home.arpa")
    {
        return true;
    }
    let Ok(address) = host.parse::<std::net::IpAddr>() else {
        return false;
    };
    match address {
        std::net::IpAddr::V4(address) => {
            let [a, b, ..] = address.octets();
            a == 0
                || a == 10
                || a == 127
                || (a == 169 && b == 254)
                || (a == 172 && (16..=31).contains(&b))
                || (a == 192 && b == 168)
        }
        std::net::IpAddr::V6(address) => {
            let bytes = address.octets();
            address.is_loopback()
                || address.is_unspecified()
                || bytes[0] & 0xfe == 0xfc
                || (bytes[0] == 0xfe && bytes[1] & 0xc0 == 0x80)
        }
    }
}

pub struct RulePaths {
    pub system_proxy: PathBuf,
    pub system_local: PathBuf,
    pub user_proxy: PathBuf,
    pub user_local: PathBuf,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct UserRules {
    pub proxy: String,
    pub direct: String,
}

pub fn rule_paths() -> RulePaths {
    let directory = config::app_dir().join("rules");
    RulePaths {
        system_proxy: directory.join("sys_proxy.txt"),
        system_local: directory.join("sys_local.txt"),
        user_proxy: directory.join("user_proxy.txt"),
        user_local: directory.join("user_local.txt"),
    }
}

pub fn read_user_rules() -> Result<UserRules, String> {
    let paths = rule_paths();
    Ok(UserRules {
        proxy: std::fs::read_to_string(paths.user_proxy).unwrap_or_default(),
        direct: std::fs::read_to_string(paths.user_local).unwrap_or_default(),
    })
}

pub fn write_user_rules(rules: &UserRules) -> Result<(), String> {
    let paths = rule_paths();
    let directory = paths
        .user_proxy
        .parent()
        .ok_or_else(|| String::from("rule path has no parent"))?;
    std::fs::create_dir_all(directory).map_err(|e| format!("create rule directory: {e}"))?;
    std::fs::write(paths.user_proxy, &rules.proxy)
        .map_err(|e| format!("write user proxy rules: {e}"))?;
    std::fs::write(paths.user_local, &rules.direct)
        .map_err(|e| format!("write user direct rules: {e}"))
}

/// Domains pinned by the per-row buttons. Broad hand-written patterns are not
/// listed here because the buttons can neither represent nor remove them.
#[derive(Debug, Clone, Default, Serialize)]
pub struct PinnedDomains {
    pub proxy: Vec<String>,
    pub direct: Vec<String>,
}

fn normalize_domain(domain: &str) -> Result<String, String> {
    let domain = domain.trim().trim_end_matches('.').to_ascii_lowercase();
    if domain.is_empty()
        || domain.len() > 253
        || domain
            .chars()
            .any(|character| character.is_whitespace() || "/\\?#".contains(character))
    {
        return Err(String::from("invalid domain for route rule"));
    }
    Ok(domain)
}

pub fn set_domain_route(domain: &str, route: RouteDecision) -> Result<UserRules, String> {
    let domain = normalize_domain(domain)?;
    let pattern = format!("^{}$", regex::escape(&domain));
    let mut rules = read_user_rules()?;
    rules.proxy = remove_exact_rule(&rules.proxy, &pattern);
    rules.direct = remove_exact_rule(&rules.direct, &pattern);
    match route {
        RouteDecision::Proxy => append_rule(&mut rules.proxy, &pattern),
        RouteDecision::Direct => append_rule(&mut rules.direct, &pattern),
    }
    write_user_rules(&rules)?;
    Ok(rules)
}

/// Drops both pins for a domain so it falls back to the system rules.
pub fn clear_domain_route(domain: &str) -> Result<UserRules, String> {
    let domain = normalize_domain(domain)?;
    let pattern = format!("^{}$", regex::escape(&domain));
    let mut rules = read_user_rules()?;
    rules.proxy = remove_exact_rule(&rules.proxy, &pattern);
    rules.direct = remove_exact_rule(&rules.direct, &pattern);
    write_user_rules(&rules)?;
    Ok(rules)
}

pub fn pinned_domains() -> Result<PinnedDomains, String> {
    let rules = read_user_rules()?;
    Ok(PinnedDomains {
        proxy: rules.proxy.lines().filter_map(exact_rule_domain).collect(),
        direct: rules.direct.lines().filter_map(exact_rule_domain).collect(),
    })
}

/// Inverse of the `^escaped$` form the buttons write. Anything that is not a
/// plain anchored hostname returns `None` and stays unlisted.
fn exact_rule_domain(line: &str) -> Option<String> {
    let body = line.trim().strip_prefix('^')?.strip_suffix('$')?;
    let mut domain = String::with_capacity(body.len());
    let mut characters = body.chars();
    while let Some(character) = characters.next() {
        let character = if character == '\\' {
            characters.next()?
        } else {
            character
        };
        if !character.is_ascii_alphanumeric() && !"-._".contains(character) {
            return None;
        }
        domain.push(character.to_ascii_lowercase());
    }
    (!domain.is_empty()).then_some(domain)
}

fn remove_exact_rule(text: &str, pattern: &str) -> String {
    text.lines()
        .filter(|line| !line.trim().eq_ignore_ascii_case(pattern))
        .collect::<Vec<_>>()
        .join("\n")
}

fn append_rule(text: &mut String, pattern: &str) {
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(pattern);
    text.push('\n');
}

async fn refresh_rule(url: &str, path: &Path) {
    if url.is_empty() {
        return;
    }
    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
    else {
        return;
    };
    let Ok(response) = client.get(url).send().await else {
        return;
    };
    if !response.status().is_success() {
        return;
    }
    let Ok(text) = response.text().await else {
        return;
    };
    if !text.trim().is_empty() {
        let _ = std::fs::write(path, text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_user_then_system_priority() {
        let rules = RuleEngine::from_text(
            ".*google.*",
            ".*\\.cn$",
            "force\\.cn$",
            "local\\.google\\.com$",
            RouteDecision::Direct,
        );
        assert_eq!(rules.decide("force.cn"), RouteDecision::Proxy);
        assert_eq!(rules.decide("local.google.com"), RouteDecision::Direct);
        assert_eq!(rules.decide("www.google.com"), RouteDecision::Proxy);
        assert_eq!(rules.decide("www.baidu.cn"), RouteDecision::Direct);
        assert_eq!(rules.decide("unknown.test"), RouteDecision::Direct);
    }

    #[test]
    fn exact_local_override_beats_broad_proxy_rule() {
        let rules = RuleEngine::from_text(
            "",
            "",
            ".*\\.example\\.com$",
            "^api\\.example\\.com$",
            RouteDecision::Proxy,
        );
        assert_eq!(rules.decide("api.example.com"), RouteDecision::Direct);
        assert_eq!(rules.decide("www.example.com"), RouteDecision::Proxy);
    }

    #[test]
    fn pinned_domain_survives_the_escape_round_trip() {
        for domain in ["api2.cursor.sh", "api-2.example.com", "a.b-c.d.test"] {
            let pattern = format!("^{}$", regex::escape(domain));
            assert_eq!(exact_rule_domain(&pattern).as_deref(), Some(domain));
        }
    }

    #[test]
    fn hand_written_patterns_are_not_reported_as_pinned() {
        for line in ["# comment", ".*\\.example\\.com$", "^.*google.*$", ""] {
            assert_eq!(exact_rule_domain(line), None, "{line} must stay unlisted");
        }
    }

    #[test]
    fn generated_exact_rule_is_idempotent() {
        let pattern = "^api\\.example\\.com$";
        let mut text = format!("# user\n{pattern}\n{pattern}\n");
        text = remove_exact_rule(&text, pattern);
        append_rule(&mut text, pattern);
        assert_eq!(text.matches(pattern).count(), 1);
    }

    #[test]
    fn route_modes_are_distinct_and_keep_lan_direct() {
        let smart = RuleEngine::from_text("proxy\\.test$", "", "", "", RouteDecision::Direct);
        assert_eq!(smart.decide("proxy.test"), RouteDecision::Proxy);
        assert_eq!(smart.decide("other.test"), RouteDecision::Direct);

        let global = smart.clone().with_mode(RouteMode::Global);
        assert_eq!(global.decide("other.test"), RouteDecision::Proxy);
        assert_eq!(global.decide("192.168.1.1"), RouteDecision::Direct);

        let direct = smart.with_mode(RouteMode::Direct);
        assert_eq!(direct.decide("proxy.test"), RouteDecision::Direct);
        assert_eq!(direct.decide("localhost"), RouteDecision::Direct);
    }

    #[test]
    fn infrastructure_checks_bypass_even_global_mode() {
        let global = RuleEngine::default().with_mode(RouteMode::Global);
        for host in [
            "www.msftconnecttest.com",
            "dns.msftncsi.com",
            "ocsp.digicert.com",
            "crl3.digicert.com",
            "r10.o.lencr.org",
            "ocsp.pki.goog",
        ] {
            assert_eq!(
                global.decide(host),
                RouteDecision::Direct,
                "{host} must bypass the proxy"
            );
        }
        assert_eq!(global.decide("www.example.com"), RouteDecision::Proxy);
    }
}
