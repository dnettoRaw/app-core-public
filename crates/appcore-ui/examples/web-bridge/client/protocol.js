export const defaultUiLimits = Object.freeze({
    maxFrameBytes: 256 * 1024, maxPayloadBytes: 64 * 1024, maxTimeoutMs: 30_000,
});
/** A protocol-owned failure: application content and credentials are never details. */
export class UiBridgeError extends Error {
    code;
    state;
    constructor(code, state) {
        super(code === "unsupported_version" ? "NO MORE SUPPORTED PLEASE UPDATE" : `UI bridge: ${code}`);
        this.name = "UiBridgeError";
        this.code = code;
        this.state = state;
    }
}
const encoder = new TextEncoder();
export function encodedJson(value) {
    try {
        const json = JSON.stringify(value, (_key, item) => {
            if ((typeof item === "number" && !Number.isFinite(item)) || ["undefined", "function", "symbol", "bigint"].includes(typeof item)) {
                throw new UiBridgeError("invalid_frame");
            }
            return item;
        });
        if (json === undefined)
            throw new UiBridgeError("invalid_frame");
        return json;
    }
    catch {
        throw new UiBridgeError("invalid_frame");
    }
}
export function byteLength(value) { return encoder.encode(value).byteLength; }
export function validateName(value) {
    if (typeof value !== "string" || value.length === 0 || value.length > 128 || !/^[a-zA-Z0-9._:-]+$/.test(value)) {
        throw new UiBridgeError("invalid_request");
    }
}
export function validateLimits(limits) {
    if (!Number.isInteger(limits.maxFrameBytes) || limits.maxFrameBytes <= 0 || limits.maxFrameBytes > 1024 * 1024
        || !Number.isInteger(limits.maxPayloadBytes) || limits.maxPayloadBytes <= 0 || limits.maxPayloadBytes > limits.maxFrameBytes
        || !Number.isInteger(limits.maxTimeoutMs) || limits.maxTimeoutMs <= 0 || limits.maxTimeoutMs > 60_000) {
        throw new UiBridgeError("invalid_request");
    }
}
/** Validate before handing the frame to a selected transport. */
export function encodeUiRequestV1(request, limits = defaultUiLimits) {
    validateLimits(limits);
    if (request.version !== 1)
        throw new UiBridgeError("unsupported_version");
    validateName(request.surface_id);
    validateName(request.request_id);
    if (!Number.isInteger(request.timeout_ms) || request.timeout_ms <= 0 || request.timeout_ms > limits.maxTimeoutMs) {
        throw new UiBridgeError("invalid_request");
    }
    const action = request.action;
    switch (action.kind) {
        case "query": {
            validateName(action.query.query_name);
            validateName(action.query.query_id);
            if (action.query.query_id !== request.request_id)
                throw new UiBridgeError("invalid_request");
            if (byteLength(encodedJson(action.query.payload)) > limits.maxPayloadBytes)
                throw new UiBridgeError("payload_too_large");
            break;
        }
        case "command": {
            validateName(action.command.command_name);
            validateName(action.command.command_id);
            if (action.command.command_id !== request.request_id)
                throw new UiBridgeError("invalid_request");
            if (action.command.idempotency_key === null || action.command.idempotency_key === undefined)
                throw new UiBridgeError("idempotency_required");
            validateName(action.command.idempotency_key);
            if (typeof action.command.payload !== "string")
                throw new UiBridgeError("invalid_request");
            if (byteLength(action.command.payload) > limits.maxPayloadBytes)
                throw new UiBridgeError("payload_too_large");
            break;
        }
        case "subscribe":
            validateName(action.capability);
            validateName(action.topic);
            break;
        case "poll":
        case "unsubscribe":
            validateName(action.subscription_id);
            break;
        case "cancel":
            validateName(action.target_request_id);
            break;
        case "close": break;
        default: throw new UiBridgeError("invalid_frame");
    }
    const json = encodedJson(request);
    if (byteLength(json) > limits.maxFrameBytes)
        throw new UiBridgeError("payload_too_large");
    return json;
}
