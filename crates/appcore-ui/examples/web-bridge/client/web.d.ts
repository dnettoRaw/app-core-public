/** Connects only to the explicitly installed native web bridge; starts no host or login. */
import { UiClient, type UiClientOptions } from "./client.ts";
/** Native bootstrap metadata is public correlation data, never authentication authority. */
export declare function connectUiWeb(options?: UiClientOptions, host?: unknown): UiClient;
