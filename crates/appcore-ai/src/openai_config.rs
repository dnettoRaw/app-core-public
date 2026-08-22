// =============================================================================
//        #######
//     ###       ###     F: openai_config.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::{
    AiError, AiModality, AiResult, AiTask, ArtifactFormat, BackendCostHints, BackendDevice,
    BackendId, ModelId,
};
use appcore_transport::HttpTarget;
use std::collections::BTreeMap;
use std::time::Duration;

/// Known server family using an OpenAI-compatible chat-completions boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpenAiCompatibleEngine {
    /// `llama-server` from llama.cpp.
    LlamaCpp,
    /// MLX-LM's OpenAI-compatible server.
    MlxLm,
    /// vLLM's OpenAI-compatible server.
    Vllm,
    /// SGLang's OpenAI-compatible server.
    Sglang,
    /// TensorRT-LLM's OpenAI-compatible server.
    TensorRtLlm,
    /// OpenVINO Model Server's OpenAI-compatible endpoint.
    OpenVino,
    /// TabbyAPI or another ExLlama-family server.
    TabbyApi,
    /// An explicitly tested compatible implementation.
    Generic,
}

impl OpenAiCompatibleEngine {
    fn formats(self) -> Vec<ArtifactFormat> {
        match self {
            Self::LlamaCpp => vec![ArtifactFormat::Gguf],
            Self::OpenVino => vec![ArtifactFormat::Onnx],
            Self::MlxLm | Self::Vllm | Self::Sglang | Self::TensorRtLlm | Self::TabbyApi => {
                vec![ArtifactFormat::SafeTensors]
            }
            Self::Generic => vec![
                ArtifactFormat::Gguf,
                ArtifactFormat::Onnx,
                ArtifactFormat::SafeTensors,
            ],
        }
    }
}

/// Options that one exact server deployment is known to honor.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OpenAiGenerationCapabilities {
    /// Tool definitions and tool calls are supported.
    pub tools: bool,
    /// `image/*` data URLs are supported by the selected model/server.
    pub vision: bool,
    /// Deterministic seed is supported.
    pub seed: bool,
    /// Stop sequences are supported.
    pub stop_sequences: bool,
}

/// Explicit bounded configuration for one OpenAI-compatible backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenAiCompatibleConfig {
    /// Backend identity referenced by model descriptors.
    pub backend_id: BackendId,
    /// Server family used only for diagnostics and safe format defaults.
    pub engine: OpenAiCompatibleEngine,
    /// Base URL. `Debug` for the runtime backend never exposes it.
    pub base_url: String,
    /// Relative chat-completions path.
    pub request_path: String,
    /// Whether a non-loopback host was intentionally allowed.
    pub allow_non_loopback: bool,
    /// Backend-owned devices advertised to placement.
    pub devices: Vec<BackendDevice>,
    /// Exact AppCore model ID to server model-name bindings.
    pub model_names: BTreeMap<ModelId, String>,
    /// Options verified for this deployment.
    pub capabilities: OpenAiGenerationCapabilities,
    /// Bounded transport timeout.
    pub timeout: Duration,
    /// Maximum encoded JSON request bytes.
    pub max_request_bytes: usize,
    /// Maximum decoded JSON response bytes.
    pub max_response_bytes: usize,
    /// Relative cold-load cost used by placement.
    pub load_cost_units: u64,
    /// Relative inference cost used by placement.
    pub inference_cost_units: u64,
}

impl OpenAiCompatibleConfig {
    /// Creates a conservative local-server configuration.
    pub fn local(
        engine: OpenAiCompatibleEngine,
        backend_id: BackendId,
        base_url: impl Into<String>,
        devices: Vec<BackendDevice>,
        model_names: BTreeMap<ModelId, String>,
    ) -> AiResult<Self> {
        Self::new(engine, backend_id, base_url, false, devices, model_names)
    }

    /// Creates an explicitly remote-server configuration.
    ///
    /// This only permits a non-loopback endpoint. Authentication remains the
    /// responsibility of a caller-provided [`crate::OpenAiHttpTransport`]
    /// backed by AppCore secret references and policy.
    pub fn remote(
        engine: OpenAiCompatibleEngine,
        backend_id: BackendId,
        base_url: impl Into<String>,
        devices: Vec<BackendDevice>,
        model_names: BTreeMap<ModelId, String>,
    ) -> AiResult<Self> {
        Self::new(engine, backend_id, base_url, true, devices, model_names)
    }

    fn new(
        engine: OpenAiCompatibleEngine,
        backend_id: BackendId,
        base_url: impl Into<String>,
        allow_non_loopback: bool,
        devices: Vec<BackendDevice>,
        model_names: BTreeMap<ModelId, String>,
    ) -> AiResult<Self> {
        let config = Self {
            backend_id,
            engine,
            base_url: base_url.into(),
            request_path: "/v1/chat/completions".to_string(),
            allow_non_loopback,
            devices,
            model_names,
            capabilities: OpenAiGenerationCapabilities::default(),
            timeout: Duration::from_secs(30),
            max_request_bytes: 4 * 1_024 * 1_024,
            max_response_bytes: 4 * 1_024 * 1_024,
            load_cost_units: 10,
            inference_cost_units: 10,
        };
        config.validate()?;
        Ok(config)
    }

    /// Validates endpoint policy, declarations and every bounded server model name.
    pub fn validate(&self) -> AiResult<()> {
        if self.base_url.len() > 2_048
            || self.request_path.is_empty()
            || self.request_path.len() > 256
            || !self.request_path.starts_with('/')
            || self.timeout.is_zero()
            || self.timeout > Duration::from_secs(600)
            || self.max_request_bytes == 0
            || self.max_response_bytes == 0
            || self.max_request_bytes > 64 * 1_024 * 1_024
            || self.max_response_bytes > 64 * 1_024 * 1_024
            || self.model_names.is_empty()
            || self.model_names.len() > 128
        {
            return Err(AiError::InvalidInput("OpenAI-compatible config"));
        }
        let target = HttpTarget::parse(&self.base_url, &self.request_path)
            .map_err(|_| AiError::InvalidInput("OpenAI-compatible endpoint"))?;
        if !self.allow_non_loopback && !is_loopback_host(target.host()) {
            return Err(AiError::Unauthorized);
        }
        if self.devices.is_empty() || self.devices.len() > 32 {
            return Err(AiError::InvalidInput("OpenAI-compatible devices"));
        }
        for name in self.model_names.values() {
            if name.is_empty()
                || name.len() > 256
                || name.chars().any(char::is_control)
                || name.contains("..")
            {
                return Err(AiError::InvalidInput("server model name"));
            }
        }
        Ok(())
    }

    pub(crate) fn tasks(&self) -> Vec<AiTask> {
        let mut tasks = vec![
            AiTask::GenerateText,
            AiTask::Chat,
            AiTask::TransformText,
            AiTask::Extract,
            AiTask::Decide,
        ];
        if self.capabilities.vision {
            tasks.push(AiTask::AnalyzeImage);
        }
        tasks
    }

    pub(crate) fn modalities(&self) -> Vec<AiModality> {
        let mut modalities = vec![AiModality::Text];
        if self.capabilities.vision {
            modalities.push(AiModality::Image);
        }
        modalities
    }

    pub(crate) fn formats(&self) -> Vec<ArtifactFormat> {
        self.engine.formats()
    }

    pub(crate) fn costs(&self) -> BackendCostHints {
        BackendCostHints {
            load_units: self.load_cost_units,
            inference_units: self.inference_cost_units,
            supports_batching: false,
        }
    }
}

fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host == "::1"
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}
