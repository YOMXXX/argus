//! Argus 内核 —— 模型无关的 Provider 抽象与 Agent 主循环。

pub mod agent;
pub(crate) mod anthropic;
pub mod provider;
pub mod types;

pub use agent::Agent;
pub use anthropic::AnthropicProvider;
pub use provider::{MockProvider, Provider};
pub use types::{CompletionRequest, CompletionResponse, Message, Role, Usage};
