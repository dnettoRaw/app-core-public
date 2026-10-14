/** Strict correlated replies: unknown versions, fields and errors never become successes. */
import { byteLength, UiBridgeError } from "./protocol.js";
export const uiErrorCodes = new Set([
    "unsupported_version", "invalid_frame", "invalid_request", "payload_too_large", "idempotency_required",
    "authentication_required", "forbidden", "busy", "unavailable", "cancelled", "deadline_exceeded",
    "idempotency_pending", "idempotency_conflict", "closed", "subscription_not_found", "sequence_exhausted", "backend_failure",
]);
export function record(value) {
    return typeof value === "object" && value !== null && !Array.isArray(value);
}
function keys(value, names) {
    if (Object.keys(value).length !== names.length || Object.keys(value).some(key => !names.includes(key))) {
        throw new UiBridgeError("invalid_frame");
    }
}
export function decodeUiReplyV1(json, surfaceId, requestId, limits) {
    if (typeof json !== "string")
        throw new UiBridgeError("invalid_frame");
    if (json.length > limits.maxFrameBytes || byteLength(json) > limits.maxFrameBytes)
        throw new UiBridgeError("payload_too_large");
    let reply;
    try {
        reply = JSON.parse(json);
    }
    catch {
        throw new UiBridgeError("invalid_frame");
    }
    if (!record(reply))
        throw new UiBridgeError("invalid_frame");
    if (reply.version !== 1)
        throw new UiBridgeError("unsupported_version");
    keys(reply, ["version", "surface_id", "request_id", "state", "outcome"]);
    if (reply.surface_id !== surfaceId || reply.request_id !== requestId)
        throw new UiBridgeError("invalid_frame");
    if (!["connected", "offline", "degraded", "closed"].includes(reply.state) || !record(reply.outcome))
        throw new UiBridgeError("invalid_frame");
    const state = reply.state;
    if (reply.outcome.status === "error") {
        keys(reply.outcome, ["status", "code"]);
        if (typeof reply.outcome.code !== "string" || !uiErrorCodes.has(reply.outcome.code))
            throw new UiBridgeError("invalid_frame");
        throw new UiBridgeError(reply.outcome.code, state);
    }
    if (reply.outcome.status !== "ok")
        throw new UiBridgeError("invalid_frame");
    keys(reply.outcome, ["status", "value"]);
    return { state, value: reply.outcome.value };
}
