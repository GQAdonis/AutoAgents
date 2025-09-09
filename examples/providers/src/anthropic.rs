use autoagents::core::agent::memory::SlidingWindowMemory;
use autoagents::core::agent::prebuilt::executor::BasicAgent;
use autoagents::core::agent::task::Task;
use autoagents::core::agent::{AgentBuilder, DirectAgent};
use autoagents::core::error::Error;
use autoagents::core::tool::ToolT;
use autoagents::llm::backends::anthropic::Anthropic;
use autoagents::llm::builder::LLMBuilder;
use autoagents_derive::{agent, AgentHooks};
use serde_json::Value;
use std::sync::Arc;

#[agent(
    name = "math_agent",
    description = "You are a Math agent",
    tools = [],
)]
#[derive(Default, Clone, AgentHooks)]
pub struct MathAgent {}

pub async fn run() -> Result<(), Error> {
    let api_key = std::env::var("ANTHROPIC_API_KEY").unwrap_or("".into());

    // Initialize and configure the LLM client
    let llm: Arc<Anthropic> = LLMBuilder::<Anthropic>::new()
        .api_key(api_key) // Set the API key
        .model("claude-sonnet-4-20250514") // Use Claude 4 Sonnet
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
