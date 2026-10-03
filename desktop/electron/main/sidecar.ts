import { app } from "electron";
import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { accessSync, constants, existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { configDir } from "./config";

type SidecarStatus = "unavailable" | "starting" | "running" | "stopped";

interface PendingRequest {
  resolve: (value: unknown) => void;
  reject: (error: Error) => void;
  timeout: ReturnType<typeof setTimeout>;
}

interface SidecarEvent {
  event: string;
  payload?: unknown;
  target?: "main" | "settings" | "all" | "tray";
}

interface SidecarOptions {
  appRoot: string;
  onEvent: (event: SidecarEvent) => void | Promise<void>;
}

interface SidecarResponseMessage {
  type: "response";
  id: number;
  ok: boolean;
  result?: unknown;
  error?: string | { message?: string };
}

interface SidecarEventMessage {
  type: "event";
  event: string;
  payload?: unknown;
  target?: SidecarEvent["target"];
}

type SidecarMessage = SidecarResponseMessage | SidecarEventMessage;

const REQUEST_TIMEOUT_MS = 60_000;
const DOWNLOAD_REQUEST_TIMEOUT_MS = 30 * 60_000;

export class SidecarUnavailableError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "SidecarUnavailableError";
  }
}

export class YapCoreSidecar {
  private child: ChildProcessWithoutNullStreams | null = null;
  private executablePath: string | null = null;
  private nextRequestId = 1;
  private pending = new Map<number, PendingRequest>();
  private stdoutBuffer = "";
  private status: SidecarStatus = "stopped";

  constructor(private readonly options: SidecarOptions) {}

  getStatus(): SidecarStatus {
    return this.status;
  }

  async invoke(command: string, args: Record<string, unknown>): Promise<unknown> {
    if (!(await this.ensureStarted())) {
      throw new SidecarUnavailableError("yap-core sidecar is not available");
    }

    const child = this.child;
    if (!child || !child.stdin.writable) {
      throw new SidecarUnavailableError("yap-core sidecar is not writable");
    }

    const id = this.nextRequestId++;
    const message = JSON.stringify({ id, method: command, params: args });

    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`yap-core timed out handling command: ${command}`));
      }, isDownloadCommand(command) ? DOWNLOAD_REQUEST_TIMEOUT_MS : REQUEST_TIMEOUT_MS);

      this.pending.set(id, { resolve, reject, timeout });
      child.stdin.write(`${message}\n`, (error) => {
        if (!error) return;
        clearTimeout(timeout);
        this.pending.delete(id);
        reject(error);
      });
    });
  }

  async stop(): Promise<void> {
    const child = this.child;
    if (!child) return;

    this.child = null;
    this.status = "stopped";
    child.kill();
    this.rejectPending(new SidecarUnavailableError("yap-core sidecar stopped"));
  }

  private async ensureStarted(): Promise<boolean> {
    if (this.child && this.status === "running") return true;

    this.executablePath ??= resolveYapCorePath(this.options.appRoot);
    if (!this.executablePath) {
      this.status = "unavailable";
      return false;
    }

    this.status = "starting";
    const child = spawn(this.executablePath, [], {
      cwd: dirname(this.executablePath),
      env: { ...process.env, YAP_CONFIG_DIR: configDir() },
      stdio: ["pipe", "pipe", "pipe"],
    });

    this.child = child;
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk: string) => this.handleStdout(chunk));
    child.stderr.on("data", (chunk: string) => this.handleStderr(chunk));
    child.on("error", (error) => {
      this.child = null;
      this.status = "unavailable";
      this.rejectPending(error);
    });
    child.on("exit", (code, signal) => {
      this.child = null;
      this.status = "stopped";
      this.rejectPending(new SidecarUnavailableError(`yap-core exited (${code ?? signal ?? "unknown"})`));
      void this.options.onEvent({
        event: "sidecar:status",
        payload: { status: this.status, code, signal },
        target: "all",
      });
    });

    this.status = "running";
    await this.options.onEvent({
      event: "sidecar:status",
      payload: { status: this.status },
      target: "all",
    });
    return true;
  }

  private handleStdout(chunk: string): void {
    this.stdoutBuffer += chunk;

    let newlineIndex = this.stdoutBuffer.indexOf("\n");
    while (newlineIndex >= 0) {
      const line = this.stdoutBuffer.slice(0, newlineIndex).trim();
      this.stdoutBuffer = this.stdoutBuffer.slice(newlineIndex + 1);
      if (line) this.handleLine(line);
      newlineIndex = this.stdoutBuffer.indexOf("\n");
    }
  }

  private handleLine(line: string): void {
    let message: SidecarMessage;
    try {
      message = JSON.parse(line) as SidecarMessage;
    } catch {
      console.warn("[yap-core] ignored non-JSON stdout line:", line);
      return;
    }

    if (message.type === "response") {
      this.handleResponse(message);
      return;
    }

    if (message.type === "event") {
      void this.options.onEvent({
        event: message.event,
        payload: message.payload,
        target: message.target ?? "all",
      });
    }
  }

  private handleResponse(message: SidecarResponseMessage): void {
    const pending = this.pending.get(message.id);
    if (!pending) return;

    clearTimeout(pending.timeout);
    this.pending.delete(message.id);

    if (message.ok) {
      pending.resolve(message.result);
      return;
    }

    pending.reject(new Error(sidecarErrorMessage(message.error)));
  }

  private handleStderr(chunk: string): void {
    for (const line of chunk.split(/\r?\n/)) {
      if (line.trim()) console.warn("[yap-core]", line);
    }
  }

  private rejectPending(error: Error): void {
    for (const [id, pending] of this.pending) {
      clearTimeout(pending.timeout);
      pending.reject(error);
      this.pending.delete(id);
    }
  }
}

function isDownloadCommand(command: string): boolean {
  return command === "models.whisper.download" || command === "models.ollama.pull";
}

function sidecarErrorMessage(error: SidecarResponseMessage["error"]): string {
  if (typeof error === "string") return error;
  if (error && typeof error.message === "string") return error.message;
  return "yap-core command failed";
}

function resolveYapCorePath(appRoot: string): string | null {
  const override = process.env.YAP_CORE_PATH;
  if (override && isExecutableCandidate(override)) return override;

  const executable = process.platform === "win32" ? "yap-core.exe" : "yap-core";
  const candidates = app.isPackaged
    ? [
        join(process.resourcesPath, "yap-core", executable),
        join(process.resourcesPath, "bin", executable),
        join(process.resourcesPath, "app.asar.unpacked", "yap-core", executable),
      ]
    : [
        join(appRoot, "native-core", "target", "debug", executable),
      ];

  return candidates.find(isExecutableCandidate) ?? null;
}

function isExecutableCandidate(path: string): boolean {
  if (!existsSync(path)) return false;
  if (process.platform === "win32") return true;

  try {
    accessSync(path, constants.X_OK);
    return true;
  } catch {
    return false;
  }
}
