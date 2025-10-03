#![allow(dead_code)]
use crate::agent::task::Task;
use crate::agent::{
    AgentDeriveT, AgentExecutor, AgentHooks, AgentOutputT, Context, ExecutorConfig,
};
use crate::tool::ToolT;
use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum TestError {
    #[error("Test error: {0}")]
    TestError(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestAgentOutput {
    pub result: String,
}

impl AgentOutputT for TestAgentOutput {
    fn output_schema() -> &'static str {
        r#"{"type":"object","properties":{"result":{"type":"string"}},"required":["result"]}"#
    }

    fn structured_output_format() -> Value {
        serde_json::json!({
            "name": "TestAgentOutput",
            "description": "Test agent output schema",
            "schema": {
                "type": "object",
                "properties": {
                    "result": {"type": "string"}
                },
                "required": ["result"]
            },
            "strict": true
        })
    }
}

impl From<TestAgentOutput> for Value {
    fn from(output: TestAgentOutput) -> Self {
        serde_json::to_value(output).unwrap_or(Value::Null)
    }
}

#[derive(Debug)]
pub struct MockAgentImpl {
    pub name: String,
    pub description: String,
    pub should_fail: bool,
}

impl MockAgentImpl {
    pub fn new(name: &str, description: &str) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            should_fail: false,
        }
    }

    pub fn with_failure(mut self, should_fail: bool) -> Self {
        self.should_fail = should_fail;
        self
    }
}

#[async_trait]
impl AgentDeriveT for MockAgentImpl {
    type Output = TestAgentOutput;

    fn description(&self) -> &'static str {
        Box::leak(self.description.clone().into_boxed_str())
    }

    fn output_schema(&self) -> Option<Value> {
        Some(TestAgentOutput::structured_output_format())
    }

    fn name(&self) -> &'static str {
        Box::leak(self.name.clone().into_boxed_str())
    }

    fn tools(&self) -> Vec<Box<dyn ToolT>> {
        vec![]
    }
}

#[async_trait]
impl AgentExecutor for MockAgentImpl {
    type Output = TestAgentOutput;
    type Error = TestError;

    fn config(&self) -> ExecutorConfig {
        ExecutorConfig::default()
    }

    async fn execute(
        &self,
        task: &Task,
        _context: Arc<Context>,
    ) -> Result<Self::Output, Self::Error> {
        if self.should_fail {
            return Err(TestError::TestError("Mock execution failed".to_string()));
        }

        Ok(TestAgentOutput {
            result: format!("Processed: {}", task.prompt),
        })
    }
    async fn execute_stream(
        &self,
        _task: &Task,
        _context: Arc<Context>,
    ) -> Result<
        std::pin::Pin<Box<dyn Stream<Item = Result<Self::Output, Self::Error>> + Send>>,
        Self::Error,
    > {
        unimplemented!()
    }
}

impl AgentHooks for MockAgentImpl {}

// Test tool for agents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestToolArgs {
    pub input: String,
}

#[derive(Debug)]
pub struct MockTool {
    pub name: String,
    pub description: String,
}

impl MockTool {
    pub fn new(name: &str, description: &str) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
        }
    }
}
