// Checks that a web project's page is actually wired to its program.
//
//   node scripts/verify-web-page.js <project-dir> [bundle.js]
//
// The dev server serves a project's own public/index.html, and that page only
// does anything if the NXL program's output reaches it. There is no browser in
// CI, so this drives public/bootstrap.js and the compiled program against a
// DOM small enough to read, and asserts the page ended up filled in and the
// splash screen cleared.
//
// Exits non-zero when the page would render empty.

const fs = require("fs");
const path = require("path");
const vm = require("vm");

const projectDir = path.resolve(process.argv[2]);
if (!process.argv[2]) {
  console.error("usage: node scripts/verify-web-page.js <project-dir> [bundle.js]");
  process.exit(2);
}

const bootstrapPath = path.join(projectDir, "public", "bootstrap.js");

// The bundle is whatever `nxl build` produced: prefer the project's own name,
// then fall back to a single JavaScript file under build/.
function findBundle() {
  if (process.argv[3]) {
    return path.resolve(process.argv[3]);
  }
  const buildDir = path.join(projectDir, "build");
  const named = path.join(buildDir, "debug", path.basename(projectDir) + ".js");
  if (fs.existsSync(named)) {
    return named;
  }
  for (const profile of ["release", "debug"]) {
    const dir = path.join(buildDir, profile);
    if (!fs.existsSync(dir)) {
      continue;
    }
    const found = fs.readdirSync(dir).filter((file) => file.endsWith(".js"));
    if (found.length === 1) {
      return path.join(dir, found[0]);
    }
  }
  return named;
}

const bundlePath = findBundle();
for (const required of [bootstrapPath, bundlePath]) {
  if (!fs.existsSync(required)) {
    console.error(`missing ${required}`);
    console.error("build the project first: nxl build");
    process.exit(2);
  }
}

// ── a DOM just large enough ──────────────────────────────────────────────────

function element(id) {
  return {
    id,
    className: "",
    textContent: "",
    children: [],
    scrollTop: 0,
    scrollHeight: 0,
    classList: {
      _set: new Set(),
      add(name) {
        this._set.add(name);
      },
      has(name) {
        return this._set.has(name);
      },
    },
    appendChild(child) {
      this.children.push(child);
    },
    removeChild(child) {
      this.children = this.children.filter((existing) => existing !== child);
    },
    querySelector(selector) {
      const wanted = selector.replace(".", "");
      return this.children.find((child) => child.className === wanted) || null;
    },
  };
}

const ids = [
  "log",
  "splash",
  "splash-stage",
  "page",
  "status",
  "status-text",
  "features",
  "routes",
  "app-name",
  "app-tagline",
];
const registry = {};
for (const id of ids) {
  registry[id] = element(id);
}
registry.features.children.push(
  Object.assign(element("placeholder"), {
    className: "placeholder",
    textContent: "waiting for the app…",
  })
);

// The sandbox gets its own console: the bridge replaces console.log, and that
// must not reach the console running this script.
const sandbox = {
  console: { log() {}, warn() {}, error() {} },
  document: {
    title: "",
    getElementById: (id) => registry[id] || null,
    createElement: (tag) => Object.assign(element(tag), { tagName: tag }),
    addEventListener() {},
  },
  window: { addEventListener() {} },
  setTimeout() {},
};
sandbox.window.document = sandbox.document;

vm.createContext(sandbox);
vm.runInContext(fs.readFileSync(bootstrapPath, "utf8"), sandbox);
vm.runInContext(fs.readFileSync(bundlePath, "utf8"), sandbox);

// ── what the page ended up showing ───────────────────────────────────────────

const page = {
  splashCleared: registry.splash.classList.has("is-done"),
  pageRevealed: registry.page.classList.has("is-live"),
  statusLive: registry.status.classList.has("is-live"),
  statusText: registry["status-text"].textContent,
  heroName: registry["app-name"].textContent,
  heroTagline: registry["app-tagline"].textContent,
  documentTitle: sandbox.document.title,
  features: registry.features.children.map((child) => child.textContent),
  routes: registry.routes.children.map((child) => child.textContent),
  logLines: registry.log.children.length,
  readyMarked: registry.log.children.some((child) => child.className === "is-ready"),
};

const problems = [];
if (!page.splashCleared) {
  problems.push("the splash screen was never cleared: the program printed no READY line");
}
if (!page.pageRevealed) {
  problems.push("the page was never revealed");
}
if (!page.readyMarked) {
  problems.push("the READY line did not reach the boot log");
}
if (page.logLines === 0) {
  problems.push("nothing the program printed reached the page");
}
if (page.features.some((text) => text === "waiting for the app…")) {
  problems.push("the feature list was never filled in");
}
if (page.heroTagline === "Loading the application…") {
  problems.push("the hero never received the app tagline");
}

console.log(JSON.stringify(page, null, 2));
if (problems.length > 0) {
  console.error("\nweb page verification failed:");
  for (const problem of problems) {
    console.error(`  - ${problem}`);
  }
  process.exit(1);
}
console.log("\nweb page verified: the program fills the page and clears the splash screen");