import { afterAll, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const directory = mkdtempSync(join(tmpdir(), "zed-bun-inspector-"));
const scriptPath = join(directory, "café.test.ts");
const existingSource = join(directory, "existing", "café.test.ts");
writeFileSync(scriptPath, "const greeting = 'olá';\n");
mkdirSync(join(directory, "existing"));
writeFileSync(existingSource, "const value = 42;\n");

const NativeWebSocket = globalThis.WebSocket;
class MockWebSocket extends EventTarget {
    static OPEN = 1;

    constructor(...arguments_) {
        super();
        this.arguments = arguments_;
    }

    dispatchEvent(event) {
        const result = super.dispatchEvent(event);
        this.onmessage?.call(this, event);
        return result;
    }
}
globalThis.WebSocket = MockWebSocket;
await import("./bun_inspector.js");

afterAll(() => {
    globalThis.WebSocket = NativeWebSocket;
    rmSync(directory, { recursive: true });
});

function message(sourceMap, url = scriptPath, prefix = "data:application/json;base64,") {
    return JSON.stringify({
        method: "Debugger.scriptParsed",
        params: {
            url,
            scriptId: "42",
            sourceMapURL: prefix + Buffer.from(JSON.stringify(sourceMap)).toString("base64"),
        },
    });
}

function receive(data, listener = () => {}) {
    const socket = new WebSocket("ws://127.0.0.1/inspector", {
        headers: { "Ref-Event-Loop": "0" },
    });
    const event = new MessageEvent("message", { data, origin: "inspector" });
    let received;
    socket.addEventListener("message", function (messageEvent) {
        expect(this).toBe(socket);
        expect(messageEvent).toBe(event);
        received = messageEvent.data;
        listener(messageEvent);
    });
    socket.onmessage = function (messageEvent) {
        expect(this).toBe(socket);
        expect(messageEvent).toBe(event);
        expect(messageEvent.data).toBe(received);
        expect(messageEvent.origin).toBe("inspector");
    };
    socket.dispatchEvent(event);
    expect(socket.arguments[1].headers["Ref-Event-Loop"]).toBe("0");
    expect(WebSocket.OPEN).toBe(MockWebSocket.OPEN);
    return received;
}

test("normalizes Bun's synthetic source path and preserves UTF-8 source content", () => {
    const sourceMap = {
        version: 3,
        sources: ["/café.test.ts"],
        sourcesContent: ["const greeting = 'olá';\n"],
        mappings: "AAAA",
        names: [],
    };
    for (const url of [scriptPath, pathToFileURL(scriptPath).href]) {
        for (const prefix of ["data:application/json;base64,", "data:application/json;charset=utf-8;base64,"]) {
            const received = JSON.parse(receive(message(sourceMap, url, prefix)));
            expect(received.params.scriptId).toBe("42");
            const decoded = JSON.parse(
                Buffer.from(received.params.sourceMapURL.split(",")[1], "base64").toString("utf8"),
            );
            expect(decoded).toEqual({ ...sourceMap, sources: [scriptPath] });
        }
    }
});

test("preserves real sources, other filenames, source roots, and malformed messages", () => {
    const messages = [
        message({ sources: [scriptPath] }),
        message({ sources: ["/another-file.test.ts"] }),
        message({ sources: ["café.test.ts"] }),
        message({ sources: ["/café.test.ts"], sourceRoot: directory }),
        message({ sources: [existingSource] }),
        message({ sources: ["/café.test.ts"] }, "https://example.com/café.test.ts"),
        message(null),
        JSON.stringify({ method: "Debugger.scriptParsed", params: { url: scriptPath, sourceMapURL: "data:application/json;base64,invalid" } }),
        JSON.stringify({ method: "Debugger.paused", params: { reason: "breakpoint" } }),
        "null",
        "invalid JSON",
        new Uint8Array([1, 2, 3]),
    ];
    for (const data of messages) expect(receive(data)).toBe(data);
});

test("retains listener removal semantics", () => {
    const socket = new WebSocket("ws://127.0.0.1/inspector");
    let called = false;
    const listener = () => { called = true; };
    socket.addEventListener("message", listener);
    socket.removeEventListener("message", listener);
    socket.dispatchEvent(new MessageEvent("message", { data: "{}" }));
    expect(called).toBe(false);
});
