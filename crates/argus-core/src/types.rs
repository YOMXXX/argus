//! Argus 核心数据类型：消息、内容块、补全请求/响应、工具规格、用量。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role { System, User, Assistant }

/// 一条消息里的内容块（支持文本与工具交互）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Content {
    Text { text: String },
    ToolUse { id: String, name: String, input: serde_json::Value },
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(default)]
        is_error: bool,
    },
}

/// 一条对话消息（内容为若干内容块）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Vec<Content>,
}

impl Message {
    pub fn system(text: impl Into<String>) -> Self {
        Self { role: Role::System, content: vec![Content::Text { text: text.into() }] }
    }
    pub fn user(text: impl Into<String>) -> Self {
        Self { role: Role::User, content: vec![Content::Text { text: text.into() }] }
    }
    pub fn assistant(text: impl Into<String>) -> Self {
        Self { role: Role::Assistant, content: vec![Content::Text { text: text.into() }] }
    }
    /// 取该消息所有 Text 块拼接（用于估算/展示）。
    pub fn text(&self) -> String {
        self.content.iter().filter_map(|c| match c {
            Content::Text { text } => Some(text.as_str()),
            _ => None,
        }).collect::<Vec<_>>().join("")
    }
}

/// 暴露给模型的工具规格。
#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// 模型请求调用的一个工具。
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

/// 模型停止原因。
#[derive(Debug, Clone, PartialEq)]
pub enum StopReason { EndTurn, ToolUse, Other }

#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolSpec>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Usage { pub prompt_tokens: u64, pub completion_tokens: u64 }

#[derive(Debug, Clone, PartialEq)]
pub struct CompletionResponse {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub usage: Usage,
    pub stop_reason: StopReason,
}
