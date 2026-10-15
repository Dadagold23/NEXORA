// Drives `nxlc lsp` over real stdio, the way an editor does.
//
//   node scripts/lsp-smoke.js <path to nxlc>
//
// Speaks the actual protocol - didOpen, hover, definition, completion,
// didChange - and checks that the answers come back and that a broken document
// produces diagnostics. Exits non-zero when the server misbehaves.

const { spawn } = require("child_process");

const binary = process.argv[2];
if (!binary) {
  console.error("usage: node scripts/lsp-smoke.js <path-to-nxlc>");
  process.exit(2);
}

const child = spawn(binary, ["lsp"], { stdio: ["pipe", "pipe", "pipe"] });

const received = [];
let buffer = Buffer.alloc(0);

child.stdout.on("data", (chunk) => {
  buffer = Buffer.concat([buffer, chunk]);
  for (;;) {
    const marker = buffer.indexOf("\r\n\r\n");
    if (marker < 0) return;
    const header = buffer.subarray(0, marker).toString("ascii");
    const match = /Content-Length:\s*(\d+)/i.exec(header);
    if (!match) return;
    const length = Number(match[1]);
    const body = buffer.subarray(marker + 4, marker + 4 + length);
    if (body.length < length) return;
    buffer = buffer.subarray(marker + 4 + length);
    try {
      received.push(JSON.parse(body.toString("utf8")));
    } catch (error) {
      received.push({ parseError: String(error), raw: body.toString("utf8") });
    }
  }
});

let stderr = "";
child.stderr.on("data", (chunk) => {
  stderr += chunk.toString();
});

function send(message) {
  const body = Buffer.from(JSON.stringify(message), "utf8");
  child.stdin.write(`Content-Length: ${body.length}\r\n\r\n`);
  child.stdin.write(body);
}

const uri = "file:///c:/work/site/src/main.nxl";
const good = [
  "fn add(a: Int, b: Int) -> Int {",
  "    return a + b;",
  "}",
  "",
  "fn main() {",
  "    print(add(2, 3));",
  "}",
  "",
].join("\n");
const broken = "fn main() { let x = ; }\n";

const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

function byId(id) {
  return received.find((message) => message.id === id);
}

function diagnosticsFor() {
  return received.filter(
    (message) => message.method === "textDocument/publishDiagnostics"
  );
}

(async () => {
  send({ jsonrpc: "2.0", id: 1, method: "initialize", params: { rootUri: uri, capabilities: {} } });
  await wait(150);
  send({ jsonrpc: "2.0", method: "initialized", params: {} });
  send({
    jsonrpc: "2.0",
    method: "textDocument/didOpen",
    params: { textDocument: { uri, languageId: "nxl", version: 1, text: good } },
  });
  await wait(150);
  // Cursor on `add` in its declaration.
  send({
    jsonrpc: "2.0",
    id: 2,
    method: "textDocument/hover",
    params: { textDocument: { uri }, position: { line: 0, character: 4 } },
  });
  // Cursor on the use of `add` inside main.
  send({
    jsonrpc: "2.0",
    id: 3,
    method: "textDocument/definition",
    params: { textDocument: { uri }, position: { line: 5, character: 11 } },
  });
  send({
    jsonrpc: "2.0",
    id: 4,
    method: "textDocument/completion",
    params: { textDocument: { uri }, position: { line: 5, character: 10 } },
  });
  send({
    jsonrpc: "2.0",
    id: 5,
    method: "textDocument/documentSymbol",
    params: { textDocument: { uri } },
  });
  await wait(250);
  send({
    jsonrpc: "2.0",
    method: "textDocument/didChange",
    params: { textDocument: { uri, version: 2 }, contentChanges: [{ text: broken }] },
  });
  await wait(250);
  send({ jsonrpc: "2.0", id: 6, method: "shutdown", params: {} });
  await wait(150);
  send({ jsonrpc: "2.0", method: "exit", params: {} });
  await wait(250);

  const problems = [];
  const initialize = byId(1);
  if (!initialize || !initialize.result) {
    problems.push("initialize returned nothing");
  } else {
    const capabilities = initialize.result.capabilities || {};
    for (const capability of [
      "hoverProvider",
      "definitionProvider",
      "documentSymbolProvider",
      "completionProvider",
    ]) {
      if (!capabilities[capability]) {
        problems.push(`initialize does not advertise ${capability}`);
      }
    }
  }

  const hover = byId(2);
  if (!hover || !hover.result) {
    problems.push("hover returned nothing on a known symbol");
  } else if (!/add/.test(JSON.stringify(hover.result))) {
    problems.push(`hover did not describe 'add': ${JSON.stringify(hover.result)}`);
  }

  const definition = byId(3);
  if (!definition || !Array.isArray(definition.result) || definition.result.length === 0) {
    problems.push("definition returned nothing for a known symbol");
  } else if (definition.result[0].range.start.line !== 0) {
    problems.push(`definition pointed at line ${definition.result[0].range.start.line}, expected 0`);
  }

  const completion = byId(4);
  const items = completion && completion.result && completion.result.items;
  if (!Array.isArray(items) || items.length === 0) {
    problems.push("completion returned no items");
  } else if (!items.some((item) => item.label === "add")) {
    problems.push("completion does not offer the document's own symbols");
  }

  const symbols = byId(5);
  if (!symbols || !Array.isArray(symbols.result) || symbols.result.length < 2) {
    problems.push("documentSymbol did not report the file's declarations");
  } else if (!symbols.result[0].range) {
    problems.push("documentSymbol entries have no range, so an outline cannot use them");
  }

  const published = diagnosticsFor();
  if (published.length < 2) {
    problems.push(`expected diagnostics after open and after change, saw ${published.length}`);
  } else {
    if ((published[0].params.diagnostics || []).length !== 0) {
      problems.push("a valid document was reported as broken");
    }
    const afterBreak = published[published.length - 1].params.diagnostics || [];
    if (afterBreak.length === 0) {
      problems.push("a broken document produced no diagnostics");
    } else {
      const first = afterBreak[0];
      if (typeof first.range?.start?.line !== "number" || !first.message) {
        problems.push("diagnostics have no position or message");
      }
    }
  }

  if (stderr.trim()) {
    problems.push(`server wrote to stderr: ${stderr.trim()}`);
  }

  child.kill();

  const summary = {
    initialize: !!initialize,
    hover: hover ? JSON.parse(JSON.stringify(hover.result)) : null,
    definition: definition ? JSON.stringify(definition.result[0]) : null,
    completionItems: items ? items.length : 0,
    symbols: symbols ? symbols.result.map((s) => s.name) : [],
    diagnosticsPublished: published.length,
    lastDiagnostic: diagnosticsFor().length
      ? diagnosticsFor()[diagnosticsFor().length - 1].params.diagnostics[0]
      : null,
  };
  console.log(JSON.stringify(summary, null, 2));

  if (problems.length > 0) {
    console.error("\nlanguage server check failed:");
    for (const problem of problems) {
      console.error(`  - ${problem}`);
    }
    process.exit(1);
  }
  console.log("\nlanguage server verified over stdio");
})();