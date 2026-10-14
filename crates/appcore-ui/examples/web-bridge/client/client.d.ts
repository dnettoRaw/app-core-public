/** A selected transport client; it never starts a Runtime, retries effects or owns authentication. */
import { type JsonValue, type UiConnectionState, type UiLimits } from "./protocol.ts";
export interface UiTransport {
    send(frame: string, signal?: AbortSignal): Promise<string>;
}
export interface UiCallOptions {
    timeoutMs?: number;
    signal?: AbortSignal;
}
export interface UiCommandOptions extends UiCallOptions {
    idempotencyKey: string;
}
export interface UiOperation<Input, Output> {
    readonly kind: "query" | "command";
    readonly capability: string;
    readonly encode: (input: Input) => JsonValue;
    readonly decode: (value: unknown) => Output;
}
export declare function defineUiQuery<Input, Output>(definition: Omit<UiOperation<Input, Output>, "kind">): UiOperation<Input, Output>;
export declare function defineUiCommand<Input, Output>(definition: Omit<UiOperation<Input, Output>, "kind">): UiOperation<Input, Output>;
export interface UiClientOptions {
    limits?: UiLimits;
    maxPending?: number;
    maxSubscriptions?: number;
    defaultTimeoutMs?: number;
}
export interface UiClientSnapshot {
    state: UiConnectionState;
    pending: number;
    wireInflight: number;
    subscriptions: number;
    observerFailures: number;
    cleanupFailures: number;
}
export interface UiEvent {
    sequence: number;
    topic: string;
    payload: unknown;
}
export interface UiEventPage {
    subscription_id: string;
    events: UiEvent[];
    dropped: number;
}
export declare class UiClient {
    readonly surfaceId: string;
    private readonly transport;
    private readonly limits;
    private readonly maxPending;
    private readonly maxSubscriptions;
    private readonly timeoutMs;
    private readonly pending;
    private readonly subscriptions;
    private readonly observers;
    private pendingSubscriptions;
    private normalWire;
    private controlWire;
    private connection;
    private closed;
    private observerFailures;
    private cleanupFailures;
    constructor(surfaceId: string, transport: UiTransport, options?: UiClientOptions);
    snapshot(): UiClientSnapshot;
    onStateChanged(observer: (state: UiConnectionState) => void): () => void;
    query<Input, Output>(operation: UiOperation<Input, Output>, input: Input, options?: UiCallOptions): Promise<Output>;
    command<Input, Output>(operation: UiOperation<Input, Output>, input: Input, options: UiCommandOptions): Promise<Output>;
    subscribe(capability: string, topic: string, options?: UiCallOptions): Promise<UiSubscription>;
    /** Close this owned surface; no application command is retried or undone. */
    close(): Promise<void>;
    pollSubscription(subscription: UiSubscription, options: UiCallOptions): Promise<UiEventPage>;
    releaseSubscription(subscription: UiSubscription): Promise<void>;
    private input;
    private output;
    private changeState;
    private request;
}
export declare class UiSubscription {
    readonly id: string;
    readonly topic: string;
    lastSequence: number;
    private active;
    private polling;
    private readonly client;
    constructor(client: UiClient, id: string, topic: string);
    poll(options?: UiCallOptions): Promise<UiEventPage>;
    close(): Promise<void>;
    /** Detaches resources when the owning surface closes. */
    detach(): void;
}
