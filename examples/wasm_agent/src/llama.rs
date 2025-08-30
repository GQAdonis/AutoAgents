use crate::console_log;
use candle_core::quantized::gguf_file;
use candle_core::{DType, Device, Tensor};
use candle_transformers::generation::LogitsProcessor;
use candle_transformers::models::quantized_llama::ModelWeights as QLlamaModel;
use js_sys::Date;
use serde::Deserialize;
use tokenizers::Tokenizer;
use wasm_bindgen::prelude::*;

enum SelectedModel {
    Quantized(QLlamaModel),
}

#[wasm_bindgen]
pub struct Model {
    model: SelectedModel,
    tokenizer: Tokenizer,
    logits_processor: LogitsProcessor,
    tokens: Vec<u32>,
    repeat_penalty: f32,
    repeat_last_n: usize,
    previous_text_length: usize,
    stop_tokens: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct ModelName {
    pub _name_or_path: Option<String>,
    pub model_type: Option<String>,
    pub architectures: Option<Vec<String>>,
}

impl Default for ModelName {
    fn default() -> Self {
        Self {
            _name_or_path: Some("TinyLlama".to_string()),
            model_type: Some("llama".to_string()),
            architectures: Some(vec!["LlamaForCausalLM".to_string()]),
        }
    }
}

#[wasm_bindgen]
impl Model {
    #[wasm_bindgen(constructor)]
    pub fn load(
        weights: Vec<u8>,
        _tokenizer: Vec<u8>, // Unused - we use embedded tokenizer
        _config: Vec<u8>,    // Unused - we skip config parsing
        quantized: bool,
    ) -> Result<Model, JsError> {
        console_error_panic_hook::set_once();
        console_log!("loading TinyLlama model");
        let device = Device::Cpu;
        // Simply assume it's a TinyLlama model for now - no complex config parsing
        console_log!("Skipping config parsing to avoid interference with tokenizer");

        // Use the embedded tokenizer file instead of downloading
        console_log!("Using embedded tokenizer from models folder...");
        let embedded_tokenizer = include_bytes!("../models/tokenizer.json");
        console_log!("Embedded tokenizer length: {}", embedded_tokenizer.len());

        let tokenizer = match Tokenizer::from_bytes(embedded_tokenizer) {
            Ok(t) => {
                console_log!("Embedded tokenizer loaded successfully");
                t
            }
            Err(e) => {
                console_log!(
                    "Embedded tokenizer failed: {}, trying downloaded tokenizer...",
                    e
                );
                // Fall back to the downloaded tokenizer parameter
                Tokenizer::from_bytes(&_tokenizer)
                    .map_err(|m| JsError::new(&format!("Both embedded and downloaded tokenizer failed. Embedded: {}, Downloaded: {}", e, m)))?
            }
        };
        let start = Date::now();
        console_log!("weights len: {:?}", weights.len());

        if !quantized {
            return Err(JsError::new(
                "Only quantized TinyLlama models are supported",
            ));
        }

        console_log!("Loading quantized TinyLlama model from GGUF");
        // Parse GGUF content for quantized models
        let mut reader = std::io::Cursor::new(&weights);
        let content = gguf_file::Content::read(&mut reader)
            .map_err(|e| JsError::new(&format!("Failed to read GGUF content: {}", e)))?;

        // Load quantized llama model - compatible with TinyLlama
        let model = QLlamaModel::from_gguf(content, &mut reader, &device)
            .map_err(|e| JsError::new(&format!("Failed to load quantized TinyLlama: {}", e)))?;
        console_log!("Quantized TinyLlama model loaded successfully");
        let selected_model = SelectedModel::Quantized(model);

        console_log!("model loaded in {:?}s", (Date::now() - start) / 1000.);
        let logits_processor = LogitsProcessor::new(299792458, None, None);

        // Define stop tokens for TinyLlama
        let stop_tokens = vec![
            "</s>".to_string(),
            "<|endoftext|>".to_string(),
            "<|user|>".to_string(),
            "<|system|>".to_string(),
            "<|assistant|>".to_string(),
            "[INST]".to_string(),
            "[/INST]".to_string(),
            "Human:".to_string(),
            "Assistant:".to_string(),
        ];

        Ok(Self {
            model: selected_model,
            tokenizer,
            tokens: vec![],
            logits_processor,
            repeat_penalty: 1.,
            repeat_last_n: 64,
            previous_text_length: 0,
            stop_tokens,
        })
    }

    #[wasm_bindgen]
    pub fn init_with_prompt(
        &mut self,
        prompt: String,
        temp: f64,
        top_p: f64,
        repeat_penalty: f32,
        repeat_last_n: usize,
        seed: u64,
    ) -> Result<String, JsError> {
        // Clear cache - not implemented for quantized models yet
        match &mut self.model {
            SelectedModel::Quantized(_) => {} // Cache clearing not available
        };

        let temp = if temp <= 0. { None } else { Some(temp) };
        let top_p = if top_p <= 0. || top_p >= 1. {
            None
        } else {
            Some(top_p)
        };
        self.logits_processor = LogitsProcessor::new(seed, temp, top_p);
        self.repeat_penalty = repeat_penalty;
        self.repeat_last_n = repeat_last_n;
        self.tokens.clear();

        // Set previous_text_length to the prompt length so we only decode generated text
        let prompt_tokens = self
            .tokenizer
            .encode(prompt.clone(), true)
            .map_err(|m| JsError::new(&m.to_string()))?
            .get_ids()
            .to_vec();

        // Decode the prompt to get its length for proper offset
        let prompt_text = self
            .tokenizer
            .decode(&prompt_tokens, true)
            .unwrap_or(prompt.clone());
        self.previous_text_length = prompt_text.len();

        console_log!(
            "Prompt has {} tokens, text length: {}",
            prompt_tokens.len(),
            self.previous_text_length
        );

        let text = self
            .process(&prompt_tokens)
            .map_err(|m| JsError::new(&m.to_string()))?;
        Ok(text)
    }

    #[wasm_bindgen]
    pub fn next_token(&mut self) -> Result<String, JsError> {
        let last_token = *self.tokens.last().unwrap();
        let text = self
            .process(&[last_token])
            .map_err(|m| JsError::new(&m.to_string()))?;
        Ok(text)
    }
}

impl Model {
    fn process(&mut self, tokens: &[u32]) -> candle_core::Result<String> {
        console_log!(
            "Processing {} tokens, existing tokens: {}",
            tokens.len(),
            self.tokens.len()
        );

        let dev = Device::Cpu;
        let input = Tensor::new(tokens, &dev)?.unsqueeze(0)?;
        let logits = match &mut self.model {
            SelectedModel::Quantized(m) => m.forward(&input, self.tokens.len())?,
        };
        let logits = logits.squeeze(0)?.to_dtype(DType::F32)?;

        // For the initial call, add all prompt tokens to history
        if self.tokens.is_empty() {
            for &token in tokens {
                self.tokens.push(token);
            }
        }

        // Apply repeat penalty considering all tokens processed so far
        let logits = if self.repeat_penalty == 1. {
            logits
        } else {
            let start_at = self.tokens.len().saturating_sub(self.repeat_last_n);
            candle_transformers::utils::apply_repeat_penalty(
                &logits,
                self.repeat_penalty,
                &self.tokens[start_at..],
            )?
        };

        let next_token = self.logits_processor.sample(&logits)?;
        console_log!("Sampled next token: {}", next_token);
        self.tokens.push(next_token);

        // Decode the entire sequence to get proper spacing, then extract the last token
        let full_text = self
            .tokenizer
            .decode(&self.tokens, true)
            .unwrap_or_else(|e| {
                console_log!("error decoding full sequence: {:?}", e);
                "".to_string()
            });

        // Check if the full text contains any stop tokens
        for stop_token in &self.stop_tokens {
            if full_text.contains(stop_token) {
                console_log!("Stop token detected: {}", stop_token);
                // Return the text up to the stop token
                if let Some(stop_pos) = full_text.find(stop_token) {
                    let clean_text = &full_text[..stop_pos];
                    let token = if clean_text.len() > self.previous_text_length {
                        let new_text = &clean_text[self.previous_text_length..];
                        self.previous_text_length = clean_text.len();
                        new_text.to_string()
                    } else {
                        String::new()
                    };
                    console_log!("Final token before stop: '{}'", token);
                    return Ok(token);
                }
            }
        }

        // For streaming, we need to return only the new part
        let current_length = full_text.len();
        let token = if current_length > self.previous_text_length {
            let new_text = &full_text[self.previous_text_length..];
            self.previous_text_length = current_length;
            new_text.to_string()
        } else {
            String::new()
        };

        console_log!("Decoded token: '{}'", token);
        Ok(token)
    }
}
