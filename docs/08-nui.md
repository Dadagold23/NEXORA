# NUI — Universal UI (P18)

NUI is a renderer-independent component model. Only targets with a passing integration test are marked IMPLEMENTED.

```rust
use nxl_compiler::nui::*;
let tree = Node::new("view")
    .layout(Layout::Column)
    .role("main")
    .aria_label("app")
    .child(Node::new("text").text("greeting"))
    .child(Node::new("button").role("button").aria_label("save").text("Save"));
let html = render_html(&tree, &Theme::default(), &catalog);
```

- `Catalog::t("key")` resolves active locale → fallback locale → key itself (no silent blanks)
- `Store::set` notifies subscribers → listeners re-render
- `Dispatcher::dispatch(id, UiEvent)` routes events by node id
- `NavStack::push/pop/replace`, root protected
- `Form::bind` + `validate(required)`

Adapters: **HTML** — IMPLEMENTED and integration-tested (`nui_tests.rs::component_tree_renders_html_with_accessibility`). Desktop/mobile/WASM-DOM adapters: NOT IMPLEMENTED. Animations: DESIGNED, not implemented.
