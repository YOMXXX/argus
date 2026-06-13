//! MCP(Model Context Protocol)client —— 连接外部 MCP server,复用其工具。
//!
//! stdio transport:每条 JSON-RPC 2.0 消息一行(`\n` 分隔)。

use anyhow::Result;
use crate::tool::Tool;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout, Command};
use tokio::process::Child;
use tokio::sync::Mutex;

/// 一个 MCP server 暴露的工具定义。
#[derive(Debug, Clone)]
pub struct McpToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

/// 一个 MCP client 连接(泛型 transport;生产用子进程 stdio,测试用内存 duplex)。
pub struct McpClient<R, W> {
    reader: BufReader<R>,
    writer: W,
    next_id: u64,
    /// 持有子进程(若由 spawn 创建);Drop 时杀掉,避免遗留。
    child: Option<Child>,
}

impl<R: AsyncRead + Unpin, W: AsyncWrite + Unpin> McpClient<R, W> {
    /// 用给定读写端构造(不绑定子进程;spawn 会另行设置 child)。
    pub fn new(reader: R, writer: W) -> Self {
        Self { reader: BufReader::new(reader), writer, next_id: 1, child: None }
    }

    async fn send(&mut self, msg: &Value) -> Result<()> {
        let line = serde_json::to_string(msg)?;
        self.writer.write_all(line.as_bytes()).await?;
        self.writer.write_all(b"\n").await?;
        self.writer.flush().await?;
        Ok(())
    }

    async fn read_message(&mut self) -> Result<Value> {
        let mut line = String::new();
        let n = self.reader.read_line(&mut line).await?;
        if n == 0 {
            anyhow::bail!("MCP server closed the connection");
        }
        Ok(serde_json::from_str(&line)?)
    }

    /// 发请求,读到匹配 id 的响应(跳过通知/不匹配 id 的行)。
    async fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})).await?;
        loop {
            let msg = self.read_message().await?;
            if msg.get("id").and_then(|v| v.as_u64()) == Some(id) {
                if let Some(err) = msg.get("error") {
                    anyhow::bail!("MCP error for {method}: {err}");
                }
                return Ok(msg.get("result").cloned().unwrap_or(Value::Null));
            }
            // 否则是通知/日志,继续读
        }
    }

    async fn notify(&mut self, method: &str, params: Value) -> Result<()> {
        self.send(&json!({"jsonrpc": "2.0", "method": method, "params": params})).await
    }

    /// 握手:initialize 请求 + initialized 通知。
    pub async fn initialize(&mut self) -> Result<()> {
        self.request(
            "initialize",
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "argus", "version": env!("CARGO_PKG_VERSION")}
            }),
        )
        .await?;
        self.notify("notifications/initialized", json!({})).await?;
        Ok(())
    }

    /// 列出 server 的工具。
    pub async fn list_tools(&mut self) -> Result<Vec<McpToolDef>> {
        let result = self.request("tools/list", json!({})).await?;
        let tools = result.get("tools").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        Ok(tools
            .into_iter()
            .map(|t| McpToolDef {
                name: t.get("name").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                description: t.get("description").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                input_schema: t.get("inputSchema").cloned().unwrap_or_else(|| json!({"type": "object"})),
            })
            .collect())
    }

    /// 调用一个工具,返回拼接的文本内容。
    pub async fn call_tool(&mut self, name: &str, arguments: &Value) -> Result<String> {
        let result = self.request("tools/call", json!({"name": name, "arguments": arguments})).await?;
        let content = result.get("content").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        let mut out = String::new();
        for block in content {
            if let Some(t) = block.get("text").and_then(|v| v.as_str()) {
                out.push_str(t);
            }
        }
        Ok(out)
    }
}

impl McpClient<ChildStdout, ChildStdin> {
    /// spawn 一个 MCP server 子进程(stdio transport)并完成握手。
    pub async fn spawn(command: &str, args: &[String]) -> Result<Self> {
        let mut child = Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| anyhow::anyhow!("failed to spawn MCP server '{command}': {e}"))?;
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let mut client = McpClient {
            reader: BufReader::new(stdout),
            writer: stdin,
            next_id: 1,
            child: Some(child),
        };
        client.initialize().await?;
        Ok(client)
    }
}

impl<R, W> Drop for McpClient<R, W> {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.start_kill();
        }
    }
}

/// 把一个 MCP server 的工具包装成 Argus `Tool`,execute 转发到 tools/call。
pub struct McpTool<R, W> {
    client: Arc<Mutex<McpClient<R, W>>>,
    name: String,
    description: String,
    input_schema: Value,
}

#[async_trait]
impl<R: AsyncRead + Unpin + Send, W: AsyncWrite + Unpin + Send> Tool for McpTool<R, W> {
    fn name(&self) -> &str {
        &self.name
    }
    fn description(&self) -> &str {
        &self.description
    }
    fn input_schema(&self) -> Value {
        self.input_schema.clone()
    }
    async fn execute(&self, input: &Value) -> anyhow::Result<String> {
        self.client.lock().await.call_tool(&self.name, input).await
    }
    /// 外部 MCP 工具默认需审批(可能有副作用)。
    fn requires_approval(&self) -> bool {
        true
    }
}

/// spawn 一个 MCP server 并把它的全部工具包装为 `Box<dyn Tool>`。
/// 返回的工具共享同一连接(Arc<Mutex>);连接随最后一个工具 drop 而关闭(子进程被杀)。
pub async fn mcp_connect(command: &str, args: &[String]) -> Result<Vec<Box<dyn Tool>>> {
    let mut client = McpClient::spawn(command, args).await?;
    let defs = client.list_tools().await?;
    let shared = Arc::new(Mutex::new(client));
    let tools: Vec<Box<dyn Tool>> = defs
        .into_iter()
        .map(|d| {
            Box::new(McpTool {
                client: shared.clone(),
                name: d.name,
                description: d.description,
                input_schema: d.input_schema,
            }) as Box<dyn Tool>
        })
        .collect();
    Ok(tools)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncWriteExt, BufReader};

    // 用内存 duplex 扮演一个 MCP server,验证 client 的协议编解码。
    #[tokio::test]
    async fn initialize_list_and_call_over_duplex() {
        let (client_end, server_end) = tokio::io::duplex(8192);
        let (cr, cw) = tokio::io::split(client_end);
        let mut client = McpClient::new(cr, cw);

        let server = tokio::spawn(async move {
            let (sr, mut sw) = tokio::io::split(server_end);
            let mut reader = BufReader::new(sr);
            let mut line = String::new();

            // initialize 请求 → 回 result
            reader.read_line(&mut line).await.unwrap();
            let req: Value = serde_json::from_str(&line).unwrap();
            let id = req["id"].clone();
            let resp = json!({"jsonrpc":"2.0","id":id,"result":{"protocolVersion":"2024-11-05","capabilities":{},"serverInfo":{"name":"mock","version":"1"}}});
            sw.write_all(format!("{resp}\n").as_bytes()).await.unwrap();
            sw.flush().await.unwrap();

            // initialized 通知(无 id,无响应)
            line.clear();
            reader.read_line(&mut line).await.unwrap();

            // tools/list 请求 → 回一个 echo 工具
            line.clear();
            reader.read_line(&mut line).await.unwrap();
            let req: Value = serde_json::from_str(&line).unwrap();
            let id = req["id"].clone();
            let resp = json!({"jsonrpc":"2.0","id":id,"result":{"tools":[{"name":"echo","description":"echoes","inputSchema":{"type":"object"}}]}});
            sw.write_all(format!("{resp}\n").as_bytes()).await.unwrap();
            sw.flush().await.unwrap();

            // tools/call 请求 → 回 content
            line.clear();
            reader.read_line(&mut line).await.unwrap();
            let req: Value = serde_json::from_str(&line).unwrap();
            let id = req["id"].clone();
            let msg = req["params"]["arguments"]["msg"].as_str().unwrap_or("").to_string();
            let resp = json!({"jsonrpc":"2.0","id":id,"result":{"content":[{"type":"text","text":format!("echoed: {msg}")}]}});
            sw.write_all(format!("{resp}\n").as_bytes()).await.unwrap();
            sw.flush().await.unwrap();
        });

        client.initialize().await.unwrap();
        let tools = client.list_tools().await.unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "echo");
        let out = client.call_tool("echo", &json!({"msg": "hi"})).await.unwrap();
        assert_eq!(out, "echoed: hi");
        server.await.unwrap();
    }

    #[tokio::test]
    async fn request_skips_unrelated_notifications() {
        let (client_end, server_end) = tokio::io::duplex(8192);
        let (cr, cw) = tokio::io::split(client_end);
        let mut client = McpClient::new(cr, cw);

        let server = tokio::spawn(async move {
            let (sr, mut sw) = tokio::io::split(server_end);
            let mut reader = BufReader::new(sr);
            let mut line = String::new();
            reader.read_line(&mut line).await.unwrap();
            let req: Value = serde_json::from_str(&line).unwrap();
            let id = req["id"].clone();
            // 先发一条无关通知,再发真正的响应
            sw.write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"log\",\"params\":{}}\n").await.unwrap();
            let resp = json!({"jsonrpc":"2.0","id":id,"result":{"tools":[]}});
            sw.write_all(format!("{resp}\n").as_bytes()).await.unwrap();
            sw.flush().await.unwrap();
        });

        let tools = client.list_tools().await.unwrap();
        assert!(tools.is_empty());
        server.await.unwrap();
    }
}
