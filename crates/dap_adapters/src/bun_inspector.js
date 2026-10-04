import { existsSync } from "node:fs";
import { basename, isAbsolute } from "node:path";
import { fileURLToPath } from "node:url";

const OriginalWebSocket = globalThis.WebSocket;

globalThis.WebSocket = class extends OriginalWebSocket {
    constructor(...arguments_) {
        super(...arguments_);
        this.addEventListener("message", (event) => {
            if (typeof event.data !== "string") return;

            let message;
            try {
                message = JSON.parse(event.data);
            } catch {
                return;
            }
            if (message?.method !== "Debugger.scriptParsed") return;

            const parameters = message.params;
            if (
                typeof parameters?.url !== "string" ||
                typeof parameters.sourceMapURL !== "string"
            ) {
                return;
            }

            let scriptPath;
            try {
                scriptPath = parameters.url.startsWith("file://")
                    ? fileURLToPath(parameters.url)
                    : parameters.url;
            } catch {
                return;
            }
            if (!isAbsolute(scriptPath) || !existsSync(scriptPath)) return;

            const sourceMapUrl = parameters.sourceMapURL.match(
                /^(data:application\/json(?:;charset=[^;,]+)?;base64,)(.*)$/,
            );
            if (!sourceMapUrl) return;

            let sourceMap;
            try {
                sourceMap = JSON.parse(
                    Buffer.from(sourceMapUrl[2], "base64").toString("utf8"),
                );
            } catch {
                return;
            }
            if (
                !Array.isArray(sourceMap?.sources) ||
                (sourceMap.sourceRoot !== undefined && sourceMap.sourceRoot !== "")
            ) {
                return;
            }

            let changed = false;
            sourceMap.sources = sourceMap.sources.map((source) => {
                if (
                    typeof source === "string" &&
                    isAbsolute(source) &&
                    !existsSync(source) &&
                    basename(source) === basename(scriptPath)
                ) {
                    // Bun test source maps can use /file.test.ts while scriptParsed has the real path.
                    changed = true;
                    return scriptPath;
                }
                return source;
            });
            if (!changed) return;

            parameters.sourceMapURL =
                sourceMapUrl[1] +
                Buffer.from(JSON.stringify(sourceMap)).toString("base64");
            Object.defineProperty(event, "data", {
                value: JSON.stringify(message),
            });
        });
    }
};
