// =============================================================================
//        #######
//     ###       ###     F: openai_compatible.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

#![cfg(feature = "backend-openai-compatible")]

use appcore_ai::{
    AiError, AiLimits, AiMessage, AiMessageRole, AiOutput, AiRequest, AiResult, AiStreamEvent,
    AiStreamSink, AiStructuredOutput, AiStructuredOutputFallback, AiTask, AiToolChoice,
    AiToolDefinition, ArtifactDigest, ArtifactFormat, ArtifactIdentity, BackendDevice, BackendId,
    CancellationToken, DeviceId, DeviceKind, InferenceBackend, ModelDescriptor, ModelId,
    OpenAiCompatibilityProfile, OpenAiCompatibleBackend, OpenAiCompatibleConfig,
    OpenAiCompatibleEngine, OpenAiCompatibleTransport, OpenAiExtraParameter, OpenAiTokenLimitField,
    OpenAiTransportChunkSink, OpenAiTransportFuture, OpenAiTransportRequest,
    OpenAiTransportResponse, QualityTier, Quantization, UnauthenticatedOpenAiHttpTransport,
};
use std::collections::BTreeMap;
use std::future::Future;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

#[test]
fn loopback_backend_executes_bounded_chat_completion() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let body = read_request_body(&mut stream);
        let request: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(request["model"], "tiny-chat");
        assert_eq!(request["messages"][0]["role"], "system");
        assert_eq!(request["messages"][1]["role"], "user");
        let response = br#"{"choices":[{"message":{"role":"assistant","content":"local answer"},"finish_reason":"stop"}],"usage":{"prompt_tokens":4,"completion_tokens":2,"total_tokens":6}}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            response.len()
        )
        .unwrap();
        stream.write_all(response).unwrap();
    });

    let backend_id = BackendId::new("local/openai-compatible").unwrap();
    let model_id = ModelId::new("tiny-chat").unwrap();
    let device = DeviceId::new("local/cpu").unwrap();
    let mut model_names = BTreeMap::new();
    model_names.insert(model_id.clone(), "tiny-chat".to_string());
    let config = OpenAiCompatibleConfig::local(
        OpenAiCompatibleEngine::LlamaCpp,
        backend_id.clone(),
        format!("http://{address}"),
        vec![BackendDevice {
            id: device.clone(),
            kind: DeviceKind::Cpu,
        }],
        model_names,
    )
    .unwrap();
    let backend = OpenAiCompatibleBackend::new(
        config,
        Arc::new(UnauthenticatedOpenAiHttpTransport::default()),
    )
    .unwrap();
    let model = descriptor(model_id, backend_id);
    let request = AiRequest::chat(
        [
            AiMessage::new(AiMessageRole::System, "Be concise").unwrap(),
            AiMessage::new(AiMessageRole::User, "Answer locally").unwrap(),
        ],
        AiLimits::default(),
    )
    .unwrap();
    let response =
        block_on(backend.infer(&request, &model, &device, &CancellationToken::new())).unwrap();
    assert_eq!(response.output, AiOutput::Text("local answer".to_string()));
    assert_eq!(response.metadata.len(), 4);
    server.join().unwrap();
}

#[test]
fn local_config_rejects_non_loopback_endpoint() {
    let mut names = BTreeMap::new();
    names.insert(ModelId::new("model").unwrap(), "model".to_string());
    let result = OpenAiCompatibleConfig::local(
        OpenAiCompatibleEngine::Generic,
        BackendId::new("remote").unwrap(),
        "https://models.example.test",
        vec![BackendDevice {
            id: DeviceId::new("remote/gpu").unwrap(),
            kind: DeviceKind::Gpu,
        }],
        names,
    );
    assert!(matches!(result, Err(appcore_ai::AiError::Unauthorized)));
}

#[test]
fn remote_config_requires_an_explicit_remote_constructor() {
    let mut names = BTreeMap::new();
    names.insert(ModelId::new("model").unwrap(), "model".to_string());
    let config = OpenAiCompatibleConfig::remote(
        OpenAiCompatibleEngine::Generic,
        BackendId::new("remote").unwrap(),
        "https://models.example.test",
        vec![BackendDevice {
            id: DeviceId::new("remote/gpu").unwrap(),
            kind: DeviceKind::Gpu,
        }],
        names,
    )
    .unwrap();
    assert!(config.allow_non_loopback);
}

#[test]
fn preserves_http_status_and_retry_hint() {
    let (parts, model, device, _) =
        backend_with_transport(StaticTransport::response(OpenAiTransportResponse {
            status_code: 429,
            retry_after: Some(std::time::Duration::from_secs(12)),
            body: Vec::new(),
        }));
    let backend = OpenAiCompatibleBackend::new(parts.config, parts.transport).unwrap();
    let request = chat_request();
    let error =
        block_on(backend.infer(&request, &model, &device, &CancellationToken::new())).unwrap_err();
    assert!(matches!(
        error,
        AiError::BackendHttp {
            status_code: 429,
            retry_after: Some(value),
            ..
        } if value == std::time::Duration::from_secs(12)
    ));
    assert!(error.is_transient());
}

#[test]
fn malformed_tool_arguments_remain_recoverable_with_metadata() {
    let body = br#"{"choices":[{"message":{"tool_calls":[{"id":"call-1","function":{"name":"record","arguments":"{\"partial\":"}}]},"finish_reason":"length"}],"usage":{"prompt_tokens":8,"completion_tokens":4,"total_tokens":12}}"#;
    let transport = StaticTransport::ok(body);
    let (mut backend, model, device, _) = backend_with_transport(transport);
    backend.config.capabilities.tools = true;
    let backend = OpenAiCompatibleBackend::new(backend.config, backend.transport).unwrap();
    let mut request = chat_request();
    request.options.generation.tools = vec![AiToolDefinition {
        name: "record".to_string(),
        description: "Record one bounded result".to_string(),
        input_schema: r#"{"type":"object"}"#.to_string(),
    }];
    request.options.generation.tool_choice = AiToolChoice::Required;
    let response =
        block_on(backend.infer(&request, &model, &device, &CancellationToken::new())).unwrap();
    let AiOutput::ToolCalls(calls) = response.output else {
        panic!("expected tool calls");
    };
    assert_eq!(calls[0].arguments_json, r#"{"partial":"#);
    assert!(response
        .metadata
        .iter()
        .any(|entry| { entry.key == "tool_calls.invalid_arguments" && entry.value == "1" }));
    assert!(response
        .metadata
        .iter()
        .any(|entry| entry.key == "finish_reason" && entry.value == "length"));
    assert!(response
        .metadata
        .iter()
        .any(|entry| entry.key == "usage.total_tokens" && entry.value == "12"));
}

#[test]
fn compatibility_profile_and_json_schema_are_encoded_explicitly() {
    let transport = StaticTransport::ok(
        br#"{"choices":[{"message":{"content":"{}"},"finish_reason":"stop"}]}"#,
    );
    let (mut parts, model, device, recording) = backend_with_transport(transport);
    parts.config.capabilities.structured_output = true;
    parts.config.compatibility = OpenAiCompatibilityProfile {
        send_temperature: false,
        send_top_p: false,
        token_limit_field: OpenAiTokenLimitField::MaxCompletionTokens,
        extra_parameters: vec![OpenAiExtraParameter {
            name: "thinking".to_string(),
            value_json: r#"{"type":"disabled"}"#.to_string(),
        }],
    };
    let backend = OpenAiCompatibleBackend::new(parts.config, parts.transport).unwrap();
    let mut request = chat_request();
    request.options.generation.structured_output = Some(AiStructuredOutput {
        name: "answer".to_string(),
        schema: r#"{"type":"object","properties":{"ok":{"type":"boolean"}},"required":["ok"]}"#
            .to_string(),
        strict: true,
        fallback: AiStructuredOutputFallback::Reject,
    });
    block_on(backend.infer(&request, &model, &device, &CancellationToken::new())).unwrap();
    let body = recording.body.lock().unwrap().clone().unwrap();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(value.get("temperature").is_none());
    assert!(value.get("top_p").is_none());
    assert_eq!(value["max_completion_tokens"], 512);
    assert_eq!(value["thinking"]["type"], "disabled");
    assert_eq!(value["response_format"]["type"], "json_schema");
    assert_eq!(value["response_format"]["json_schema"]["strict"], true);
}

#[test]
fn compatibility_profile_rejects_reserved_overrides() {
    let mut names = BTreeMap::new();
    names.insert(ModelId::new("model").unwrap(), "model".to_string());
    let mut config = OpenAiCompatibleConfig::local(
        OpenAiCompatibleEngine::Generic,
        BackendId::new("local/profile").unwrap(),
        "http://127.0.0.1:8080",
        vec![BackendDevice {
            id: DeviceId::new("local/cpu").unwrap(),
            kind: DeviceKind::Cpu,
        }],
        names,
    )
    .unwrap();
    config.compatibility.extra_parameters = vec![OpenAiExtraParameter {
        name: "messages".to_string(),
        value_json: "[]".to_string(),
    }];
    assert!(matches!(
        config.validate(),
        Err(AiError::InvalidInput("OpenAI compatibility parameter"))
    ));
}

#[test]
fn compatibility_profile_rejects_duplicate_parameters() {
    let mut names = BTreeMap::new();
    names.insert(ModelId::new("model").unwrap(), "model".to_string());
    let mut config = OpenAiCompatibleConfig::local(
        OpenAiCompatibleEngine::Generic,
        BackendId::new("local/profile").unwrap(),
        "http://127.0.0.1:8080",
        vec![BackendDevice {
            id: DeviceId::new("local/cpu").unwrap(),
            kind: DeviceKind::Cpu,
        }],
        names,
    )
    .unwrap();
    config.compatibility.extra_parameters = vec![
        OpenAiExtraParameter {
            name: "thinking".to_string(),
            value_json: "false".to_string(),
        },
        OpenAiExtraParameter {
            name: "thinking".to_string(),
            value_json: "true".to_string(),
        },
    ];
    assert!(matches!(
        config.validate(),
        Err(AiError::InvalidInput("OpenAI compatibility parameter"))
    ));
}

#[test]
fn json_text_fallback_is_explicit_and_schema_bounded() {
    let transport = StaticTransport::ok(
        br#"{"choices":[{"message":{"content":"{\"ok\":true}"},"finish_reason":"stop"}]}"#,
    );
    let (parts, model, device, recording) = backend_with_transport(transport);
    let backend = OpenAiCompatibleBackend::new(parts.config, parts.transport).unwrap();
    let mut request = chat_request();
    request.options.generation.structured_output = Some(AiStructuredOutput {
        name: "fallback".to_string(),
        schema: r#"{"type":"object"}"#.to_string(),
        strict: true,
        fallback: AiStructuredOutputFallback::JsonText,
    });
    block_on(backend.infer(&request, &model, &device, &CancellationToken::new())).unwrap();
    let body = recording.body.lock().unwrap().clone().unwrap();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(value.get("response_format").is_none());
    assert!(value["messages"][0]["content"]
        .as_str()
        .unwrap()
        .contains("JSON Schema"));
}

#[test]
fn streaming_decodes_text_with_backpressure_and_terminal_usage() {
    let chunks = vec![
        b"data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"},\"finish_reason\":null}]}\r\n"
            .to_vec(),
        b"\r\ndata: {\"choices\":[{\"delta\":{\"content\":\"lo\"},\"finish_reason\":\"stop\"}]}\r\n\r\n"
            .to_vec(),
        b"data: {\"choices\":[],\"usage\":{\"prompt_tokens\":2,\"completion_tokens\":1,\"total_tokens\":3}}\n\ndata: [DONE]\n\n"
            .to_vec(),
    ];
    let transport = StaticTransport::stream(chunks);
    let (mut parts, model, device, _) = backend_with_transport(transport);
    parts.config.capabilities.streaming = true;
    let backend = OpenAiCompatibleBackend::new(parts.config, parts.transport).unwrap();
    let sink = RecordingStream::default();
    let response = block_on(backend.infer_stream(
        &chat_request(),
        &model,
        &device,
        &CancellationToken::new(),
        &sink,
    ))
    .unwrap();
    assert_eq!(response.output, AiOutput::Text("Hello".to_string()));
    assert_eq!(
        *sink.events.lock().unwrap(),
        vec![
            AiStreamEvent::TextDelta("Hel".to_string()),
            AiStreamEvent::TextDelta("lo".to_string())
        ]
    );
    assert!(response
        .metadata
        .iter()
        .any(|entry| entry.key == "usage.total_tokens" && entry.value == "3"));
}

#[test]
fn backend_propagates_pending_from_async_transport() {
    let (parts, model, device, _) = backend_with_transport(StaticTransport::ok(
        br#"{"choices":[{"message":{"content":"ready"},"finish_reason":"stop"}]}"#,
    ));
    let backend = OpenAiCompatibleBackend::new(
        parts.config,
        Arc::new(PendingTransport {
            response: parts.transport.response.clone(),
        }),
    )
    .unwrap();
    let request = chat_request();
    let cancellation = CancellationToken::new();
    let mut future = backend.infer(&request, &model, &device, &cancellation);
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    assert!(matches!(future.as_mut().poll(&mut context), Poll::Pending));
    assert!(matches!(
        future.as_mut().poll(&mut context),
        Poll::Ready(Ok(_))
    ));
}

#[test]
fn streaming_stops_after_cooperative_cancellation() {
    let transport = StaticTransport::stream(vec![
        b"data: {\"choices\":[{\"delta\":{\"content\":\"first\"}}]}\n\n".to_vec(),
        b"data: {\"choices\":[{\"delta\":{\"content\":\"second\"}}]}\n\n".to_vec(),
    ]);
    let (mut parts, model, device, _) = backend_with_transport(transport);
    parts.config.capabilities.streaming = true;
    let backend = OpenAiCompatibleBackend::new(parts.config, parts.transport).unwrap();
    let cancellation = CancellationToken::new();
    let sink = CancellingStream {
        cancellation: cancellation.clone(),
        events: Mutex::new(Vec::new()),
    };
    let error =
        block_on(backend.infer_stream(&chat_request(), &model, &device, &cancellation, &sink))
            .unwrap_err();
    assert_eq!(error, AiError::Cancelled);
    assert_eq!(sink.events.lock().unwrap().len(), 1);
}

#[derive(Clone)]
struct BackendParts {
    config: OpenAiCompatibleConfig,
    transport: Arc<StaticTransport>,
}

fn backend_with_transport(
    transport: StaticTransport,
) -> (
    BackendParts,
    ModelDescriptor,
    DeviceId,
    Arc<RecordingRequest>,
) {
    let backend_id = BackendId::new("local/test-openai").unwrap();
    let model_id = ModelId::new("test-chat").unwrap();
    let device = DeviceId::new("local/test-cpu").unwrap();
    let config = OpenAiCompatibleConfig::local(
        OpenAiCompatibleEngine::Generic,
        backend_id.clone(),
        "http://127.0.0.1:8080",
        vec![BackendDevice {
            id: device.clone(),
            kind: DeviceKind::Cpu,
        }],
        BTreeMap::from([(model_id.clone(), "test-chat".to_string())]),
    )
    .unwrap();
    let recording = Arc::clone(&transport.recording);
    let transport = Arc::new(transport);
    (
        BackendParts { config, transport },
        descriptor(model_id, backend_id),
        device,
        recording,
    )
}

fn chat_request() -> AiRequest {
    AiRequest::chat(
        [AiMessage::new(AiMessageRole::User, "bounded request").unwrap()],
        AiLimits::default(),
    )
    .unwrap()
}

#[derive(Default)]
struct RecordingRequest {
    body: Mutex<Option<Vec<u8>>>,
}

struct StaticTransport {
    response: OpenAiTransportResponse,
    chunks: Vec<Vec<u8>>,
    recording: Arc<RecordingRequest>,
}

impl StaticTransport {
    fn ok(body: &[u8]) -> Self {
        Self::response(OpenAiTransportResponse {
            status_code: 200,
            retry_after: None,
            body: body.to_vec(),
        })
    }

    fn response(response: OpenAiTransportResponse) -> Self {
        Self {
            response,
            chunks: Vec::new(),
            recording: Arc::new(RecordingRequest::default()),
        }
    }

    fn stream(chunks: Vec<Vec<u8>>) -> Self {
        Self {
            response: OpenAiTransportResponse {
                status_code: 200,
                retry_after: None,
                body: Vec::new(),
            },
            chunks,
            recording: Arc::new(RecordingRequest::default()),
        }
    }
}

impl OpenAiCompatibleTransport for StaticTransport {
    fn send<'a>(
        &'a self,
        request: &'a OpenAiTransportRequest,
        _cancellation: &'a CancellationToken,
    ) -> OpenAiTransportFuture<'a> {
        *self.recording.body.lock().unwrap() = Some(request.body().to_vec());
        Box::pin(async { Ok(self.response.clone()) })
    }

    fn send_stream<'a>(
        &'a self,
        request: &'a OpenAiTransportRequest,
        cancellation: &'a CancellationToken,
        sink: &'a mut dyn OpenAiTransportChunkSink,
    ) -> OpenAiTransportFuture<'a> {
        *self.recording.body.lock().unwrap() = Some(request.body().to_vec());
        Box::pin(async move {
            for chunk in &self.chunks {
                if cancellation.is_cancelled() {
                    return Err(AiError::Cancelled);
                }
                sink.chunk(chunk)?;
            }
            Ok(self.response.clone())
        })
    }
}

#[derive(Default)]
struct RecordingStream {
    events: Mutex<Vec<AiStreamEvent>>,
}

impl AiStreamSink for RecordingStream {
    fn event(&self, event: &AiStreamEvent) -> AiResult<()> {
        self.events.lock().unwrap().push(event.clone());
        Ok(())
    }
}

struct CancellingStream {
    cancellation: CancellationToken,
    events: Mutex<Vec<AiStreamEvent>>,
}

impl AiStreamSink for CancellingStream {
    fn event(&self, event: &AiStreamEvent) -> AiResult<()> {
        self.events.lock().unwrap().push(event.clone());
        self.cancellation.cancel();
        Ok(())
    }
}

struct PendingTransport {
    response: OpenAiTransportResponse,
}

impl OpenAiCompatibleTransport for PendingTransport {
    fn send<'a>(
        &'a self,
        _request: &'a OpenAiTransportRequest,
        _cancellation: &'a CancellationToken,
    ) -> OpenAiTransportFuture<'a> {
        Box::pin(PendingOnce {
            pending: true,
            response: self.response.clone(),
        })
    }
}

struct PendingOnce {
    pending: bool,
    response: OpenAiTransportResponse,
}

impl Future for PendingOnce {
    type Output = AiResult<OpenAiTransportResponse>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        if self.pending {
            self.pending = false;
            context.waker().wake_by_ref();
            Poll::Pending
        } else {
            Poll::Ready(Ok(self.response.clone()))
        }
    }
}

fn descriptor(id: ModelId, backend: BackendId) -> ModelDescriptor {
    ModelDescriptor {
        id,
        revision: "v1".to_string(),
        tasks: vec![AiTask::GenerateText, AiTask::Chat],
        input_modalities: vec![appcore_ai::AiModality::Text],
        format: ArtifactFormat::Gguf,
        quantization: Quantization::Int4,
        estimated_memory_bytes: 1_024,
        estimated_vram_bytes: 0,
        max_input_bytes: 8_192,
        max_output_bytes: 8_192,
        context_limit: Some(2_048),
        supported_backends: vec![backend],
        supported_devices: vec![DeviceKind::Cpu],
        load_cost_units: 1,
        quality: Some(QualityTier::Tiny),
        artifact: ArtifactIdentity {
            digest: ArtifactDigest::from_bytes(b"externally-managed-model"),
            size_bytes: 24,
            publisher: None,
            signature_required: false,
        },
    }
}

fn read_request_body(stream: &mut std::net::TcpStream) -> Vec<u8> {
    let mut bytes = Vec::new();
    let header_end = loop {
        let mut chunk = [0u8; 1_024];
        let read = stream.read(&mut chunk).unwrap();
        assert!(read > 0);
        bytes.extend_from_slice(&chunk[..read]);
        if let Some(index) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break index + 4;
        }
    };
    let headers = std::str::from_utf8(&bytes[..header_end]).unwrap();
    let content_length = headers
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length: ")
                .and_then(|value| value.parse::<usize>().ok())
        })
        .unwrap();
    while bytes.len() - header_end < content_length {
        let mut chunk = [0u8; 1_024];
        let read = stream.read(&mut chunk).unwrap();
        assert!(read > 0);
        bytes.extend_from_slice(&chunk[..read]);
    }
    bytes[header_end..header_end + content_length].to_vec()
}

fn block_on<T>(future: impl Future<Output = T>) -> T {
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    let mut future = Box::pin(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}
