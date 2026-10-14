/** Strict correlated replies: unknown versions, fields and errors never become successes. */
import { type UiConnectionState, type UiLimits } from "./protocol.ts";
export declare const uiErrorCodes: ReadonlySet<string>;
export declare function record(value: unknown): value is Record<string, unknown>;
export declare function decodeUiReplyV1(json: string, surfaceId: string, requestId: string, limits: UiLimits): {
    state: UiConnectionState;
    value: unknown;
};
