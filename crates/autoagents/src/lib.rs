// Re-export for convenience
pub use async_trait::async_trait;
pub use autoagents_core::{self as core, error as core_error};
pub use autoagents_llm::{self as llm, error as llm_error};

#[inline]
/// Initialize logging using env_logger if the "logging" feature is enabled.
/// This is a no-op if the feature is not enabled.
pub fn init_logging() {
    #[cfg(feature = "logging")]
    {
        let _ = env_logger::try_init();
    }
}
