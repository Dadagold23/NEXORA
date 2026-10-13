use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use nxl_compiler::nui;

#[test]
fn component_tree_renders_html_with_accessibility() {
    let tree = nui::Node::new("view")
        .layout(nui::Layout::Column)
        .role("main")
        .aria_label("app")
        .child(nui::Node::new("text").text("Hello"))
        .child(
            nui::Node::new("button")
                .role("button")
                .aria_label("save")
                .text("Save"),
        );
    let theme = nui::Theme::default();
    let catalog = nui::Catalog::new(HashMap::new(), HashMap::new());
    let html = nui::render_html(&tree, &theme, &catalog);
    assert!(html.contains("role=\"main\""), "{html}");
    assert!(html.contains("aria-label=\"save\""), "{html}");
    assert!(html.contains("flex-direction:column"), "{html}");
    assert!(html.contains("Hello"), "{html}");
}

#[test]
fn text_content_is_html_escaped_and_localized() {
    let mut es = HashMap::new();
    es.insert("greeting".to_owned(), "Hola <b>amigo</b>".to_owned());
    let catalog = nui::Catalog::new(es, HashMap::new());
    let node = nui::Node::new("text").text("greeting");
    let html = nui::render_html(&node, &nui::Theme::default(), &catalog);
    assert!(html.contains("Hola &lt;b&gt;amigo&lt;/b&gt;"), "{html}");
}

#[test]
fn store_notifies_render_listeners() {
    let store = nui::Store::new();
    let renders = Arc::new(AtomicUsize::new(0));
    let r2 = renders.clone();
    store.subscribe(Arc::new(move |_, _| {
        r2.fetch_add(1, Ordering::SeqCst);
    }));
    store.set("count", "1");
    store.set("count", "2");
    assert_eq!(renders.load(Ordering::SeqCst), 2);
    assert_eq!(store.get("count").as_deref(), Some("2"));
}

#[test]
fn navigation_stack_pushes_pops_and_protects_root() {
    let mut nav = nui::NavStack::new();
    nav.push("home");
    nav.push("details");
    assert_eq!(nav.current(), Some("details"));
    assert_eq!(nav.pop(), Some("details".to_owned()));
    assert_eq!(nav.current(), Some("home"));
    // root is protected
    assert_eq!(nav.pop(), None);
    nav.replace("settings");
    assert_eq!(nav.current(), Some("settings"));
    assert_eq!(nav.depth(), 1);
}

#[test]
fn form_validation_collects_errors() {
    let mut form = nui::Form::new();
    form.bind("name", "");
    form.bind("email", "not-an-email");
    let errs = form.validate(&["name", "email"]).unwrap_err();
    assert!(errs.iter().any(|e| e.contains("name")));
    assert!(errs.iter().any(|e| e.contains("@")));
    form.bind("name", "Ada");
    form.bind("email", "ada@example.com");
    assert!(form.validate(&["name", "email"]).is_ok());
}

#[test]
fn catalog_falls_back_to_default_locale() {
    let mut fr = HashMap::new();
    fr.insert("title".to_owned(), "Titre".to_owned());
    let mut en = HashMap::new();
    en.insert("title".to_owned(), "Title".to_owned());
    let catalog = nui::Catalog::new(fr, en);
    assert_eq!(catalog.t("title"), "Titre");
    // Missing in both: key itself, never blank
    assert_eq!(catalog.t("missing"), "missing");
}

#[test]
fn dispatcher_routes_events_by_node_id() {
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let s2 = seen.clone();
    let mut dispatcher = nui::Dispatcher::new();
    dispatcher.on(
        "save-btn",
        Arc::new(move |ev| {
            s2.lock().unwrap().push(format!("{ev:?}"));
        }),
    );
    assert!(dispatcher.dispatch("save-btn", nui::UiEvent::Click("save-btn".into())));
    assert!(!dispatcher.dispatch("unknown", nui::UiEvent::Click("x".into())));
    assert_eq!(seen.lock().unwrap().len(), 1);
}
