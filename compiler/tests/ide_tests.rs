use nxl_compiler::ide::{Document, IdeModel};
use nxl_compiler::nls;

const SAMPLE: &str =
    "fn add(a: Int, b: Int) -> Int {\n    return a + b;\n}\n\nlet total: Int = add(1, 2);\n";

fn doc(text: &str) -> Document {
    Document {
        path: std::path::PathBuf::from("sample.nxl"),
        text: text.to_owned(),
    }
}

#[test]
fn diagnostics_match_nls() {
    let model = IdeModel::new();
    let document = doc("let x: Int = \"oops\";\n");
    assert_eq!(
        model.diagnostics(&document),
        nls::diagnostics("let x: Int = \"oops\";\n")
    );
    assert!(!model.diagnostics(&document).is_empty());
}

#[test]
fn outline_matches_nls_symbols() {
    let model = IdeModel::new();
    let document = doc(SAMPLE);
    let outline = model.outline(&document);
    let labels: Vec<&str> = outline.iter().map(|i| i.label.as_str()).collect();
    assert!(labels.contains(&"add"));
    assert!(labels.contains(&"total"));
}

#[test]
fn completions_match_nls() {
    let model = IdeModel::new();
    let document = doc(SAMPLE);
    let items = model.completions(&document);
    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    assert!(labels.contains(&"fn"));
    assert!(labels.contains(&"add"));
}

#[test]
fn hover_and_definition_delegate() {
    let model = IdeModel::new();
    let document = doc("let total: Int = 0;\n");
    assert!(model
        .hover(&document, 1, 5)
        .unwrap()
        .contains("variable total: Int"));
    assert!(model.definition(&doc(SAMPLE), "add").unwrap().0 == 1);
}

#[test]
fn format_normalises_via_shared_formatter() {
    let model = IdeModel::new();
    let document = doc("let x = 1;   \n\nlet y = 2;\t\n");
    let formatted = model.format(&document);
    assert_eq!(formatted, "let x = 1;\n\nlet y = 2;\n");
}

#[test]
fn capability_table_documents_shared_compiler() {
    let table = nxl_compiler::ide::ide_capability_table();
    assert!(table
        .iter()
        .any(|(k, v)| *k == "compiler" && v.contains("shared")));
    assert!(table.iter().any(|(_, v)| v.contains("nxl-compiler::nls")));
    assert!(table
        .iter()
        .any(|(k, v)| *k == "IDE shell" && v.contains("NOT IMPLEMENTED")));
}
