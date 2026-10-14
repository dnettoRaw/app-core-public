/** UI V1 framing composes the existing Runtime command and query DTOs. */
export type JsonValue = null | boolean | number | string | JsonValue[] | {
    [key: string]: JsonValue;
};
export type UiConnectionState = "connected" | "offline" | "degraded" | "closed";
export type UiErrorCode = "unsupported_version" | "invalid_frame" | "invalid_request" | "payload_too_large" | "idempotency_required" | "authentication_required" | "forbidden" | "idempotency_pending" | "idempotency_conflict" | "busy" | "unavailable" | "cancelled" | "deadline_exceeded" | "closed" | "subscription_not_found" | "sequence_exhausted" | "backend_failure";
export type UiActionV1 = {
    kind: "query";
    query: {
        query_name: string;
        query_id: string;
        payload: JsonValue;
    };
} | {
    kind: "command";
    command: {
        command_name: string;
        command_id: string;
        payload: string;
        idempotency_key: string | null;
    };
} | {
    kind: "subscribe";
    capability: string;
    topic: string;
} | {
    kind: "poll" | "unsubscribe";
    subscription_id: string;
} | {
    kind: "cancel";
    target_request_id: string;
} | {
    kind: "close";
};
export interface UiRequestV1 {
    version: 1;
    surface_id: string;
    request_id: string;
    timeout_ms: number;
    action: UiActionV1;
}
export interface UiLimits {
    maxFrameBytes: number;
    maxPayloadBytes: number;
    maxTimeoutMs: number;
}
export declare const defaultUiLimits: Readonly<UiLimits>;
/** A protocol-owned failure: application content and credentials are never details. */
export declare class UiBridgeError extends Error {
    readonly code: UiErrorCode;
    readonly state?: UiConnectionState;
    constructor(code: UiErrorCode, state?: UiConnectionState);
}
export declare function encodedJson(value: unknown): string;
export declare function byteLength(value: string): number;
export declare function validateName(value: unknown): asserts value is string;
export declare function validateLimits(limits: UiLimits): void;
/** Validate before handing the frame to a selected transport. */
export declare function encodeUiRequestV1(request: UiRequestV1, limits?: UiLimits): string;
