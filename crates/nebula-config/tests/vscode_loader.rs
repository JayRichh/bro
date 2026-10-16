//! The shared VS Code theme loader (`nebula_config::vscode`) read against
//! folders shaped the way VS Code 1.141 lays them out: built-ins under
//! `<install>/<commit>/resources/app/extensions`, the user's extensions in
//! `~/.vscode/extensions` with `extensions.json` and `.obsolete` beside
//! them, and themes that `include` one another.
//!
//! Static folders under `tests/fixtures/vscode` stand where a case reads
//! better as a file (`atom-one-dark`, the built-in `theme-defaults`
//! chain, a `settings.json`); a temp dir stands where a case needs an
//! absolute path, a mutation or a file's age.

use nebula_config::vscode::*;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

// ---- fixtures and helpers -------------------------------------------------

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("vscode")
}

/// The fixture install's built-in extensions: a folder that ends
/// `resources/app/extensions`, which is how `themes_in` knows VS Code's own.
fn fixture_builtin() -> PathBuf {
    fixtures()
        .join("install")
        .join("resources")
        .join("app")
        .join("extensions")
}

/// A VS Code whose user folders and built-ins are the fixtures'.
fn fixture_install() -> Install {
    Install {
        name: "VS Code",
        settings: fixtures().join("User").join("settings.json"),
        extensions: fixtures().join("extensions"),
        builtin: Some(fixture_builtin()),
    }
}

/// A VS Code that lives in `dir`: nothing there yet, and no built-ins.
fn install_in(dir: &Path) -> Install {
    Install {
        name: "VS Code",
        settings: dir.join("User").join("settings.json"),
        extensions: dir.join("extensions"),
        builtin: None,
    }
}

/// The `atom-one-dark` fixture's theme file (akamud's: no `include`, no
/// `terminal.*` keys).
fn atom_one_dark() -> PathBuf {
    fixtures()
        .join("extensions")
        .join("akamud.vscode-theme-onedark-2.3.0")
        .join("themes")
        .join("OneDark.json")
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn write_json(path: &Path, value: &Value) {
    write(path, &value.to_string());
}

/// The extension folder `folder` in `dir`, its `package.json` as given.
fn extension_with(dir: &Path, folder: &str, package: Value) -> PathBuf {
    let ext = dir.join(folder);
    write_json(&ext.join("package.json"), &package);
    ext
}

/// The extension folder `folder` in `dir`, contributing `themes`.
fn extension(dir: &Path, folder: &str, themes: Value) -> PathBuf {
    extension_with(
        dir,
        folder,
        json!({ "name": folder, "contributes": { "themes": themes } }),
    )
}

/// Empty folders `names` in `dir`.
fn folders(dir: &Path, names: &[&str]) {
    for name in names {
        fs::create_dir_all(dir.join(name)).unwrap();
    }
}

fn leaf(path: &Path) -> String {
    path.file_name().unwrap().to_string_lossy().into_owned()
}

fn dir_names(found: &[PathBuf]) -> Vec<String> {
    found.iter().map(|p| leaf(p)).collect()
}

fn labels(themes: &[ThemeFile]) -> Vec<&str> {
    themes.iter().map(|t| t.label.as_str()).collect()
}

/// A path as the `path` of a VS Code URI writes it: `/c:/Users/Jay/...`
/// on Windows (forward slashes, a slash before the drive), the path itself
/// elsewhere.
fn uri_path(path: &Path) -> String {
    let text = path.to_string_lossy().into_owned();
    if cfg!(windows) {
        let mut text = text.replace(std::path::MAIN_SEPARATOR, "/");
        text[..1].make_ascii_lowercase();
        format!("/{text}")
    } else {
        text
    }
}

fn get<'a>(data: &'a ThemeData, key: &str) -> Option<&'a str> {
    data.colors.get(key).map(String::as_str)
}

fn with_colours(pairs: &[(&str, &str)]) -> ThemeData {
    ThemeData {
        colors: pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        ..ThemeData::default()
    }
}

fn with_tokens(pairs: &[(&str, &str)]) -> ThemeData {
    ThemeData {
        tokens: pairs
            .iter()
            .map(|(s, c)| (s.to_string(), c.to_string()))
            .collect(),
        ..ThemeData::default()
    }
}

/// `settings` whose only colour customization is the block `key`.
fn scoped(key: &str, colours: Value) -> Value {
    let mut custom = serde_json::Map::new();
    custom.insert(key.to_string(), colours);
    json!({ "workbench.colorCustomizations": custom })
}

/// Does the customization key `key` (`[Atom One Dark]`, as written in
/// settings.json) reach a theme called any of `names`?
fn scope_hits(key: &str, names: &[&str]) -> bool {
    let mut data = with_colours(&[]);
    customize(
        &mut data,
        &scoped(key, json!({ "focusBorder": "#123456" })),
        names,
    );
    data.colors.contains_key("focusBorder")
}

/// Set `path`'s modification time to `secs` after the epoch.
fn touch(path: &Path, secs: u64) {
    let file = fs::OpenOptions::new().write(true).open(path).unwrap();
    file.set_modified(UNIX_EPOCH + Duration::from_secs(secs))
        .unwrap();
}

/// `<root>/<commit>/resources/app/extensions`, made; with a `product.json`
/// of that age when one is given. Returns the extensions folder.
fn commit_folder(root: &Path, commit: &str, product_json_secs: Option<u64>) -> PathBuf {
    let app = root.join(commit).join("resources").join("app");
    fs::create_dir_all(app.join("extensions")).unwrap();
    if let Some(secs) = product_json_secs {
        let product = app.join("product.json");
        fs::write(&product, "{}").unwrap();
        touch(&product, secs);
    }
    app.join("extensions")
}

/// `json` with spaces around it, `len` bytes in all: still the same JSON,
/// so a read that fails is the size cap and not the parser.
fn padded_to(json: &str, len: u64) -> String {
    let pad = usize::try_from(len).unwrap() - json.len();
    format!("{}{json}{}", " ".repeat(pad / 2), " ".repeat(pad - pad / 2))
}

// ---- 1. where the built-in extensions are ---------------------------------

/// The flat layout: `<root>/resources/app/extensions`.
#[test]
fn a_flat_install_keeps_its_builtins_under_resources_app_extensions() {
    let root = tempfile::tempdir().unwrap();
    let ext = root.path().join("resources").join("app").join("extensions");
    fs::create_dir_all(&ext).unwrap();
    assert_eq!(builtin_extensions(root.path()), Some(ext));
}

/// macOS (and any root that already is `resources/app`) has `extensions`
/// directly under it.
#[test]
fn a_root_that_is_resources_app_already_keeps_them_under_extensions() {
    let root = tempfile::tempdir().unwrap();
    let ext = root.path().join("extensions");
    fs::create_dir_all(&ext).unwrap();
    assert_eq!(builtin_extensions(root.path()), Some(ext));
}

/// The versioned layout updates leave behind:
/// `<root>/<commit>/resources/app/extensions`, beside folders that are
/// not a VS Code (`bin`, `locales`).
#[test]
fn a_versioned_install_keeps_its_builtins_under_its_commit_folder() {
    let root = tempfile::tempdir().unwrap();
    folders(root.path(), &["bin", "locales"]);
    let ext = commit_folder(root.path(), "1a2b3c4d5e", None);
    assert_eq!(builtin_extensions(root.path()), Some(ext));
}

/// Two commit folders: the one whose `resources/app/product.json` is the
/// newer. The names run both ways round so the folder name (and the order
/// the OS lists them in) is not what decides.
#[test]
fn the_commit_folder_with_the_newest_product_json_is_the_one_taken() {
    for (newer, older) in [("aaaaaaaaaa", "zzzzzzzzzz"), ("zzzzzzzzzz", "aaaaaaaaaa")] {
        let root = tempfile::tempdir().unwrap();
        commit_folder(root.path(), older, Some(1_700_000_000));
        let wanted = commit_folder(root.path(), newer, Some(1_800_000_000));
        assert_eq!(
            builtin_extensions(root.path()),
            Some(wanted),
            "{newer} is newer than {older}"
        );
    }
}

/// A commit folder with no `product.json` to age it is the last pick.
#[test]
fn a_commit_folder_with_no_product_json_loses_to_one_that_has_it() {
    let root = tempfile::tempdir().unwrap();
    commit_folder(root.path(), "zzzzzzzzzz", None);
    let wanted = commit_folder(root.path(), "aaaaaaaaaa", Some(1_700_000_000));
    assert_eq!(builtin_extensions(root.path()), Some(wanted));
}

/// Nothing to find: an empty root, a root that is not there, and a root
/// whose folders hold no `resources/app/extensions`.
#[test]
fn a_root_with_no_extensions_folder_anywhere_has_no_builtins() {
    let root = tempfile::tempdir().unwrap();
    assert_eq!(builtin_extensions(root.path()), None, "empty");
    assert_eq!(
        builtin_extensions(&root.path().join("missing")),
        None,
        "not there"
    );
    folders(root.path(), &["bin", "1a2b3c4d5e/resources/app"]);
    assert_eq!(builtin_extensions(root.path()), None, "no extensions");
}

// ---- 2. the themes an extension contributes -------------------------------

/// The built-in `theme-defaults` as VS Code ships it: `%key%` labels
/// resolved from `package.nls.json` (plain string values), ids kept as
/// they are, and the extension's own `displayName` resolved too.
#[test]
fn a_percent_key_label_is_looked_up_in_package_nls_json() {
    let themes = themes_in(&[fixture_builtin()]);
    assert_eq!(labels(&themes), ["Dark Modern", "Light Modern", "Dark+"]);
    let ids: Vec<Option<&str>> = themes.iter().map(|t| t.id.as_deref()).collect();
    assert_eq!(
        ids,
        [
            Some("Dark Modern"),
            Some("Light Modern"),
            Some("Default Dark+")
        ]
    );
    assert!(themes
        .iter()
        .all(|t| t.builtin && t.extension == "Default Themes"));
}

/// The other spelling of an nls value, `{ "message": ..., "comment": [...] }`,
/// and a `%` that does not wrap a whole key.
#[test]
fn the_message_and_comment_spelling_of_an_nls_value_is_read() {
    let dir = tempfile::tempdir().unwrap();
    let ext = extension(
        dir.path(),
        "pub.themes-1.0.0",
        json!([
            { "label": "%night%", "path": "./night.json" },
            { "label": "%day%", "path": "./day.json" },
            { "label": "100% Dark", "path": "./dark.json" }
        ]),
    );
    write_json(
        &ext.join("package.nls.json"),
        &json!({
            "night": { "message": "Midnight", "comment": ["Name of the theme", "Not translated"] },
            "day": "Noon"
        }),
    );
    let themes = themes_in(&[dir.path().to_path_buf()]);
    assert_eq!(labels(&themes), ["Midnight", "Noon", "100% Dark"]);
}

/// The extension's name for a theme picker: its `displayName` (a `%key%`
/// looked up), else its package `name`.
#[test]
fn the_extension_is_named_by_its_display_name_else_its_package_name() {
    let dir = tempfile::tempdir().unwrap();
    let one = json!([{ "label": "One", "path": "./one.json" }]);
    let ext = extension_with(
        dir.path(),
        "a.one-1.0.0",
        json!({ "name": "one", "displayName": "%displayName%", "contributes": { "themes": one } }),
    );
    write_json(
        &ext.join("package.nls.json"),
        &json!({ "displayName": "Theme One" }),
    );
    extension_with(
        dir.path(),
        "b.two-1.0.0",
        json!({ "name": "two", "displayName": "Theme Two", "contributes": { "themes": one } }),
    );
    extension_with(
        dir.path(),
        "c.three-1.0.0",
        json!({ "name": "three", "contributes": { "themes": one } }),
    );
    let themes = themes_in(&[dir.path().to_path_buf()]);
    let named: Vec<&str> = themes.iter().map(|t| t.extension.as_str()).collect();
    assert_eq!(named, ["Theme One", "Theme Two", "three"]);
}

/// The id is what `workbench.colorTheme` stores, so it is looked for first
/// across every theme: a theme listed earlier whose *label* is the name
/// does not beat the one whose *id* is.
#[test]
fn a_theme_is_found_by_id_before_label() {
    let dir = tempfile::tempdir().unwrap();
    extension(
        dir.path(),
        "pub.themes-1.0.0",
        json!([
            { "id": "Baz", "label": "Bar", "path": "./by_label.json" },
            { "id": "Bar", "label": "Other", "path": "./by_id.json" }
        ]),
    );
    let dirs = [dir.path().to_path_buf()];
    let found = |name: &str| find_theme(name, &dirs).map(|t| leaf(&t.path));
    assert_eq!(found("Bar").as_deref(), Some("by_id.json"));
    assert_eq!(found("Baz").as_deref(), Some("by_label.json"), "an id");
    assert_eq!(found("Other").as_deref(), Some("by_id.json"), "a label");
    assert_eq!(found("Nope"), None);
}

/// Settings are matched without regard to case or the spaces around.
#[test]
fn a_theme_is_found_whatever_the_case_or_the_spaces_around_its_name() {
    let dir = tempfile::tempdir().unwrap();
    extension(
        dir.path(),
        "pub.themes-1.0.0",
        json!([{ "label": "Atom One Dark", "path": "./a.json" }]),
    );
    let dirs = [dir.path().to_path_buf()];
    for name in ["atom one dark", "ATOM ONE DARK", "  Atom One Dark  "] {
        let found = find_theme(name, &dirs).map(|t| t.label);
        assert_eq!(found.as_deref(), Some("Atom One Dark"), "{name:?}");
    }
}

/// Newer releases ship `Dark Modern` where the settings default still says
/// `Default Dark Modern`: both defaults find the id, and a theme that
/// really is called `Default ...` is taken as written first.
#[test]
fn default_dark_modern_finds_the_dark_modern_id() {
    let dir = tempfile::tempdir().unwrap();
    extension(
        dir.path(),
        "vscode.theme-defaults-1.0.0",
        json!([
            { "id": "Dark Modern", "label": "Dark Modern", "path": "./dm.json" },
            { "id": "Light Modern", "label": "Light Modern", "path": "./lm.json" }
        ]),
    );
    let dirs = [dir.path().to_path_buf()];
    let id = |name: &str| find_theme(name, &dirs).and_then(|t| t.id);
    assert_eq!(id(DEFAULT_DARK).as_deref(), Some("Dark Modern"));
    assert_eq!(id(DEFAULT_LIGHT).as_deref(), Some("Light Modern"));
    assert_eq!(id("Default Missing Theme"), None);

    let old = tempfile::tempdir().unwrap();
    extension(
        old.path(),
        "pub.old-1.0.0",
        json!([{ "label": "Default Dark Modern", "path": "./old.json" }]),
    );
    let both = [old.path().to_path_buf(), dir.path().to_path_buf()];
    let found = find_theme("Default Dark Modern", &both).unwrap();
    assert_eq!(leaf(&found.path), "old.json");
}

/// A user's `~/.vscode/extensions` is listed before the install's
/// `resources/app/extensions` (the one place `builtin` is true), and where
/// both have a theme of the label it is the user's that is found.
#[test]
fn the_users_extension_wins_over_a_builtin_of_the_same_label() {
    let root = tempfile::tempdir().unwrap();
    let user = root.path().join("user");
    let builtin = root
        .path()
        .join("code")
        .join("resources")
        .join("app")
        .join("extensions");
    extension(
        &user,
        "me.mine-1.0.0",
        json!([{ "label": "Dark Modern", "uiTheme": "vs", "path": "./mine.json" }]),
    );
    extension(
        &builtin,
        "theme-defaults",
        json!([{ "label": "Dark Modern", "uiTheme": "vs-dark", "path": "./theirs.json" }]),
    );
    let dirs = [user, builtin];

    let all: Vec<(String, bool)> = themes_in(&dirs)
        .iter()
        .map(|t| (leaf(&t.path), t.builtin))
        .collect();
    assert_eq!(
        all,
        [
            ("mine.json".to_string(), false),
            ("theirs.json".to_string(), true)
        ]
    );

    let found = find_theme("Dark Modern", &dirs).unwrap();
    assert_eq!(leaf(&found.path), "mine.json");
    assert!(!found.builtin);
    assert!(found.light, "it is the user's file, so its uiTheme");

    // The order of `dirs` is the whole of it.
    let flipped = [dirs[1].clone(), dirs[0].clone()];
    let found = find_theme("Dark Modern", &flipped).unwrap();
    assert_eq!(leaf(&found.path), "theirs.json");
    assert!(found.builtin);
}

/// The same, when both carry the same *id*.
#[test]
fn the_users_extension_wins_over_a_builtin_of_the_same_id() {
    let root = tempfile::tempdir().unwrap();
    let user = root.path().join("user");
    let builtin = root
        .path()
        .join("code")
        .join("resources")
        .join("app")
        .join("extensions");
    extension(
        &user,
        "me.mine-1.0.0",
        json!([{ "id": "Dark Modern", "label": "My Dark Modern", "path": "./mine.json" }]),
    );
    extension(
        &builtin,
        "theme-defaults",
        json!([{ "id": "Dark Modern", "label": "Dark Modern", "path": "./theirs.json" }]),
    );
    let found = find_theme("Dark Modern", &[user, builtin]).unwrap();
    assert_eq!(leaf(&found.path), "mine.json");
    assert!(!found.builtin);
}

/// `ThemeFile.light` is the `uiTheme`: `vs` and `hc-light` are light,
/// `vs-dark` and `hc-black` dark, and none at all is dark.
#[test]
fn a_theme_is_light_by_its_ui_theme() {
    let dir = tempfile::tempdir().unwrap();
    extension(
        dir.path(),
        "pub.themes-1.0.0",
        json!([
            { "label": "Plain", "uiTheme": "vs", "path": "./a.json" },
            { "label": "Dark", "uiTheme": "vs-dark", "path": "./b.json" },
            { "label": "HC Dark", "uiTheme": "hc-black", "path": "./c.json" },
            { "label": "HC Light", "uiTheme": "hc-light", "path": "./d.json" },
            { "label": "Unsaid", "path": "./e.json" }
        ]),
    );
    let themes = themes_in(&[dir.path().to_path_buf()]);
    let light: Vec<(&str, bool)> = themes.iter().map(|t| (t.label.as_str(), t.light)).collect();
    assert_eq!(
        light,
        [
            ("Plain", true),
            ("Dark", false),
            ("HC Dark", false),
            ("HC Light", true),
            ("Unsaid", false)
        ]
    );
}

/// An entry with no `path` names no file: it is skipped, its neighbours
/// are not.
#[test]
fn a_theme_entry_with_no_path_is_skipped() {
    let dir = tempfile::tempdir().unwrap();
    extension(
        dir.path(),
        "pub.themes-1.0.0",
        json!([
            { "label": "No Path", "uiTheme": "vs" },
            { "label": "Has Path", "path": "./has.json" }
        ]),
    );
    let themes = themes_in(&[dir.path().to_path_buf()]);
    assert_eq!(labels(&themes), ["Has Path"]);
}

/// A theme with no label is called by its id; one with neither (or a
/// blank label) has no name to be found by and is skipped.
#[test]
fn a_theme_with_no_label_is_named_by_its_id_and_one_with_neither_is_skipped() {
    let dir = tempfile::tempdir().unwrap();
    extension(
        dir.path(),
        "pub.themes-1.0.0",
        json!([
            { "id": "Only Id", "path": "./a.json" },
            { "path": "./b.json" },
            { "label": "   ", "path": "./c.json" },
            { "label": "Kept", "path": "./d.json" }
        ]),
    );
    let themes = themes_in(&[dir.path().to_path_buf()]);
    assert_eq!(labels(&themes), ["Only Id", "Kept"]);
}

/// What `workbench.colorTheme` stores, and what `[...]` customization
/// scopes match: the id, else the label.
#[test]
fn the_settings_id_of_a_theme_is_its_id_else_its_label() {
    let dir = tempfile::tempdir().unwrap();
    extension(
        dir.path(),
        "pub.themes-1.0.0",
        json!([
            { "id": "Default Dark+", "label": "Dark+", "path": "./a.json" },
            { "label": "Atom One Dark", "path": "./b.json" }
        ]),
    );
    let themes = themes_in(&[dir.path().to_path_buf()]);
    assert_eq!(themes[0].settings_id(), "Default Dark+");
    assert_eq!(themes[1].settings_id(), "Atom One Dark");
}

/// The `atom-one-dark` fixture as the user's extension: no id, a
/// `vs-dark` theme, not a built-in.
#[test]
fn the_fixture_extensions_folder_contributes_atom_one_dark() {
    let themes = themes_in(&[fixtures().join("extensions")]);
    assert_eq!(themes.len(), 1);
    let theme = &themes[0];
    assert_eq!(theme.label, "Atom One Dark");
    assert_eq!(theme.id, None);
    assert_eq!(theme.extension, "Atom One Dark Theme");
    assert!(!theme.light && !theme.builtin);
    assert_eq!(leaf(&theme.path), "OneDark.json");
}

// ---- 3. which extension folders count -------------------------------------

/// `extensions.json` is VS Code's own record of what is installed: only
/// the folders it lists count (a stale one beside them does not), found by
/// `relativeLocation`; an entry whose folder is gone is dropped. The
/// result is sorted by name.
#[test]
fn extensions_json_decides_which_folders_are_installed() {
    let dir = tempfile::tempdir().unwrap();
    folders(
        dir.path(),
        &[
            "esbenp.prettier-vscode-11.0.0",
            "akamud.vscode-theme-onedark-2.3.0",
            "stale.ext-0.1.0",
        ],
    );
    write_json(
        &dir.path().join("extensions.json"),
        &json!([
            {
                "identifier": { "id": "esbenp.prettier-vscode" },
                "version": "11.0.0",
                "relativeLocation": "esbenp.prettier-vscode-11.0.0"
            },
            {
                "identifier": { "id": "akamud.vscode-theme-onedark" },
                "version": "2.3.0",
                "relativeLocation": "akamud.vscode-theme-onedark-2.3.0"
            },
            {
                "identifier": { "id": "gone.ext" },
                "version": "1.0.0",
                "relativeLocation": "gone.ext-1.0.0"
            }
        ]),
    );
    assert_eq!(
        dir_names(&extension_folders(dir.path())),
        [
            "akamud.vscode-theme-onedark-2.3.0",
            "esbenp.prettier-vscode-11.0.0"
        ]
    );
}

/// With no `relativeLocation` an entry's `location` says where the folder
/// is: a URI `path` (on Windows `/c:/Users/...`, its leading slash
/// stripped) or an `fsPath`.
#[test]
fn extensions_json_falls_back_to_the_location_when_it_has_no_relative_location() {
    let dir = tempfile::tempdir().unwrap();
    folders(
        dir.path(),
        &[
            "akamud.vscode-theme-onedark-2.3.0",
            "dracula-theme.theme-dracula-2.25.1",
            "unlisted.ext-1.0.0",
        ],
    );
    let onedark = uri_path(&dir.path().join("akamud.vscode-theme-onedark-2.3.0"));
    let dracula = dir
        .path()
        .join("dracula-theme.theme-dracula-2.25.1")
        .to_string_lossy()
        .into_owned();
    write_json(
        &dir.path().join("extensions.json"),
        &json!([
            {
                "identifier": { "id": "akamud.vscode-theme-onedark" },
                "version": "2.3.0",
                "location": { "$mid": 1, "path": onedark, "scheme": "file" }
            },
            {
                "identifier": { "id": "dracula-theme.theme-dracula" },
                "version": "2.25.1",
                "location": { "$mid": 1, "fsPath": dracula, "path": "/not/the/one", "scheme": "file" }
            }
        ]),
    );
    // Sorted here by name: the two paths are spelt differently (a URI's
    // `c:/...`, an fsPath's `C:\...`), so how they sort as paths is not
    // what this case is about.
    let mut found = dir_names(&extension_folders(dir.path()));
    found.sort();
    assert_eq!(
        found,
        [
            "akamud.vscode-theme-onedark-2.3.0",
            "dracula-theme.theme-dracula-2.25.1"
        ]
    );
}

/// `.obsolete` (`{"<folder>": true}`) marks folders VS Code is done with,
/// and they are dropped even where `extensions.json` still lists them.
#[test]
fn obsolete_extensions_are_dropped_when_extensions_json_lists_them() {
    let dir = tempfile::tempdir().unwrap();
    folders(
        dir.path(),
        &[
            "saoudrizwan.claude-dev-4.1.23",
            "akamud.vscode-theme-onedark-2.3.0",
        ],
    );
    write_json(
        &dir.path().join("extensions.json"),
        &json!([
            { "relativeLocation": "saoudrizwan.claude-dev-4.1.23" },
            { "relativeLocation": "akamud.vscode-theme-onedark-2.3.0" }
        ]),
    );
    write_json(
        &dir.path().join(".obsolete"),
        &json!({ "saoudrizwan.claude-dev-4.1.23": true }),
    );
    assert_eq!(
        dir_names(&extension_folders(dir.path())),
        ["akamud.vscode-theme-onedark-2.3.0"]
    );
}

/// ...and where there is no `extensions.json` and the folders are scanned.
#[test]
fn obsolete_extensions_are_dropped_when_the_folders_are_scanned() {
    let dir = tempfile::tempdir().unwrap();
    folders(
        dir.path(),
        &[
            "saoudrizwan.claude-dev-4.1.23",
            "akamud.vscode-theme-onedark-2.3.0",
        ],
    );
    write_json(
        &dir.path().join(".obsolete"),
        &json!({ "saoudrizwan.claude-dev-4.1.23": true }),
    );
    assert_eq!(
        dir_names(&extension_folders(dir.path())),
        ["akamud.vscode-theme-onedark-2.3.0"]
    );
}

/// Without `extensions.json`, two versions of an extension side by side
/// keep the higher one by number, not by text: 1.10.0 over 1.9.0.
#[test]
fn without_extensions_json_the_higher_version_of_each_extension_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    folders(
        dir.path(),
        &["pub.ext-1.9.0", "pub.ext-1.10.0", "pub.solo-0.3.0"],
    );
    assert_eq!(
        dir_names(&extension_folders(dir.path())),
        ["pub.ext-1.10.0", "pub.solo-0.3.0"]
    );
}

/// A platform suffix (`-win32-x64`) does not hide the version from the
/// comparison, nor an unsuffixed older build outrank a suffixed newer one.
#[test]
fn a_platform_suffix_is_tolerated_when_versions_are_compared() {
    let dir = tempfile::tempdir().unwrap();
    folders(
        dir.path(),
        &[
            "ms.native-1.9.0-win32-x64",
            "ms.native-1.10.0-win32-x64",
            "ms.native-1.0.0",
            "ms.solo-1.0.241-win32-x64",
        ],
    );
    assert_eq!(
        dir_names(&extension_folders(dir.path())),
        ["ms.native-1.10.0-win32-x64", "ms.solo-1.0.241-win32-x64"]
    );
}

/// Dot-folders (`.git`, a test host's) and loose files are not extensions.
#[test]
fn dot_folders_and_loose_files_are_not_extensions() {
    let dir = tempfile::tempdir().unwrap();
    folders(dir.path(), &[".git", ".vscode-test", "pub.ext-1.0.0"]);
    write(&dir.path().join("notes.txt"), "not an extension");
    write(&dir.path().join(".obsolete"), "{}");
    assert_eq!(dir_names(&extension_folders(dir.path())), ["pub.ext-1.0.0"]);
}

/// An `extensions.json` that is not JSON is no record: the folders are
/// scanned as if it were not there. A folder that is not there lists none.
#[test]
fn a_broken_extensions_json_falls_back_to_the_folders() {
    let dir = tempfile::tempdir().unwrap();
    folders(dir.path(), &["a.one-1.0.0", "b.two-1.0.0"]);
    write(&dir.path().join("extensions.json"), "[ this is not json");
    assert_eq!(
        dir_names(&extension_folders(dir.path())),
        ["a.one-1.0.0", "b.two-1.0.0"]
    );
    assert!(extension_folders(&dir.path().join("missing")).is_empty());
}

// ---- 4. loading a theme file ----------------------------------------------

/// The built-in `dark_modern` -> `dark_plus` -> `dark_vs` chain: the base
/// first and each child over it, in the colours and in the tokens, and the
/// base's `type` surviving a child that has none.
#[test]
fn the_include_chain_merges_base_first_with_the_child_winning() {
    let themes = fixture_builtin().join("theme-defaults").join("themes");
    let data = load_theme(&themes.join("dark_modern.json")).unwrap();

    assert_eq!(
        get(&data, "editor.background"),
        Some("#1F1F1F"),
        "modern over vs"
    );
    assert_eq!(
        get(&data, "editor.foreground"),
        Some("#CCCCCC"),
        "modern over vs"
    );
    assert_eq!(
        get(&data, "sideBar.background"),
        Some("#181818"),
        "modern over plus over vs"
    );
    assert_eq!(
        get(&data, "editor.inactiveSelectionBackground"),
        Some("#3A3D41"),
        "vs only"
    );
    assert_eq!(
        get(&data, "list.deemphasizedForeground"),
        Some("#8C8C8C"),
        "plus only"
    );

    let scopes: Vec<&str> = data.tokens.iter().map(|(s, _)| s.as_str()).collect();
    assert_eq!(
        scopes,
        [
            "comment",
            "keyword",
            "string",
            "entity.name.function",
            "support.function",
            "keyword.control"
        ]
    );
    assert_eq!(data.token(&["comment"]), Some(Rgba::rgb(0x6A, 0x99, 0x55)));
    assert_eq!(data.token(&["keyword"]), Some(Rgba::rgb(0x56, 0x9C, 0xD6)));
    assert_eq!(
        data.token(&["keyword.control"]),
        Some(Rgba::rgb(0xC5, 0x86, 0xC0))
    );
    assert_eq!(
        data.token(&["entity.name.function"]),
        Some(Rgba::rgb(0xDC, 0xDC, 0xAA))
    );

    assert_eq!(data.kind.as_deref(), Some("dark"));
    assert_eq!(data.kind_light(), Some(false));
}

/// A child's token for a scope its base also styles comes after the base's,
/// so it is the one that wins.
#[test]
fn a_childs_token_for_a_scope_its_base_styles_wins() {
    let dir = tempfile::tempdir().unwrap();
    write_json(
        &dir.path().join("base.json"),
        &json!({ "tokenColors": [
            { "scope": "keyword", "settings": { "foreground": "#111111" } },
            { "scope": "string", "settings": { "foreground": "#222222" } }
        ] }),
    );
    let child = dir.path().join("child.json");
    write_json(
        &child,
        &json!({ "include": "./base.json", "tokenColors": [
            { "scope": "keyword", "settings": { "foreground": "#333333" } }
        ] }),
    );
    let data = load_theme(&child).unwrap();
    assert_eq!(data.token(&["keyword"]), Some(Rgba::rgb(0x33, 0x33, 0x33)));
    assert_eq!(data.token(&["string"]), Some(Rgba::rgb(0x22, 0x22, 0x22)));
}

/// A theme that `include`s itself is cut off where the chain gets too deep
/// to be real, not followed forever: the load returns, with its colours.
#[test]
fn a_theme_that_includes_itself_terminates() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("loop.json");
    write_json(
        &path,
        &json!({ "include": "./loop.json", "colors": { "editor.background": "#112233" } }),
    );
    let data = load_theme(&path).expect("a loop is cut short, not a failure");
    assert_eq!(get(&data, "editor.background"), Some("#112233"));
}

/// Two themes that include each other terminate too, and the one that was
/// asked for is the one that wins.
#[test]
fn two_themes_that_include_each_other_terminate_too() {
    let dir = tempfile::tempdir().unwrap();
    write_json(
        &dir.path().join("a.json"),
        &json!({ "include": "./b.json", "colors": { "x": "#111111" } }),
    );
    write_json(
        &dir.path().join("b.json"),
        &json!({ "include": "./a.json", "colors": { "x": "#333333", "y": "#222222" } }),
    );
    let data = load_theme(&dir.path().join("a.json")).expect("terminates");
    assert_eq!(get(&data, "x"), Some("#111111"));
    assert_eq!(get(&data, "y"), Some("#222222"));
}

/// An `include` that is not there does not take the file's own colours
/// with it.
#[test]
fn a_missing_include_still_gives_the_files_own_colours() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("child.json");
    write_json(
        &path,
        &json!({ "include": "./not-there.json", "colors": { "editor.background": "#445566" } }),
    );
    let data = load_theme(&path).expect("the file itself is fine");
    assert_eq!(get(&data, "editor.background"), Some("#445566"));
}

/// The file asked for is another matter: missing, or not JSON, is None.
#[test]
fn a_theme_file_that_is_missing_or_not_json_gives_none() {
    let dir = tempfile::tempdir().unwrap();
    assert!(load_theme(&dir.path().join("nope.json")).is_none());
    let broken = dir.path().join("broken.json");
    write(&broken, "{ \"colors\": ");
    assert!(load_theme(&broken).is_none());
}

/// A theme's own `type` lands in `ThemeData.kind` and `kind_light` reads
/// it (in any case); none says nothing.
#[test]
fn a_theme_type_lands_in_kind_and_says_light_or_dark() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.json");
    for (kind, light) in [
        ("light", true),
        ("Light", true),
        ("hc-light", true),
        ("vs", true),
        ("dark", false),
        ("vs-dark", false),
        ("hc", false),
        ("hc-black", false),
    ] {
        write_json(&path, &json!({ "type": kind, "colors": {} }));
        let data = load_theme(&path).unwrap();
        assert_eq!(data.kind.as_deref(), Some(kind));
        assert_eq!(data.kind_light(), Some(light), "{kind}");
    }
    write_json(&path, &json!({ "colors": {} }));
    let data = load_theme(&path).unwrap();
    assert_eq!(data.kind, None);
    assert_eq!(data.kind_light(), None);
}

/// A child's own `type` replaces the one it includes; one it lacks leaves
/// the base's.
#[test]
fn a_childs_own_type_replaces_the_one_it_includes() {
    let dir = tempfile::tempdir().unwrap();
    write_json(&dir.path().join("base.json"), &json!({ "type": "dark" }));
    write_json(
        &dir.path().join("light.json"),
        &json!({ "include": "./base.json", "type": "light" }),
    );
    write_json(
        &dir.path().join("untyped.json"),
        &json!({ "include": "./base.json" }),
    );
    let light = load_theme(&dir.path().join("light.json")).unwrap();
    assert_eq!(light.kind_light(), Some(true));
    let untyped = load_theme(&dir.path().join("untyped.json")).unwrap();
    assert_eq!(untyped.kind_light(), Some(false));
}

/// `tokenColors` can be a path to a TextMate `.tmTheme` instead of a list.
/// That is not read, and it does not fail the colours beside it.
#[test]
fn token_colors_given_as_a_tmtheme_path_are_skipped_without_failing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.json");
    write_json(
        &path,
        &json!({ "colors": { "editor.background": "#000000" }, "tokenColors": "./legacy.tmTheme" }),
    );
    let data = load_theme(&path).expect("the colours are still worth having");
    assert_eq!(get(&data, "editor.background"), Some("#000000"));
    assert!(data.tokens.is_empty());
}

/// A token's `scope` is one scope, a comma-separated list or an array
/// (trimmed); an entry with no scope or no foreground styles nothing.
#[test]
fn token_scopes_are_read_as_a_string_a_comma_list_or_an_array() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tokens.json");
    write_json(
        &path,
        &json!({ "tokenColors": [
            { "scope": "a, b ,c", "settings": { "foreground": "#111111" } },
            { "scope": [" d ", "e", 7], "settings": { "foreground": "#222222" } },
            { "name": "no scope", "settings": { "foreground": "#333333" } },
            { "scope": "f", "settings": { "fontStyle": "bold" } },
            { "scope": "g", "settings": { "foreground": 5 } }
        ] }),
    );
    let data = load_theme(&path).unwrap();
    let tokens: Vec<(&str, &str)> = data
        .tokens
        .iter()
        .map(|(s, c)| (s.as_str(), c.as_str()))
        .collect();
    assert_eq!(
        tokens,
        [
            ("a", "#111111"),
            ("b", "#111111"),
            ("c", "#111111"),
            ("d", "#222222"),
            ("e", "#222222")
        ]
    );
}

/// A `colors` value that is not a string is not a colour.
#[test]
fn colours_that_are_not_strings_are_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("odd.json");
    write_json(
        &path,
        &json!({ "colors": { "a": "#111111", "b": null, "c": 3, "d": ["#222222"] } }),
    );
    let data = load_theme(&path).unwrap();
    assert_eq!(data.colors.len(), 1);
    assert_eq!(get(&data, "a"), Some("#111111"));
}

/// The reusable `atom-one-dark` fixture read whole: its workbench colours,
/// its tokens (the `comment` array spelling too) and the broader scopes
/// they cover, and no `terminal.*` keys at all.
#[test]
fn the_atom_one_dark_fixture_loads_its_colours_and_tokens() {
    let data = load_theme(&atom_one_dark()).unwrap();
    assert_eq!(data.kind, None, "the file has no type");
    assert_eq!(data.colors.len(), 8);
    assert!(data.colors.keys().all(|k| !k.starts_with("terminal.")));

    let token = |scope: &str| data.token(&[scope]);
    assert_eq!(token("comment"), Some(Rgba::rgb(0x5C, 0x63, 0x70)));
    assert_eq!(
        token("punctuation.definition.comment"),
        Some(Rgba::rgb(0x5C, 0x63, 0x70))
    );
    assert_eq!(token("keyword"), Some(Rgba::rgb(0xC6, 0x78, 0xDD)));
    assert_eq!(token("string"), Some(Rgba::rgb(0x98, 0xC3, 0x79)));
    assert_eq!(token("constant.numeric"), Some(Rgba::rgb(0xD1, 0x9A, 0x66)));
    assert_eq!(
        token("entity.name.function"),
        Some(Rgba::rgb(0x61, 0xAF, 0xEF))
    );
    assert_eq!(token("entity.name.type"), Some(Rgba::rgb(0xE5, 0xC0, 0x7B)));
    assert_eq!(token("entity.name.tag"), Some(Rgba::rgb(0xE0, 0x6C, 0x75)));
    assert_eq!(token("support"), Some(Rgba::rgb(0x56, 0xB6, 0xC2)));
    // Broader entries cover the scopes under them.
    assert_eq!(token("keyword.control.flow"), token("keyword"));
    assert_eq!(token("support.function.builtin"), token("support"));
    assert_eq!(token("variable"), None);
}

/// `ThemeData::color` parses what VS Code writes, alpha and all, and
/// gives None for a key the theme lacks or a value that is not hex.
#[test]
fn a_colour_is_read_with_its_alpha() {
    let data = load_theme(&atom_one_dark()).unwrap();
    assert_eq!(
        data.color("editor.background"),
        Some(Rgba::rgb(0x28, 0x2C, 0x34))
    );
    let hover = data.color("list.hoverBackground").unwrap();
    assert_eq!(
        hover,
        Rgba {
            r: 0x2C,
            g: 0x31,
            b: 0x3A,
            a: 0x66
        }
    );
    assert!(!hover.opaque());
    assert_eq!(
        data.color("diffEditor.insertedTextBackground"),
        Some(Rgba {
            r: 0x00,
            g: 0x80,
            b: 0x9B,
            a: 0x33
        })
    );
    assert_eq!(data.color("terminal.ansiRed"), None, "not in the file");
    assert_eq!(with_colours(&[("a", "red")]).color("a"), None, "not hex");
}

/// An exact scope is the first look, even when a broader entry comes
/// after it in the file.
#[test]
fn a_token_is_found_by_its_exact_scope_before_a_broader_one() {
    let theme = with_tokens(&[("keyword.control", "#AA0000"), ("keyword", "#0000AA")]);
    assert_eq!(
        theme.token(&["keyword.control"]),
        Some(Rgba::rgb(0xAA, 0, 0))
    );
    assert_eq!(theme.token(&["keyword"]), Some(Rgba::rgb(0, 0, 0xAA)));
}

/// With no entry for a scope, a broader one it starts with (whole dotted
/// segments only) stands for it.
#[test]
fn a_scope_with_no_entry_takes_a_broader_one_it_starts_with() {
    let theme = with_tokens(&[("keyword", "#0000AA")]);
    assert_eq!(
        theme.token(&["keyword.operator.new"]),
        Some(Rgba::rgb(0, 0, 0xAA))
    );
    assert_eq!(theme.token(&["keywords"]), None, "not a dotted prefix");
    let theme = with_tokens(&[("key", "#111111")]);
    assert_eq!(theme.token(&["keyword"]), None, "half a segment");
}

/// Two entries for a scope: the later one wins, exact or broader.
#[test]
fn the_later_of_two_entries_for_a_scope_wins() {
    let theme = with_tokens(&[("string", "#111111"), ("string", "#222222")]);
    assert_eq!(theme.token(&["string"]), Some(Rgba::rgb(0x22, 0x22, 0x22)));
    assert_eq!(
        theme.token(&["string.quoted"]),
        Some(Rgba::rgb(0x22, 0x22, 0x22))
    );
}

/// `scopes` are tried in order and the first the theme styles at all wins,
/// by a broader entry too, ahead of an exact one for a scope after it.
#[test]
fn the_first_scope_asked_for_that_the_theme_styles_wins() {
    let theme = with_tokens(&[("entity.name", "#111111"), ("support.function", "#222222")]);
    assert_eq!(
        theme.token(&["support.function", "entity.name.function"]),
        Some(Rgba::rgb(0x22, 0x22, 0x22))
    );
    assert_eq!(
        theme.token(&["entity.name.function", "support.function"]),
        Some(Rgba::rgb(0x11, 0x11, 0x11))
    );
    assert_eq!(theme.token(&["variable", "constant"]), None);
}

/// A foreground that is not a hex colour styles nothing.
#[test]
fn a_token_whose_foreground_is_not_a_colour_gives_none() {
    let theme = with_tokens(&[("string", "red")]);
    assert_eq!(theme.token(&["string"]), None);
}

/// LOOKS LIKE A BUG in `ThemeData::token`, and fails today. VS Code styles
/// a token with its most specific matching scope, and a later entry only
/// breaks a tie between two of the same specificity. `token` instead takes
/// the *latest* entry that is a prefix, so a broad `keyword` listed after
/// `keyword.control` (the order plenty of real themes use) overrides it for
/// `keyword.control.flow`.
#[test]
fn the_most_specific_matching_scope_wins_whatever_its_place_in_the_file() {
    let theme = with_tokens(&[("keyword.control", "#AA0000"), ("keyword", "#0000AA")]);
    assert_eq!(
        theme.token(&["keyword.control.flow"]),
        Some(Rgba::rgb(0xAA, 0, 0)),
        "keyword.control is the closer match, though keyword comes later"
    );
}

// ---- 5. the user's colour customizations ----------------------------------

fn base_theme() -> ThemeData {
    with_colours(&[
        ("editor.background", "#282C34"),
        ("editor.foreground", "#ABB2BF"),
    ])
}

/// Keys outside any `[...]` apply to every theme, adding keys the theme
/// lacked as well as replacing the ones it has.
#[test]
fn unscoped_customizations_apply_to_every_theme() {
    let settings = json!({ "workbench.colorCustomizations": {
        "editor.background": "#000000",
        "focusBorder": "#FF0000"
    } });
    let mut data = base_theme();
    customize(&mut data, &settings, &["Any Theme"]);
    assert_eq!(get(&data, "editor.background"), Some("#000000"));
    assert_eq!(get(&data, "focusBorder"), Some("#FF0000"));
    assert_eq!(get(&data, "editor.foreground"), Some("#ABB2BF"));
}

/// No customizations, or something that is not a table of them, leaves the
/// theme as it was.
#[test]
fn settings_with_no_customizations_leave_the_theme_alone() {
    for settings in [
        json!({}),
        json!(null),
        json!({ "workbench.colorCustomizations": "oops" }),
        json!({ "workbench.colorCustomizations": ["#000000"] }),
    ] {
        let mut data = base_theme();
        customize(&mut data, &settings, &["Atom One Dark"]);
        assert_eq!(data.colors, base_theme().colors, "{settings}");
    }
}

/// A `[Atom One Dark]` block applies to that theme and no other, and over
/// the unscoped key for the same colour. (In the settings' key order the
/// block sorts before `editor.background`, so a plain top-to-bottom apply
/// would let the unscoped key win.)
#[test]
fn a_scoped_block_applies_only_to_its_theme_and_beats_the_unscoped_key() {
    let settings = json!({ "workbench.colorCustomizations": {
        "editor.background": "#000000",
        "[Atom One Dark]": { "editor.background": "#111111", "focusBorder": "#222222" }
    } });

    let mut data = base_theme();
    customize(&mut data, &settings, &["Atom One Dark"]);
    assert_eq!(get(&data, "editor.background"), Some("#111111"));
    assert_eq!(get(&data, "focusBorder"), Some("#222222"));
    assert_eq!(get(&data, "editor.foreground"), Some("#ABB2BF"));

    let mut data = base_theme();
    customize(&mut data, &settings, &["Monokai"]);
    assert_eq!(get(&data, "editor.background"), Some("#000000"));
    assert_eq!(get(&data, "focusBorder"), None);
}

/// `*` in a scope matches any run of characters, over the whole name.
#[test]
fn a_scope_may_glob_with_a_star() {
    assert!(scope_hits("[*Light*]", &["Default Light Modern"]));
    assert!(!scope_hits("[*Light*]", &["Atom One Dark"]));
    assert!(scope_hits("[Atom*]", &["Atom One Dark"]));
    assert!(scope_hits("[*Dark]", &["Atom One Dark"]));
    assert!(!scope_hits("[*Dark]", &["Dark Modern"]));
    assert!(scope_hits("[Solarized*Dark]", &["Solarized Dark"]));
    assert!(
        !scope_hits("[Atom]", &["Atom One Dark"]),
        "no star, the whole name"
    );
}

/// A scope is matched ignoring case.
#[test]
fn a_scope_matches_ignoring_case() {
    assert!(scope_hits("[atom one dark]", &["Atom One Dark"]));
    assert!(scope_hits("[ATOM*]", &["Atom One Dark"]));
}

/// `[Atom One Dark][Monokai]` in one key names either theme.
#[test]
fn a_key_naming_several_themes_matches_any_of_them() {
    let key = "[Atom One Dark][Monokai]";
    assert!(scope_hits(key, &["Atom One Dark"]));
    assert!(scope_hits(key, &["Monokai"]));
    assert!(!scope_hits(key, &["Dracula"]));
}

/// A theme is named by its settings id and its label: either matches.
#[test]
fn a_scope_matches_either_of_the_names_it_is_given() {
    let names = ["Default Dark+", "Dark+"];
    assert!(scope_hits("[Default Dark+]", &names));
    assert!(scope_hits("[Dark+]", &names));
    assert!(!scope_hits("[Light+]", &names));
    assert!(!scope_hits("[Dark+]", &[]), "no name, no match");
}

/// Customizations that are not strings are not colours: unscoped, scoped,
/// and a whole scoped block that is a string instead of a table.
#[test]
fn customizations_that_are_not_strings_are_ignored() {
    let settings = json!({ "workbench.colorCustomizations": {
        "editor.background": 5,
        "focusBorder": null,
        "sideBar.background": ["#000000"],
        "list.hoverBackground": { "nested": "#000000" },
        "[Atom One Dark]": { "editor.foreground": 7, "tab.activeBackground": "#123456" },
        "[Atom One Dark][Dracula]": "#FFFFFF"
    } });
    let mut data = base_theme();
    customize(&mut data, &settings, &["Atom One Dark"]);
    assert_eq!(get(&data, "editor.background"), Some("#282C34"));
    assert_eq!(get(&data, "editor.foreground"), Some("#ABB2BF"));
    assert_eq!(get(&data, "tab.activeBackground"), Some("#123456"));
    for key in ["focusBorder", "sideBar.background", "list.hoverBackground"] {
        assert_eq!(get(&data, key), None, "{key}");
    }
}

// ---- 6. the theme VS Code shows --------------------------------------------

/// The fixture `settings.json` is JSONC (comments, trailing commas) and
/// reads; one that is not there does not. It names Atom One Dark, with a
/// customization the loaded theme carries.
#[test]
fn settings_json_is_read_with_its_comments_and_trailing_commas() {
    let vs = fixture_install();
    let settings = read_settings(&vs).expect("the fixture settings.json");
    assert_eq!(settings["workbench.colorTheme"], "Atom One Dark");
    assert_eq!(chosen_theme(&settings, None), "Atom One Dark");
    let shown = fonts(&settings);
    assert_eq!(shown.editor_family.as_deref(), Some("JetBrains Mono"));
    assert_eq!(shown.editor_size, Some(14.0));

    let loaded = current_in(&vs, &settings, None).unwrap();
    assert_eq!(loaded.file.label, "Atom One Dark");
    assert_eq!(
        loaded.data.color("editor.background"),
        Some(Rgba::rgb(0x1B, 0x1E, 0x24)),
        "the customization, over the file's #282C34"
    );
    assert_eq!(
        loaded.data.color("editor.foreground"),
        Some(Rgba::rgb(0xAB, 0xB2, 0xBF))
    );

    let gone = Install {
        settings: fixtures().join("User").join("nope.json"),
        ..fixture_install()
    };
    assert!(read_settings(&gone).is_none());
}

/// An install's themes are looked for in the user's extensions, then the
/// built-in ones (where it has them).
#[test]
fn the_install_looks_in_the_users_extensions_before_the_builtin_ones() {
    let vs = fixture_install();
    assert_eq!(
        vs.extension_dirs(),
        [vs.extensions.clone(), fixture_builtin()]
    );
    let dir = tempfile::tempdir().unwrap();
    let vs = install_in(dir.path());
    assert_eq!(vs.extension_dirs(), std::slice::from_ref(&vs.extensions));
}

/// `workbench.colorTheme` names the theme, a user's or a built-in, and the
/// OS's side does not matter while `window.autoDetectColorScheme` is off.
#[test]
fn the_current_theme_follows_workbench_color_theme() {
    let vs = fixture_install();
    let settings = json!({ "workbench.colorTheme": "Atom One Dark" });
    let loaded = current_in(&vs, &settings, Some(true)).unwrap();
    assert_eq!(loaded.file.label, "Atom One Dark");
    assert!(!loaded.file.builtin && !loaded.file.light);
    assert_eq!(
        loaded.data.color("editor.background"),
        Some(Rgba::rgb(0x28, 0x2C, 0x34))
    );

    let settings = json!({ "workbench.colorTheme": "Light Modern" });
    let loaded = current_in(&vs, &settings, Some(false)).unwrap();
    assert!(loaded.file.builtin && loaded.file.light);
    assert_eq!(loaded.data.kind_light(), Some(true));
}

/// The preferred dark and light themes are ignored unless auto-detect is
/// on, and then only where the OS's side is known.
#[test]
fn the_preferred_themes_are_ignored_unless_auto_detect_is_on() {
    let off = json!({
        "workbench.colorTheme": "Plain",
        "workbench.preferredDarkColorTheme": "Night",
        "workbench.preferredLightColorTheme": "Day"
    });
    assert_eq!(chosen_theme(&off, Some(false)), "Plain");
    assert_eq!(chosen_theme(&off, Some(true)), "Plain");

    let on = json!({
        "window.autoDetectColorScheme": true,
        "workbench.colorTheme": "Plain",
        "workbench.preferredDarkColorTheme": "Night",
        "workbench.preferredLightColorTheme": "Day"
    });
    assert_eq!(chosen_theme(&on, Some(false)), "Night");
    assert_eq!(chosen_theme(&on, Some(true)), "Day");
    assert_eq!(chosen_theme(&on, None), "Plain", "the OS is not known");
}

/// With auto-detect on, `os_light` picks which preferred theme loads.
#[test]
fn auto_detect_loads_the_preferred_theme_for_the_side_the_os_is_on() {
    let vs = fixture_install();
    let settings = json!({
        "window.autoDetectColorScheme": true,
        "workbench.colorTheme": "Dark Modern",
        "workbench.preferredDarkColorTheme": "Atom One Dark",
        "workbench.preferredLightColorTheme": "Light Modern"
    });
    let label = |os_light: Option<bool>| current_in(&vs, &settings, os_light).map(|l| l.file.label);
    assert_eq!(label(Some(true)).as_deref(), Some("Light Modern"));
    assert_eq!(label(Some(false)).as_deref(), Some("Atom One Dark"));
    assert_eq!(label(None).as_deref(), Some("Dark Modern"));
}

/// A settings file that names no theme shows VS Code's default, which the
/// built-in `Dark Modern` answers to; auto-detect with nothing preferred
/// shows the default of the OS's side.
#[test]
fn with_nothing_set_the_default_modern_theme_is_shown() {
    let vs = fixture_install();
    assert_eq!(chosen_theme(&json!({}), None), DEFAULT_DARK);
    let loaded = current_in(&vs, &json!({}), None).unwrap();
    assert_eq!(loaded.file.id.as_deref(), Some("Dark Modern"));

    let auto = json!({ "window.autoDetectColorScheme": true });
    assert_eq!(chosen_theme(&auto, Some(true)), DEFAULT_LIGHT);
    assert_eq!(chosen_theme(&auto, Some(false)), DEFAULT_DARK);
    let loaded = current_in(&vs, &auto, Some(true)).unwrap();
    assert_eq!(loaded.file.id.as_deref(), Some("Light Modern"));
}

/// The `Loaded` data is the file's colours with the user's customizations
/// over them: scoped over unscoped over the file, another theme's block
/// not at all.
#[test]
fn the_loaded_data_carries_the_users_customizations() {
    let vs = fixture_install();
    let settings = json!({
        "workbench.colorTheme": "Atom One Dark",
        "workbench.colorCustomizations": {
            "editor.background": "#101010",
            "focusBorder": "#FF00FF",
            "[Atom One Dark]": { "editor.background": "#202020", "sideBar.background": "#303030" },
            "[Monokai]": { "focusBorder": "#00FF00" }
        }
    });
    let loaded = current_in(&vs, &settings, None).unwrap();
    let data = &loaded.data;
    assert_eq!(get(data, "editor.background"), Some("#202020"), "scoped");
    assert_eq!(get(data, "sideBar.background"), Some("#303030"), "scoped");
    assert_eq!(
        get(data, "focusBorder"),
        Some("#FF00FF"),
        "unscoped, not Monokai's"
    );
    assert_eq!(
        get(data, "editor.foreground"),
        Some("#ABB2BF"),
        "the file's"
    );
}

/// `named_in` takes an id or a label, and a customization block names the
/// theme by either: `Default Dark+` is the id of `Dark+`.
#[test]
fn a_customization_scope_names_the_theme_by_its_id_or_its_label() {
    let vs = fixture_install();
    let settings = json!({ "workbench.colorCustomizations": {
        "[Default Dark+]": { "editor.background": "#111111" },
        "[Dark+]": { "sideBar.background": "#222222" },
        "[Light Modern]": { "focusBorder": "#333333" }
    } });
    for name in ["Default Dark+", "Dark+"] {
        let loaded = named_in(&vs, &settings, name).unwrap();
        assert_eq!(loaded.file.id.as_deref(), Some("Default Dark+"), "{name}");
        assert_eq!(
            get(&loaded.data, "editor.background"),
            Some("#111111"),
            "{name}"
        );
        assert_eq!(
            get(&loaded.data, "sideBar.background"),
            Some("#222222"),
            "{name}"
        );
        assert_eq!(
            get(&loaded.data, "focusBorder"),
            Some("#007FD4"),
            "{name}: its own"
        );
    }
}

/// A theme that is not installed is None, whichever way it is asked for.
#[test]
fn an_unknown_theme_gives_none() {
    let vs = fixture_install();
    let settings = json!({ "workbench.colorTheme": "No Such Theme" });
    assert!(current_in(&vs, &settings, None).is_none());
    assert!(named_in(&vs, &json!({}), "No Such Theme").is_none());
}

/// A theme listed by an extension whose file is not there is None too, not
/// a half-loaded theme.
#[test]
fn a_theme_whose_file_is_gone_gives_none() {
    let dir = tempfile::tempdir().unwrap();
    let vs = install_in(dir.path());
    extension(
        &vs.extensions,
        "pub.ghost-1.0.0",
        json!([{ "label": "Ghost", "path": "./themes/gone.json" }]),
    );
    assert!(
        find_theme("Ghost", &vs.extension_dirs()).is_some(),
        "it is listed"
    );
    assert!(named_in(&vs, &json!({}), "Ghost").is_none());
}

// ---- 7. the size cap -------------------------------------------------------

/// A `package.json` of exactly `MAX_FILE_BYTES` is read; one byte more is
/// not, and its themes are not seen.
#[test]
fn a_package_json_over_the_cap_is_not_read() {
    let package = json!({
        "name": "big",
        "contributes": { "themes": [{ "label": "Big", "path": "./big.json" }] }
    })
    .to_string();
    for (len, seen) in [(MAX_FILE_BYTES, 1), (MAX_FILE_BYTES + 1, 0)] {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join("pub.big-1.0.0").join("package.json"),
            &padded_to(&package, len),
        );
        let themes = themes_in(&[dir.path().to_path_buf()]);
        assert_eq!(themes.len(), seen, "a package.json of {len} bytes");
    }
}

/// A theme file of exactly `MAX_FILE_BYTES` loads; one byte more is None.
#[test]
fn a_theme_file_over_the_cap_is_not_read() {
    let theme = json!({ "colors": { "editor.background": "#111111" } }).to_string();
    for (len, loads) in [(MAX_FILE_BYTES, true), (MAX_FILE_BYTES + 1, false)] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.json");
        write(&path, &padded_to(&theme, len));
        assert_eq!(load_theme(&path).is_some(), loads, "a theme of {len} bytes");
    }
}

/// An included file over the cap is skipped like a missing one: the
/// including file still loads, without it.
#[test]
fn an_include_over_the_cap_is_skipped_like_a_missing_one() {
    let base = json!({ "colors": { "base.only": "#111111" } }).to_string();
    let child = json!({ "include": "./base.json", "colors": { "child.only": "#222222" } });
    for (len, has_base) in [(MAX_FILE_BYTES, true), (MAX_FILE_BYTES + 1, false)] {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("base.json"), &padded_to(&base, len));
        let path = dir.path().join("child.json");
        write_json(&path, &child);
        let data = load_theme(&path).expect("the child is small");
        assert_eq!(
            get(&data, "base.only").is_some(),
            has_base,
            "a base of {len} bytes"
        );
        assert_eq!(get(&data, "child.only"), Some("#222222"));
    }
}

/// `settings.json` over the cap is no settings.
#[test]
fn a_settings_json_over_the_cap_is_not_read() {
    let settings = json!({ "workbench.colorTheme": "Atom One Dark" }).to_string();
    for (len, reads) in [(MAX_FILE_BYTES, true), (MAX_FILE_BYTES + 1, false)] {
        let dir = tempfile::tempdir().unwrap();
        let vs = install_in(dir.path());
        write(&vs.settings, &padded_to(&settings, len));
        assert_eq!(
            read_settings(&vs).is_some(),
            reads,
            "settings of {len} bytes"
        );
    }
}

/// An `extensions.json` over the cap is no record, so the folders are
/// scanned and the one it would have left out is back.
#[test]
fn an_extensions_json_over_the_cap_is_not_read() {
    let list = json!([{ "relativeLocation": "a.one-1.0.0" }]).to_string();
    for (len, listed_only) in [(MAX_FILE_BYTES, true), (MAX_FILE_BYTES + 1, false)] {
        let dir = tempfile::tempdir().unwrap();
        folders(dir.path(), &["a.one-1.0.0", "b.two-1.0.0"]);
        write(&dir.path().join("extensions.json"), &padded_to(&list, len));
        let found = dir_names(&extension_folders(dir.path()));
        if listed_only {
            assert_eq!(found, ["a.one-1.0.0"], "a list of {len} bytes");
        } else {
            assert_eq!(
                found,
                ["a.one-1.0.0", "b.two-1.0.0"],
                "a list of {len} bytes"
            );
        }
    }
}

// ---- the window's palette from a fixture --------------------------------

/// Atom One Dark, as installed, maps onto every window token: its canvas
/// and ink, its syntax for the statuses, and Workshop's ember kept, its
/// accent being a blue focus ring rather than an attention colour.
#[test]
fn the_atom_one_dark_fixture_maps_to_a_whole_palette_that_keeps_ember() {
    let data = load_theme(&atom_one_dark()).unwrap();
    let p = nebula_config::look::from_vscode(&data, false, "Atom One Dark");
    assert!(p.valid());
    assert_eq!(p.tokens["color-bg"], "#282c34");
    assert_eq!(p.tokens["color-ember"], "#e08a35");
    assert_eq!(p.tokens["color-merged"], "#c678dd");
    assert_eq!(
        p.tokens["color-term-green"], "#0dbc79",
        "VS Code's own default"
    );
}
