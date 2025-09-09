use autoagents::core::agent::memory::SlidingWindowMemory;
use autoagents::core::agent::prebuilt::executor::{BasicAgent, BasicAgentOutput};
use autoagents::core::agent::task::Task;
use autoagents::core::agent::{AgentBuilder, AgentOutputT, DirectAgent};
use autoagents::core::error::Error;
use autoagents::core::tool::ToolT;
use autoagents::llm::backends::groq::Groq;
use autoagents::llm::builder::LLMBuilder;
use autoagents_derive::{agent, AgentHooks, AgentOutput};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize, AgentOutput)]
struct MathAgentOutput {
    #[output(description = "The addition result")]
    value: i64,
    #[output(description = "Explanation of the logic")]
    explanation: String,
    #[output(description = "If user asks other than math questions, use this to answer them.")]
    generic: Option<String>,
}

impl From<BasicAgentOutput> for MathAgentOutput {
    fn from(output: BasicAgentOutput) -> Self {
        let resp = output.response;
        if output.done && !resp.trim().is_empty() {
            // Try to parse as structured JSON first
            if let Ok(value) = serde_json::from_str::<MathAgentOutput>(&resp) {
                return value;
            }
        }
        // For streaming chunks or unparseable content, create a default response
        MathAgentOutput {
            value: 0,
            explanation: resp,
            generic: None,
        }
    }
}

#[agent(
    name = "math_agent",
    description = "You are a Math agent",
    output = MathAgentOutput,
)]
#[derive(Default, Clone, AgentHooks)]
struct MathAgent {}

pub async fn run() -> Result<(), Error> {
    let api_key = std::env::var("GROQ_API_KEY").unwrap_or("".into());

    // Initialize and configure the LLM client
    let llm: Arc<Groq> = LLMBuilder::<Groq>::new()
        .api_key(api_key) // Set the API key
        .model("openai/gpt-oss-20b") // Use Llama openai/gpt-oss-20b with structured output support
        .max_tokens(512) // Limit response length
        .temperature(0.2) // Control response randomness (0.0-1.0)
        .build()
        .expect("Failed to build LLM");

    let sliding_window_memory = Box::new(SlidingWindowMemory::new(10));

    let agent = BasicAgent::new(MathAgent {});
    let agent_handle = AgentBuilder::<_, DirectAgent>::new(agent)
        .llm(llm)
        .memory(sliding_window_memory)
        .build()
        .await?;

    let result = agent_handle
        .agent
        .run(Task::new("What is 20 + 10?"))
        .await?;
    println!("Result: {:?}", result);
    Ok(())
}
