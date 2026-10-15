// Drives the extension's own language client against a real `nxlc lsp`.
//
//   node scripts/lsp-client-check.js <path-to-nxlc> [path-to-extension]
//
// The extension wires completion, hover, definition and diagnostics to
// LanguageClient. Those features are only real if the client can actually talk
// to the server, so this exercises the shipped client file rather than a copy:
// it opens a document, asks the questions the providers ask, and checks that
// the diagnostics callback fires.

const path = require('path');

const binary = process.argv[2];
const extensionDir = process.argv[3] || path.join(__dirname, '..', 'editors', 'vscode-nxl');
if (!binary) {
  console.error("usage: node scripts/lsp-client-check.js <path-to-nxlc> [extension-dir]");
  process.exit(2);
}

const { LanguageClient } = require(path.join(extensionDir, 'language-client.js'));

const SOURCE = [
  "fn add(a: Int, b: Int) -> Int {",
  "    return a + b;",
  "}",
  "",
  "fn main() {",
  "    print(add(2, 3));",
  "}",
  "",
].join("\n");

const logs = [];
const published = [];

const client = new LanguageClient(
  (uri, items) => published.push({ uri, items }),
  (message) => logs.push(message)
);

// A stand-in for a vscode.TextDocument: the client only ever calls toString,
// getText and version on it.
const fakeDocument = (text) => ({
  version: 1,
  uri: { toString: () => "file:///project/src/main.nxl" },
  getText: () => text,
});

const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

(async () => {
  const started = await client.start(binary);
  const problems = [];
  if (!started) {
    problems.push(`the client could not start ${binary}`);
  } else {
    client.didOpen(fakeDocument(SOURCE));
    await wait(200);

    const completion = await client.request('textDocument/completion', {
      textDocument: { uri: 'file:///project/src/main.nxl' },
      position: { line: 5, character: 10 },
    });
    const labels = (completion.items || []).map((item) => item.label);
    if (!labels.includes('add')) {
      problems.push('completion did not offer the document\'s own functions');
    }
    if (!labels.includes('fn')) {
      problems.push('completion did not offer keywords');
    }

    const hover = await client.request('textDocument/hover', {
      textDocument: { uri: 'file:///project/src/main.nxl' },
      position: { line: 0, character: 4 },
    });
    if (!hover || !hover.contents || !hover.contents.value.includes('add')) {
      problems.push(`hover did not describe add: ${JSON.stringify(hover)}`);
    }

    const definition = await client.request('textDocument/definition', {
      textDocument: { uri: 'file:///project/src/main.nxl' },
      position: { line: 5, character: 11 },
    });
    if (!Array.isArray(definition) || definition.length === 0) {
      problems.push('definition returned nothing');
    } else if (definition[0].range.start.line !== 0) {
      problems.push(`definition pointed at line ${definition[0].range.start.line}`);
    }

    const symbols = await client.request('textDocument/documentSymbol', {
      textDocument: { uri: 'file:///project/src/main.nxl' },
    });
    if (!Array.isArray(symbols) || symbols.length < 2) {
      problems.push('documentSymbol returned too little for an outline');
    }

    // Diagnostics arrive as a notification, not a reply.
    await wait(150);
    if (published.length === 0) {
      problems.push('no diagnostics were published for an open document');
    } else if (published[0].items.length !== 0) {
      problems.push('a valid document was reported as broken');
    }

    // A document that does not compile must produce a diagnostic with a place.
    const broken = fakeDocument('fn main() { let x = ; }\n');
    broken.version = 2;
    client.didChange(broken);
    await wait(250);
    const last = published[published.length - 1];
    if (!last || last.items.length === 0) {
      problems.push('breaking the file produced no diagnostics');
    } else if (typeof last.items[0].range.start.line !== 'number') {
      problems.push('the diagnostic has no position');
    }

    await client.stop();
  }

  console.log(
    JSON.stringify(
      {
        started,
        logs,
        diagnosticsPublished: published.length,
        lastDiagnostic: published.length
          ? published[published.length - 1].items[0]
          : null,
      },
      null,
      2
    )
  );

  if (problems.length > 0) {
    console.error('\nlanguage client check failed:');
    for (const problem of problems) {
      console.error(`  - ${problem}`);
    }
    process.exit(1);
  }
  console.log('\nlanguage client verified against the real server');
})();