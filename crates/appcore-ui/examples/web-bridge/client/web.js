/** Connects only to the explicitly installed native web bridge; starts no host or login. */
import { UiClient } from "./client.js";
import { UiBridgeError } from "./protocol.js";
import { record, uiErrorCodes } from "./reply.js";
function nativeError(error) {
    const code = typeof error === "string" ? error : record(error) && typeof error.message === "string" ? error.message : null;
    return new UiBridgeError(code && uiErrorCodes.has(code) ? code : "unavailable");
}
/** Native bootstrap metadata is public correlation data, never authentication authority. */
export function connectUiWeb(options = {}, host = globalThis) {
    if (!record(host) || !record(host.appcoreUiV1))
        throw new UiBridgeError("unavailable");
    const metadata = host.appcoreUiV1;
    if (metadata.version !== 1)
        throw new UiBridgeError("unsupported_version");
    if (Object.keys(metadata).length !== 2 || typeof metadata.surface_id !== "string"
        || !/^[a-f0-9]{32}$/.test(metadata.surface_id))
        throw new UiBridgeError("invalid_frame");
    if (!record(host.webkit) || !record(host.webkit.messageHandlers)
        || !record(host.webkit.messageHandlers.appcoreUiV1))
        throw new UiBridgeError("unavailable");
    const handler = host.webkit.messageHandlers.appcoreUiV1;
    const post = handler.postMessage;
    if (typeof post !== "function")
        throw new UiBridgeError("unavailable");
    const transport = {
        async send(frame, signal) {
            if (signal?.aborted)
                throw new UiBridgeError("cancelled");
            try {
                const reply = await post.call(handler, frame);
                if (typeof reply !== "string")
                    throw new UiBridgeError("invalid_frame");
                return reply;
            }
            catch (error) {
                throw error instanceof UiBridgeError ? error : nativeError(error);
            }
        },
    };
    return new UiClient(metadata.surface_id, transport, options);
}
