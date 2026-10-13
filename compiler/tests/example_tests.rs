use std::fs;
use std::path::{Path, PathBuf};

/// Verify every runnable example ships valid, parseable artefacts.
/// Uses CARGO_MANIFEST_DIR (=compiler/) to locate ../examples.
fn examples_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("examples")
}

#[test]
fn hello_example_parses_and_typechecks() {
    let path = examples_dir().join("hello").join("src").join("main.nxl");
    let text = fs::read_to_string(&path).unwrap();
    let sf = nxl_compiler::source::SourceFile::new(path.to_string_lossy().into_owned(), text);
    nxl_compiler::compile_to_nir(&sf).expect("hello example must compile to NIR");
}

#[test]
fn blog_example_migrations_apply_and_seed() {
    let ex = examples_dir();
    // Copy project to a writable temp location — do not touch the real example dir.
    let tmp = std::env::temp_dir().join(format!("nxl_ex_blog_{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();
    fs::copy(ex.join("blog").join("nxl.toml"), tmp.join("nxl.toml")).unwrap();
    copy_dir(&ex.join("blog").join("database"), &tmp.join("database"));

    let migrations = nxl_compiler::ndb::load_migrations(&tmp).unwrap();
    assert_eq!(migrations.len(), 1);
    assert!(migrations[0].up_sql.contains("CREATE TABLE posts"));
    assert!(migrations[0].down_sql.contains("DROP TABLE posts"));

    let ran = nxl_compiler::ndb::fresh(&tmp, true).unwrap();
    assert_eq!(ran.len(), 1);

    let conn = nxl_compiler::ndb::connect(&tmp).unwrap();
    let post_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM posts", [], |r| r.get(0))
        .unwrap();
    let comment_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM comments", [], |r| r.get(0))
        .unwrap();
    assert_eq!(post_count, 2);
    assert_eq!(comment_count, 1);

    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn blog_seeders_roll_back_cleanly_on_failure() {
    // A broken seeder must fail the seed transaction, not silently corrupt state.
    let tmp = std::env::temp_dir().join(format!("nxl_ex_blog_bad_{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(tmp.join("database").join("migrations")).unwrap();
    fs::create_dir_all(tmp.join("database").join("seeders")).unwrap();
    fs::write(
        tmp.join("nxl.toml"),
        "[package]\nname = \"t\"\n\n[database]\ndriver = \"sqlite\"\npath = \"database/database.sqlite\"\n",
    )
    .unwrap();
    fs::write(
        tmp.join("database").join("seeders").join("bad.sql"),
        "INSERT INTO nope VALUES (1);\n",
    )
    .unwrap();
    // blog example has no table for "nope", so the seed must fail hard
    let err = nxl_compiler::ndb::run_seeders(&tmp);
    assert!(err.is_err(), "bad seeder must surface an error");
    let _ = fs::remove_dir_all(&tmp);
}

fn copy_dir(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let e = entry.unwrap();
        let to = dst.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &to);
        } else {
            fs::copy(e.path(), &to).unwrap();
        }
    }
}
