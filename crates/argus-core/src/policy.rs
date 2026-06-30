//! Sandbox policy decisions for tool execution.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationKind {
    Read,
    Write,
    Shell,
    Mcp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyMode {
    WorkspaceWrite,
    ReadOnly,
    Trusted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyAction {
    Allow,
    Ask,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyDecision {
    pub action: PolicyAction,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxPolicy {
    mode: PolicyMode,
}

impl SandboxPolicy {
    pub fn workspace_write() -> Self {
        Self {
            mode: PolicyMode::WorkspaceWrite,
        }
    }

    pub fn read_only() -> Self {
        Self {
            mode: PolicyMode::ReadOnly,
        }
    }

    pub fn trusted() -> Self {
        Self {
            mode: PolicyMode::Trusted,
        }
    }

    pub fn decide(&self, operation: OperationKind, tool_name: &str) -> PolicyDecision {
        match (self.mode, operation) {
            (PolicyMode::Trusted, _) => PolicyDecision {
                action: PolicyAction::Allow,
                reason: format!("trusted policy allows {tool_name}"),
            },
            (PolicyMode::ReadOnly, OperationKind::Read) => PolicyDecision {
                action: PolicyAction::Allow,
                reason: "read-only policy allows read-only tools".into(),
            },
            (PolicyMode::ReadOnly, _) => PolicyDecision {
                action: PolicyAction::Deny,
                reason: format!("read-only policy denies {tool_name}"),
            },
            (PolicyMode::WorkspaceWrite, OperationKind::Read | OperationKind::Write) => {
                PolicyDecision {
                    action: PolicyAction::Allow,
                    reason: format!("workspace-write policy allows {tool_name}"),
                }
            }
            (PolicyMode::WorkspaceWrite, OperationKind::Shell | OperationKind::Mcp) => {
                PolicyDecision {
                    action: PolicyAction::Ask,
                    reason: format!("workspace-write policy requires approval for {tool_name}"),
                }
            }
        }
    }
}

impl OperationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Shell => "shell",
            Self::Mcp => "mcp",
        }
    }
}

impl PolicyAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Ask => "ask",
            Self::Deny => "deny",
        }
    }
}

impl Default for SandboxPolicy {
    fn default() -> Self {
        Self::workspace_write()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_write_allows_file_writes_but_asks_for_shell() {
        let policy = SandboxPolicy::workspace_write();

        assert_eq!(
            policy.decide(OperationKind::Write, "write_file").action,
            PolicyAction::Allow
        );
        assert_eq!(
            policy.decide(OperationKind::Shell, "run_shell").action,
            PolicyAction::Ask
        );
    }

    #[test]
    fn read_only_denies_write_shell_and_mcp_tools() {
        let policy = SandboxPolicy::read_only();

        assert_eq!(
            policy.decide(OperationKind::Read, "read_file").action,
            PolicyAction::Allow
        );
        assert_eq!(
            policy.decide(OperationKind::Write, "write_file").action,
            PolicyAction::Deny
        );
        assert_eq!(
            policy.decide(OperationKind::Shell, "run_shell").action,
            PolicyAction::Deny
        );
        assert_eq!(
            policy.decide(OperationKind::Mcp, "external").action,
            PolicyAction::Deny
        );
    }
}
