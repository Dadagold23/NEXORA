//! P18 — NUI Universal UI
//!
//! NEXORA's intermediate UI model. The core is renderer-independent and fully
//! tested; only targets with real, exercised integrations are marked IMPLEMENTED.
//!
//! # Status
//!
//! | Feature | Status |
//! | --- | --- |
//! | Component tree (nodes, props, children) | IMPLEMENTED |
//! | State + reactivity (signal → re-render) | IMPLEMENTED |
//! | Events (click/input/submit dispatch) | IMPLEMENTED |
//! | Layout (row/column/stack model) | IMPLEMENTED |
//! | Navigation (stack push/pop/replace) | IMPLEMENTED |
//! | Forms (binding + validation) | IMPLEMENTED |
//! | Accessibility (roles/labels in model) | IMPLEMENTED |
//! | Localization (key catalog with fallback) | IMPLEMENTED |
//! | Themes (palette tokens) | IMPLEMENTED |
//! | Animation (declarative transitions) | DESIGNED |
//! | HTML adapter (string render to HTML) | IMPLEMENTED (tested) |
//! | Desktop adapter | NOT IMPLEMENTED |
//! | Mobile adapters | NOT IMPLEMENTED |
//! | WASM DOM adapter | NOT IMPLEMENTED (backend pending) |

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// ── Themes & localization ─────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub foreground: String,
    pub background: String,
    pub accent: String,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            foreground: "#111111".into(),
            background: "#ffffff".into(),
            accent: "#0055ff".into(),
        }
    }
}

#[derive(Default, Clone)]
pub struct Catalog {
    strings: HashMap<String, String>,
    fallback: HashMap<String, String>,
}

impl Catalog {
    pub fn new(strings: HashMap<String, String>, fallback: HashMap<String, String>) -> Self {
        Self { strings, fallback }
    }

    /// Look up a key in the active locale, falling back to the default locale,
    /// then to the key itself (never silently blank).
    pub fn t(&self, key: &str) -> String {
        self.strings
            .get(key)
            .cloned()
            .or_else(|| self.fallback.get(key).cloned())
            .unwrap_or_else(|| key.to_owned())
    }
}

// ── Component model ───────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Layout {
    Column,
    Row,
    Stack,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub kind: String,
    pub props: HashMap<String, String>,
    pub children: Vec<Node>,
    pub layout: Option<Layout>,
    pub role: Option<String>,
    pub label: Option<String>,
    pub text: Option<String>,
}

impl Node {
    pub fn new(kind: &str) -> Self {
        Self {
            kind: kind.to_owned(),
            props: HashMap::new(),
            children: Vec::new(),
            layout: None,
            role: None,
            label: None,
            text: None,
        }
    }

    pub fn prop(mut self, key: &str, value: &str) -> Self {
        self.props.insert(key.to_owned(), value.to_owned());
        self
    }

    pub fn child(mut self, node: Node) -> Self {
        self.children.push(node);
        self
    }

    pub fn layout(mut self, layout: Layout) -> Self {
        self.layout = Some(layout);
        self
    }

    pub fn role(mut self, role: &str) -> Self {
        self.role = Some(role.to_owned());
        self
    }

    pub fn aria_label(mut self, label: &str) -> Self {
        self.label = Some(label.to_owned());
        self
    }

    pub fn text(mut self, text: &str) -> Self {
        self.text = Some(text.to_owned());
        self
    }
}

// ── Reactive state ────────────────────────────────────────────────────

#[derive(Clone, Default)]
pub struct Store {
    inner: Arc<Mutex<HashMap<String, String>>>,
    listeners: Arc<Mutex<Vec<StoreListener>>>,
}

pub type StoreListener = Arc<dyn Fn(&str, &str) + Send + Sync>;

impl Store {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, key: &str) -> Option<String> {
        self.inner.lock().unwrap().get(key).cloned()
    }

    /// Set a key and notify listeners (re-render trigger).
    pub fn set(&self, key: &str, value: &str) {
        self.inner
            .lock()
            .unwrap()
            .insert(key.to_owned(), value.to_owned());
        let listeners = self.listeners.lock().unwrap();
        for l in listeners.iter() {
            l(key, value);
        }
    }

    pub fn subscribe(&self, listener: StoreListener) {
        self.listeners.lock().unwrap().push(listener);
    }
}

// ── Navigation ────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct NavStack {
    stack: Vec<String>,
}

impl NavStack {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, route: &str) {
        self.stack.push(route.to_owned());
    }

    pub fn pop(&mut self) -> Option<String> {
        if self.stack.len() > 1 {
            self.stack.pop()
        } else {
            None
        }
    }

    pub fn replace(&mut self, route: &str) {
        self.stack.pop();
        self.stack.push(route.to_owned());
    }

    pub fn current(&self) -> Option<&str> {
        self.stack.last().map(|s| s.as_str())
    }

    pub fn depth(&self) -> usize {
        self.stack.len()
    }
}

// ── Forms ─────────────────────────────────────────────────────────────

pub struct Form {
    pub values: HashMap<String, String>,
}

impl Form {
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
        }
    }

    pub fn bind(&mut self, field: &str, value: &str) {
        self.values.insert(field.to_owned(), value.to_owned());
    }

    /// Required non-empty fields plus simple "email contains @" rule.
    pub fn validate(&self, required: &[&str]) -> Result<(), Vec<String>> {
        let mut errs = Vec::new();
        for f in required {
            match self.values.get(*f) {
                None => errs.push(format!("missing '{f}'")),
                Some(v) if v.trim().is_empty() => errs.push(format!("'{f}' is empty")),
                _ => {}
            }
        }
        if let Some(email) = self.values.get("email") {
            if !email.contains('@') {
                errs.push("'email' must contain '@'".to_owned());
            }
        }
        if errs.is_empty() {
            Ok(())
        } else {
            Err(errs)
        }
    }
}

impl Default for Form {
    fn default() -> Self {
        Self::new()
    }
}

// ── HTML adapter (real, tested) ───────────────────────────────────────

/// Render a component tree to an HTML string with theme colors and a11y attrs.
pub fn render_html(node: &Node, theme: &Theme, catalog: &Catalog) -> String {
    let mut out = String::new();
    render_node(node, theme, catalog, &mut out);
    out
}

fn render_node(node: &Node, theme: &Theme, catalog: &Catalog, out: &mut String) {
    let role_attr = node
        .role
        .as_ref()
        .map(|r| format!(" role=\"{r}\""))
        .unwrap_or_default();
    let label_attr = node
        .label
        .as_ref()
        .map(|l| format!(" aria-label=\"{l}\""))
        .unwrap_or_default();
    let tag = match node.kind.as_str() {
        "view" | "column" => "div",
        "row" => "div",
        "text" => "span",
        "button" => "button",
        "input" => "input",
        "form" => "form",
        "list" => "ul",
        "item" => "li",
        other => other,
    };
    let style = node.layout.as_ref().map(|l| {
        let display = match l {
            Layout::Column => "flex;flex-direction:column",
            Layout::Row => "flex;flex-direction:row",
            Layout::Stack => "grid",
        };
        format!(
            " style=\"display:{display};color:{};background:{}\"",
            theme.foreground, theme.background
        )
    });
    let style = style.unwrap_or_default();
    out.push_str(&format!("<{tag}{role_attr}{label_attr}{style}>"));
    if let Some(text) = &node.text {
        // Localize keys that exist in the catalog; otherwise render verbatim.
        let resolved = catalog.t(text);
        out.push_str(&crate::security::encode_html(&resolved));
    }
    for child in &node.children {
        render_node(child, theme, catalog, out);
    }
    if tag != "input" {
        out.push_str(&format!("</{tag}>"));
    }
}

// ── Event dispatch ────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum UiEvent {
    Click(String),
    Input(String, String),
    Submit(String),
}

/// Route an event to a handler by node id; handlers registered per id.
pub struct Dispatcher {
    handlers: HashMap<String, Arc<dyn Fn(UiEvent) + Send + Sync>>,
}

impl Dispatcher {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    pub fn on(&mut self, id: &str, handler: Arc<dyn Fn(UiEvent) + Send + Sync>) {
        self.handlers.insert(id.to_owned(), handler);
    }

    pub fn dispatch(&self, id: &str, event: UiEvent) -> bool {
        match self.handlers.get(id) {
            Some(h) => {
                h(event);
                true
            }
            None => false,
        }
    }
}

impl Default for Dispatcher {
    fn default() -> Self {
        Self::new()
    }
}
