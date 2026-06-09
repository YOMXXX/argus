//! 工具执行审批。

/// 决定某个（需审批的）工具调用是否放行。
pub trait Approver: Send + Sync {
    fn approve(&self, tool_name: &str, args: &str) -> bool;
}

/// 总是放行（用于 --yes 或测试）。
pub struct AutoApprover;
impl Approver for AutoApprover {
    fn approve(&self, _tool_name: &str, _args: &str) -> bool { true }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn auto_approver_always_true() {
        assert!(AutoApprover.approve("run_shell", "{}"));
    }
}
