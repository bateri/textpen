/**
 * The view's end of MCP Apps, written against the messages of the 2026-01-26 revision
 * and nothing else: JSON-RPC over `postMessage` to whatever frames the page. The
 * official client library does the same and brings the whole MCP SDK with it; this
 * view crosses a terminal's stream before it can start, so it carries only what it
 * uses.
 */

export const PROTOCOL_VERSION = "2026-01-26";

export interface HostContext {
  theme?: "light" | "dark";
  styles?: { variables?: Record<string, string | undefined> };
  containerDimensions?: { width?: number; height?: number };
}

export interface HostCapabilities {
  openLinks?: object;
  experimental?: Record<string, unknown>;
}

export interface ToolResult {
  isError?: boolean;
  content?: { type: string; text?: string }[];
  structuredContent?: unknown;
}

interface Pending {
  resolve: (value: unknown) => void;
  reject: (error: Error) => void;
}

export interface HostHandlers {
  toolResult?: (result: ToolResult) => void;
  toolCancelled?: (reason: string | undefined) => void;
  contextChanged?: (context: HostContext) => void;
  /** The host takes the view down; it waits for the answer, briefly. */
  teardown?: () => Promise<void>;
}

export class Host {
  #next = 1;
  readonly #pending = new Map<number, Pending>();
  #capabilities: HostCapabilities = {};
  #context: HostContext = {};

  constructor(readonly handlers: HostHandlers) {
    window.addEventListener("message", (event) => {
      if (event.source !== window.parent) return;
      this.#receive(event.data);
    });
  }

  get capabilities(): HostCapabilities {
    return this.#capabilities;
  }

  get context(): HostContext {
    return this.#context;
  }

  /** The handshake: `ui/initialize`, the host's answer, `ui/notifications/initialized`. */
  async connect(app: { name: string; version: string }): Promise<void> {
    const result = (await this.request("ui/initialize", {
      appInfo: app,
      // Written for the terminal pane: the view fills it, and every key is the view's.
      // A host that does not know the name ignores it.
      appCapabilities: { experimental: { "pencere/pane": {} } },
      protocolVersion: PROTOCOL_VERSION,
    })) as { hostCapabilities?: HostCapabilities; hostContext?: HostContext };
    this.#capabilities = result.hostCapabilities ?? {};
    this.#context = result.hostContext ?? {};
    applyContext(this.#context);
    this.notify("ui/notifications/initialized");
  }

  request(method: string, params?: unknown): Promise<unknown> {
    const id = this.#next++;
    return new Promise((resolve, reject) => {
      this.#pending.set(id, { resolve, reject });
      this.#send({ id, method, ...(params === undefined ? {} : { params }) });
    });
  }

  notify(method: string, params?: unknown): void {
    this.#send({ method, ...(params === undefined ? {} : { params }) });
  }

  callTool(name: string, args: Record<string, unknown>): Promise<ToolResult> {
    return this.request("tools/call", { name, arguments: args }) as Promise<ToolResult>;
  }

  #send(message: Record<string, unknown>): void {
    window.parent.postMessage({ jsonrpc: "2.0", ...message }, "*");
  }

  #receive(data: unknown): void {
    if (typeof data !== "object" || data === null) return;
    const message = data as {
      jsonrpc?: string;
      id?: number | string;
      method?: string;
      params?: Record<string, unknown>;
      result?: unknown;
      error?: { message?: string };
    };
    if (message.jsonrpc !== "2.0") return;
    if (typeof message.method === "string") {
      if (message.id === undefined) this.#notification(message.method, message.params ?? {});
      else void this.#request(message.id, message.method);
      return;
    }
    if (typeof message.id !== "number") return;
    const waiting = this.#pending.get(message.id);
    if (!waiting) return;
    this.#pending.delete(message.id);
    if (message.error) waiting.reject(new Error(message.error.message ?? "The host refused"));
    else waiting.resolve(message.result);
  }

  #notification(method: string, params: Record<string, unknown>): void {
    switch (method) {
      case "ui/notifications/tool-result":
        this.handlers.toolResult?.(params as ToolResult);
        return;
      case "ui/notifications/tool-cancelled":
        this.handlers.toolCancelled?.(
          typeof params.reason === "string" ? params.reason : undefined,
        );
        return;
      case "ui/notifications/host-context-changed":
        this.#context = { ...this.#context, ...(params as HostContext) };
        applyContext(params as HostContext);
        this.handlers.contextChanged?.(this.#context);
        return;
    }
  }

  async #request(id: number | string, method: string): Promise<void> {
    if (method === "ui/resource-teardown") {
      await this.handlers.teardown?.();
      this.#send({ id, result: {} });
    } else if (method === "ping") {
      this.#send({ id, result: {} });
    } else {
      this.#send({
        id,
        error: { code: -32601, message: `${method} is not a method of this view` },
      });
    }
  }
}

/** The host's theme and colours, as MCP Apps § Theming defines them, on the document. */
export function applyContext(context: HostContext): void {
  const root = document.documentElement;
  if (context.theme) {
    root.setAttribute("data-theme", context.theme);
    root.style.colorScheme = context.theme;
  }
  for (const [name, value] of Object.entries(context.styles?.variables ?? {})) {
    if (typeof value === "string" && name.startsWith("--")) root.style.setProperty(name, value);
  }
}
