//! Configurable permission rules for auto-approving or auto-denying tool calls.

/// Decision returned by `PermissionChecker::check()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    /// Auto-approve — skip the confirmation prompt.
    Allow,
    /// Auto-deny — block execution without prompting.
    Deny,
    /// No rule matched — fall through to default behaviour (prompt if dangerous).
    Prompt,
}

#[derive(Debug, Clone)]
struct PermissionRule {
    tool: String,
    pattern: Option<String>,
}

/// Evaluates `allow` / `deny` rules from `[permissions]` config.
#[derive(Debug, Clone, Default)]
pub struct PermissionChecker {
    allow: Vec<PermissionRule>,
    deny: Vec<PermissionRule>,
}

impl PermissionChecker {
    /// Build from the raw string vectors in `PermissionsConfig`.
    pub fn new(allow: &[String], deny: &[String]) -> Self {
        Self {
            allow: allow.iter().map(|s| parse_rule(s)).collect(),
            deny: deny.iter().map(|s| parse_rule(s)).collect(),
        }
    }

    /// Check whether a tool call should be auto-allowed, auto-denied, or
    /// prompted. Evaluation order: deny → allow → Prompt.
    pub fn check(&self, tool_name: &str, input: &serde_json::Value) -> Permission {
        let key_arg = extract_key_arg(tool_name, input);

        // Deny takes precedence.
        for rule in &self.deny {
            if rule.matches(tool_name, key_arg.as_deref()) {
                return Permission::Deny;
            }
        }

        for rule in &self.allow {
            if rule.matches(tool_name, key_arg.as_deref()) {
                return Permission::Allow;
            }
        }

        Permission::Prompt
    }
}

impl PermissionRule {
    fn matches(&self, tool_name: &str, key_arg: Option<&str>) -> bool {
        // "mcp" is a special tool prefix that matches any MCP tool (containing "__").
        if self.tool == "mcp" {
            if !tool_name.contains("__") {
                return false;
            }
            return match &self.pattern {
                None => true,
                Some(pat) => glob_match(pat, tool_name),
            };
        }

        if self.tool != tool_name {
            return false;
        }
        match &self.pattern {
            None => true,
            Some(pat) => key_arg.is_some_and(|arg| glob_match(pat, arg)),
        }
    }
}

/// Parse `"bash:git *"` into `PermissionRule { tool: "bash", pattern: Some("git *") }`.
/// Bare `"read"` → `{ tool: "read", pattern: None }`.
fn parse_rule(s: &str) -> PermissionRule {
    let s = s.trim();
    if let Some((tool, pat)) = s.split_once(':') {
        PermissionRule {
            tool: tool.trim().to_string(),
            pattern: Some(pat.trim().to_string()),
        }
    } else {
        PermissionRule {
            tool: s.to_string(),
            pattern: None,
        }
    }
}

/// Extract the key argument from a tool call's JSON input, matching the same
/// logic as `tool_call_summary()`.
fn extract_key_arg(tool_name: &str, input: &serde_json::Value) -> Option<String> {
    let key_field = match tool_name {
        "bash" => "command",
        "write" => "file_path",
        "edit" => "file_path",
        "web_fetch" => "url",
        "web_search" => "query",
        _ => "",
    };

    if !key_field.is_empty() {
        if let Some(val) = input.get(key_field).and_then(|v| v.as_str()) {
            return Some(val.to_string());
        }
    }

    // MCP tools (server__tool): match against the tool name after "__".
    if tool_name.contains("__") {
        return Some(tool_name.to_string());
    }

    None
}

/// Simple glob matching supporting `*` (any chars) and `?` (single char).
fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let pn = p.len();
    let tn = t.len();

    // DP table: dp[i][j] = pattern[..i] matches text[..j]
    let mut dp = vec![vec![false; tn + 1]; pn + 1];
    dp[0][0] = true;

    // Leading *s match empty string.
    for i in 1..=pn {
        if p[i - 1] == '*' {
            dp[i][0] = dp[i - 1][0];
        } else {
            break;
        }
    }

    for i in 1..=pn {
        for j in 1..=tn {
            match p[i - 1] {
                '*' => dp[i][j] = dp[i - 1][j] || dp[i][j - 1],
                '?' => dp[i][j] = dp[i - 1][j - 1],
                c => dp[i][j] = c == t[j - 1] && dp[i - 1][j - 1],
            }
        }
    }

    dp[pn][tn]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_exact_match() {
        assert!(glob_match("cargo test", "cargo test"));
        assert!(!glob_match("cargo test", "cargo build"));
    }

    #[test]
    fn glob_star_matches_any() {
        assert!(glob_match("git *", "git status"));
        assert!(glob_match("git *", "git commit -m \"hello\""));
        assert!(!glob_match("git *", "ls"));
    }

    #[test]
    fn glob_star_matches_path() {
        assert!(glob_match("*.rs", "/foo/bar.rs"));
        assert!(glob_match("*.rs", "main.rs"));
        assert!(!glob_match("*.rs", "/foo/bar.toml"));
    }

    #[test]
    fn glob_question_mark() {
        assert!(glob_match("file?.txt", "file1.txt"));
        assert!(!glob_match("file?.txt", "file12.txt"));
    }

    #[test]
    fn parse_rule_bare_tool() {
        let r = parse_rule("read");
        assert_eq!(r.tool, "read");
        assert!(r.pattern.is_none());
    }

    #[test]
    fn parse_rule_tool_with_pattern() {
        let r = parse_rule("bash:git *");
        assert_eq!(r.tool, "bash");
        assert_eq!(r.pattern.as_deref(), Some("git *"));
    }

    #[test]
    fn check_allow_bash_git() {
        let checker = PermissionChecker::new(&["bash:git *".to_string()], &[]);
        assert_eq!(
            checker.check("bash", &serde_json::json!({"command": "git status"})),
            Permission::Allow
        );
        assert_eq!(
            checker.check("bash", &serde_json::json!({"command": "ls"})),
            Permission::Prompt
        );
    }

    #[test]
    fn check_allow_edit_rs() {
        let checker = PermissionChecker::new(&["edit:*.rs".to_string()], &[]);
        assert_eq!(
            checker.check("edit", &serde_json::json!({"file_path": "/foo/bar.rs"})),
            Permission::Allow
        );
        assert_eq!(
            checker.check("edit", &serde_json::json!({"file_path": "/foo/bar.toml"})),
            Permission::Prompt
        );
    }

    #[test]
    fn check_allow_bare_tool() {
        let checker = PermissionChecker::new(&["read".to_string()], &[]);
        assert_eq!(
            checker.check("read", &serde_json::json!({})),
            Permission::Allow
        );
    }

    #[test]
    fn check_deny_takes_precedence() {
        let checker = PermissionChecker::new(
            &["bash:git *".to_string()],
            &["bash:git push *".to_string()],
        );
        assert_eq!(
            checker.check(
                "bash",
                &serde_json::json!({"command": "git push origin main"})
            ),
            Permission::Deny
        );
        assert_eq!(
            checker.check("bash", &serde_json::json!({"command": "git status"})),
            Permission::Allow
        );
    }

    #[test]
    fn check_no_rules_is_prompt() {
        let checker = PermissionChecker::new(&[], &[]);
        assert_eq!(
            checker.check("bash", &serde_json::json!({"command": "ls"})),
            Permission::Prompt
        );
    }

    #[test]
    fn check_mcp_tool_matching() {
        let checker = PermissionChecker::new(&["mcp:github__*".to_string()], &[]);
        // MCP tool name is used as the key arg
        assert_eq!(
            checker.check("github__create_issue", &serde_json::json!({})),
            Permission::Allow
        );
        assert_eq!(
            checker.check("github__list_prs", &serde_json::json!({})),
            Permission::Allow
        );
        assert_eq!(
            checker.check("slack__send_message", &serde_json::json!({})),
            Permission::Prompt
        );
    }

    #[test]
    fn check_deny_rm_rf() {
        let checker = PermissionChecker::new(&[], &["bash:rm -rf *".to_string()]);
        assert_eq!(
            checker.check("bash", &serde_json::json!({"command": "rm -rf /tmp/thing"})),
            Permission::Deny
        );
        assert_eq!(
            checker.check("bash", &serde_json::json!({"command": "rm file.txt"})),
            Permission::Prompt
        );
    }
}
