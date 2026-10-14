/** A selected transport client; it never starts a Runtime, retries effects or owns authentication. */
import { defaultUiLimits, encodedJson, encodeUiRequestV1, byteLength, validateLimits, validateName, UiBridgeError } from "./protocol.js";
import { decodeUiReplyV1, record } from "./reply.js";
export function defineUiQuery(definition) {
    validateName(definition.capability);
    return Object.freeze({ ...definition, kind: "query" });
}
export function defineUiCommand(definition) {
    validateName(definition.capability);
    return Object.freeze({ ...definition, kind: "command" });
}
export class UiClient {
    surfaceId;
    transport;
    limits;
    maxPending;
    maxSubscriptions;
    timeoutMs;
    pending = new Map();
    subscriptions = new Set();
    observers = new Set();
    pendingSubscriptions = 0;
    normalWire = 0;
    controlWire = 0;
    connection = "offline";
    closed = false;
    observerFailures = 0;
    cleanupFailures = 0;
    constructor(surfaceId, transport, options = {}) {
        validateName(surfaceId);
        this.surfaceId = surfaceId;
        this.transport = transport;
        this.limits = options.limits ?? defaultUiLimits;
        validateLimits(this.limits);
        this.maxPending = options.maxPending ?? 16;
        this.maxSubscriptions = options.maxSubscriptions ?? 32;
        this.timeoutMs = options.defaultTimeoutMs ?? Math.min(15_000, this.limits.maxTimeoutMs);
        if (![this.maxPending, this.maxSubscriptions].every(value => Number.isInteger(value) && value > 0 && value <= 256)
            || !Number.isInteger(this.timeoutMs) || this.timeoutMs <= 0 || this.timeoutMs > this.limits.maxTimeoutMs)
            throw new UiBridgeError("invalid_request");
    }
    snapshot() {
        return { state: this.connection, pending: this.pending.size, wireInflight: this.normalWire + this.controlWire,
            subscriptions: this.subscriptions.size, observerFailures: this.observerFailures, cleanupFailures: this.cleanupFailures };
    }
    onStateChanged(observer) {
        if (this.closed)
            throw new UiBridgeError("closed");
        if (this.observers.size >= 32)
            throw new UiBridgeError("busy");
        this.observers.add(observer);
        return () => { this.observers.delete(observer); };
    }
    async query(operation, input, options = {}) {
        if (operation.kind !== "query")
            throw new UiBridgeError("invalid_request");
        const payload = this.input(operation, input);
        const value = await this.request(id => ({ kind: "query", query: { query_name: operation.capability, query_id: id, payload } }), options);
        return this.output(operation, value);
    }
    async command(operation, input, options) {
        if (operation.kind !== "command")
            throw new UiBridgeError("invalid_request");
        if (!options?.idempotencyKey)
            throw new UiBridgeError("idempotency_required");
        const payload = encodedJson(this.input(operation, input));
        const value = await this.request(id => ({ kind: "command", command: { command_name: operation.capability, command_id: id, payload, idempotency_key: options.idempotencyKey } }), options);
        return this.output(operation, value);
    }
    async subscribe(capability, topic, options = {}) {
        validateName(capability);
        validateName(topic);
        if (this.subscriptions.size + this.pendingSubscriptions >= this.maxSubscriptions)
            throw new UiBridgeError("busy");
        this.pendingSubscriptions++;
        try {
            const value = await this.request(() => ({ kind: "subscribe", capability, topic }), options);
            if (!record(value) || Object.keys(value).length !== 1)
                throw new UiBridgeError("invalid_frame");
            validateName(value.subscription_id);
            const subscription = new UiSubscription(this, value.subscription_id, topic);
            if (this.closed)
                throw new UiBridgeError("closed");
            this.subscriptions.add(subscription);
            return subscription;
        }
        finally {
            this.pendingSubscriptions--;
        }
    }
    /** Close this owned surface; no application command is retried or undone. */
    async close() {
        if (this.closed)
            return;
        this.closed = true;
        for (const pending of this.pending.values()) {
            pending.abort.abort();
            pending.reject(new UiBridgeError("closed"));
        }
        for (const subscription of this.subscriptions)
            subscription.detach();
        this.subscriptions.clear();
        this.changeState("closed");
        this.observers.clear();
        try {
            await this.request(() => ({ kind: "close" }), {}, true);
        }
        catch (error) {
            this.cleanupFailures = Math.min(Number.MAX_SAFE_INTEGER, this.cleanupFailures + 1);
            throw error instanceof UiBridgeError ? error : new UiBridgeError("unavailable");
        }
    }
    async pollSubscription(subscription, options) {
        if (!this.subscriptions.has(subscription))
            throw new UiBridgeError("subscription_not_found");
        const value = await this.request(() => ({ kind: "poll", subscription_id: subscription.id }), options);
        if (!record(value) || Object.keys(value).length !== 3 || value.subscription_id !== subscription.id
            || !Array.isArray(value.events) || value.events.length > 1024 || !Number.isSafeInteger(value.dropped) || value.dropped < 0)
            throw new UiBridgeError("invalid_frame");
        let sequence = subscription.lastSequence;
        for (const event of value.events) {
            if (!record(event) || Object.keys(event).length !== 3 || !Number.isSafeInteger(event.sequence) || event.sequence <= sequence
                || event.topic !== subscription.topic || !Object.hasOwn(event, "payload"))
                throw new UiBridgeError("invalid_frame");
            sequence = event.sequence;
        }
        subscription.lastSequence = sequence;
        return value;
    }
    async releaseSubscription(subscription) {
        if (!this.subscriptions.delete(subscription) || this.closed)
            return;
        await this.request(() => ({ kind: "unsubscribe", subscription_id: subscription.id }), {}, true);
    }
    input(operation, input) {
        try {
            return operation.encode(input);
        }
        catch {
            throw new UiBridgeError("invalid_request");
        }
    }
    output(operation, value) {
        if (byteLength(encodedJson(value)) > this.limits.maxPayloadBytes)
            throw new UiBridgeError("payload_too_large");
        try {
            return operation.decode(value);
        }
        catch {
            throw new UiBridgeError("invalid_frame");
        }
    }
    changeState(state) {
        if (this.closed && state !== "closed")
            return;
        if (this.connection === state)
            return;
        this.connection = state;
        for (const observer of this.observers) {
            try {
                observer(state);
            }
            catch {
                this.observerFailures = Math.min(Number.MAX_SAFE_INTEGER, this.observerFailures + 1);
            }
        }
    }
    request(action, options, control = false) {
        if (this.closed && !control)
            return Promise.reject(new UiBridgeError("closed"));
        if (options.signal?.aborted)
            return Promise.reject(new UiBridgeError("cancelled"));
        if ((!control && this.normalWire >= this.maxPending) || (control && this.controlWire >= 2))
            return Promise.reject(new UiBridgeError("busy"));
        let id;
        try {
            const bytes = crypto.getRandomValues(new Uint8Array(16));
            id = Array.from(bytes, byte => byte.toString(16).padStart(2, "0")).join("");
        }
        catch {
            return Promise.reject(new UiBridgeError("unavailable"));
        }
        const timeoutMs = options.timeoutMs ?? this.timeoutMs;
        const selected = action(id);
        let encoded;
        try {
            encoded = encodeUiRequestV1({ version: 1, surface_id: this.surfaceId, request_id: id, timeout_ms: timeoutMs, action: selected }, this.limits);
        }
        catch (error) {
            return Promise.reject(error);
        }
        const abort = new AbortController();
        if (control)
            this.controlWire++;
        else
            this.normalWire++;
        return new Promise((resolve, reject) => {
            let settled = false;
            const finish = (error, value) => {
                if (settled)
                    return;
                settled = true;
                clearTimeout(timer);
                options.signal?.removeEventListener("abort", onAbort);
                this.pending.delete(id);
                if (error)
                    reject(error);
                else
                    resolve(value);
            };
            const cancel = (error) => {
                abort.abort();
                finish(error);
                if (selected.kind === "query" || selected.kind === "command") {
                    void this.request(() => ({ kind: "cancel", target_request_id: id }), {}, true).catch(() => { });
                }
            };
            const onAbort = () => cancel(new UiBridgeError("cancelled"));
            const timer = setTimeout(() => { this.changeState("degraded"); cancel(new UiBridgeError("deadline_exceeded")); }, timeoutMs);
            this.pending.set(id, { reject: error => finish(error), abort });
            options.signal?.addEventListener("abort", onAbort, { once: true });
            const releaseWire = () => { if (control)
                this.controlWire--;
            else
                this.normalWire--; };
            Promise.resolve().then(() => {
                if (abort.signal.aborted)
                    throw new UiBridgeError("cancelled");
                return this.transport.send(encoded, abort.signal);
            }).then(body => {
                releaseWire();
                if (settled)
                    return;
                try {
                    const reply = decodeUiReplyV1(body, this.surfaceId, id, this.limits);
                    finish(undefined, reply.value);
                    this.changeState(reply.state);
                }
                catch (error) {
                    if (error instanceof UiBridgeError && error.state)
                        this.changeState(error.state);
                    finish(error instanceof UiBridgeError ? error : new UiBridgeError("invalid_frame"));
                }
            }, error => {
                releaseWire();
                if (settled)
                    return;
                const controlled = error instanceof UiBridgeError ? error : new UiBridgeError("unavailable");
                if (controlled.code === "unavailable")
                    this.changeState("offline");
                finish(controlled);
            });
        });
    }
}
export class UiSubscription {
    id;
    topic;
    lastSequence = 0;
    active = true;
    polling = false;
    client;
    constructor(client, id, topic) { this.client = client; this.id = id; this.topic = topic; }
    async poll(options = {}) {
        if (!this.active)
            throw new UiBridgeError("subscription_not_found");
        if (this.polling)
            throw new UiBridgeError("busy");
        this.polling = true;
        try {
            return await this.client.pollSubscription(this, options);
        }
        catch (error) {
            if (error instanceof UiBridgeError && ["authentication_required", "forbidden", "subscription_not_found", "closed"].includes(error.code)) {
                this.active = false;
                void this.client.releaseSubscription(this).catch(() => { });
            }
            throw error;
        }
        finally {
            this.polling = false;
        }
    }
    async close() { if (!this.active)
        return; this.active = false; await this.client.releaseSubscription(this); }
    /** Detaches resources when the owning surface closes. */
    detach() { this.active = false; }
}
