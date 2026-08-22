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
    AiLimits, AiMessage, AiMessageRole, AiOutput, AiRequest, AiTask, ArtifactDigest,
    ArtifactFormat, ArtifactIdentity, BackendDevice, BackendId, CancellationToken, DeviceId,
    DeviceKind, InferenceBackend, ModelDescriptor, ModelId, OpenAiCompatibleBackend,
    OpenAiCompatibleConfig, OpenAiCompatibleEngine, QualityTier, Quantization,
    UnauthenticatedOpenAiHttpTransport,
};
use std::collections::BTreeMap;
use std::future::Future;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
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
    let backend =
        OpenAiCompatibleBackend::new(config, Arc::new(UnauthenticatedOpenAiHttpTransport)).unwrap();
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
