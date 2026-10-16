use super::*;

use super::detect::{
    exe_of, is_commit_folder, parse_product, version_cmp, visual_studio, UninstallEntry,
};
use super::dock::{Insets, Plan, Rect, Side};
use super::launch::{command, strips};
use serde::Deserialize;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

/// What this machine has, printed: a hand check, never run by default.
/// It only reads (the registry, vswhere, the window list, the icons into
/// a temp folder); nothing is launched or moved.
/// `cargo test -p nebula-core editors::tests::this_machine -- --ignored --nocapture`
#[test]
#[ignore]
fn this_machine() {
    let found = detect(&detect::RealProbe);
    for e in &found {
        println!(
            "{:<28} {:<14} {:<12} {:<10} {:?} {}",
            e.id,
            e.short,
            e.version,
            e.edition.as_deref().unwrap_or("-"),
            e.source,
            e.exe.display()
        );
    }
    let dir = tempfile::tempdir().unwrap();
    for e in &found {
        println!("icon {}: {:?}", e.id, icon(e, 32, dir.path()));
    }
    for w in windows(&found) {
        println!("window {} {} {:?}", w.editor, w.pid, w.title);
    }
}

// ---- fixtures ----

// Captured read-only on this machine on 2026-10-16: the vswhere output, VS Code
// 1.141.0's product.json trimmed to the keys the scan and the installer care
// about, and its uninstall key beside Zed Attack Proxy's (a negative case).
const VSWHERE_THIS_MACHINE: &str = include_str!("../../fixtures/editors/vswhere-this-machine.json");
const VSCODE_PRODUCT: &str = include_str!("../../fixtures/editors/vscode-1.141.0-product.json");
const UNINSTALL_THIS_MACHINE: &str =
    include_str!("../../fixtures/editors/uninstall-this-machine.json");

// Hand-written, in the shape of each app's real product.json: Insiders is a
// user install under a build-commit folder, VSCodium a system install with the
// flat resources\app layout, Cursor a flat user install, Windsurf is only
// ever found through its known folder.
const INSIDERS_PRODUCT: &str = include_str!("../../fixtures/editors/vscode-insiders-product.json");
const VSCODIUM_PRODUCT: &str = include_str!("../../fixtures/editors/vscodium-product.json");
const CURSOR_PRODUCT: &str = include_str!("../../fixtures/editors/cursor-product.json");
const WINDSURF_PRODUCT: &str = include_str!("../../fixtures/editors/windsurf-product.json");

// Hand-written vswhere output: every case `visual_studio` has a rule for.
const VSWHERE_HAND_WRITTEN: &str = include_str!("../../fixtures/editors/vswhere-hand-written.json");

const LOCAL: &str = r"C:\Users\Jay\AppData\Local";
const CODE_DIR: &str = r"C:\Users\Jay\AppData\Local\Programs\Microsoft VS Code";
const INSIDERS_DIR: &str = r"C:\Users\Jay\AppData\Local\Programs\Microsoft VS Code Insiders";
const CURSOR_DIR: &str = r"C:\Users\Jay\AppData\Local\Programs\cursor";
const VSCODIUM_DIR: &str = r"C:\Program Files\VSCodium";
const CODE_KEY: &str = "{771FD6B0-FA20-440A-A002-3B3BAC16DC50}_is1";

// The hand-written vswhere fixture's instances, by the id `visual_studio` gives.
const PRO_2022: &str = "vs-5f0a91c2";
const ENTERPRISE_2022: &str = "vs-8d41e7b3";
const PREVIEW_2022: &str = "vs-c07d2a19";
const COMMUNITY_2019: &str = "vs-1e6b3f84";
const ENTERPRISE_2026: &str = "vs-a93c05de";
const INSIDERS_2026: &str = "vs-b6d8f130";
const NO_CATALOG_2026: &str = "vs-7a15c4e9";
// Left out: Build Tools 2019, an incomplete Community 2022, a Professional 2019
// that isn't launchable.
const BUILD_TOOLS_2019: &str = "vs-3c92a7f0";
const INCOMPLETE_2022: &str = "vs-0badf00d";
const UNLAUNCHABLE_2019: &str = "vs-d2e8b475";

/// A path as every OS can compare it: forward slashes, no trailing one.
fn norm(p: impl AsRef<Path>) -> String {
    p.as_ref()
        .to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string()
}

fn local(sub: &str) -> String {
    format!(r"{LOCAL}\{sub}")
}

/// A product.json with just the keys the scan reads.
fn product(app: &str, short: &str, version: &str) -> String {
    format!(
        r#"{{"applicationName":"{app}","nameShort":"{short}","nameLong":"{short}","dataFolderName":".{app}","quality":"stable","version":"{version}"}}"#
    )
}

fn version_of(product: &str) -> String {
    serde_json::from_str::<serde_json::Value>(product)
        .ok()
        .and_then(|v| v["version"].as_str().map(str::to_string))
        .unwrap_or_default()
}

// ---- the fake machine ----

/// An in-memory machine: registry values, files and environment, all written
/// the Windows way and read back the same on every OS. Paths are compared
/// case-blind, as NTFS does.
#[derive(Default)]
struct FakeProbe {
    win: bool,
    keys: Vec<UninstallEntry>,
    app_path_values: Vec<(String, String)>,
    vars: Vec<(String, String)>,
    files: Vec<(String, String)>,
    vswhere_json: Option<String>,
}

impl FakeProbe {
    fn on_windows() -> Self {
        Self {
            win: true,
            ..Self::default()
        }
    }

    fn on_unix() -> Self {
        Self::default()
    }

    fn file(mut self, path: &str, text: &str) -> Self {
        self.files.push((norm(path), text.to_string()));
        self
    }

    fn exe_at(self, path: &str) -> Self {
        self.file(path, "")
    }

    fn key(mut self, machine: bool, key: &str, version: &str, location: &str, icon: &str) -> Self {
        self.keys.push(UninstallEntry {
            key: key.to_string(),
            machine,
            display_version: version.to_string(),
            install_location: location.replace('\\', "/"),
            display_icon: icon.replace('\\', "/"),
        });
        self
    }

    fn app_path_at(mut self, exe: &str, value: &str) -> Self {
        self.app_path_values
            .push((exe.to_ascii_lowercase(), value.replace('\\', "/")));
        self
    }

    fn env_var(mut self, name: &str, value: &str) -> Self {
        self.vars.push((name.to_string(), value.replace('\\', "/")));
        self
    }

    fn with_vswhere(mut self, json: &str) -> Self {
        self.vswhere_json = Some(json.to_string());
        self
    }

    /// `dir` holds `exe` and the flat `resources\app\product.json`.
    fn flat_install(self, dir: &str, exe: &str, product: &str) -> Self {
        self.exe_at(&format!(r"{dir}\{exe}"))
            .file(&format!(r"{dir}\resources\app\product.json"), product)
    }

    /// `dir` holds `exe`; the product.json is under the build-commit folder.
    fn versioned_install(self, dir: &str, commit: &str, exe: &str, product: &str) -> Self {
        self.exe_at(&format!(r"{dir}\{exe}")).file(
            &format!(r"{dir}\{commit}\resources\app\product.json"),
            product,
        )
    }

    /// The Inno uninstall key an install at `dir` leaves.
    fn registered(self, machine: bool, dir: &str, exe: &str, product: &str) -> Self {
        let name = dir.rsplit('\\').next().unwrap_or(dir);
        self.key(
            machine,
            &format!("{name}_is1"),
            &version_of(product),
            &format!(r"{dir}\"),
            &format!(r"{dir}\{exe}"),
        )
    }

    /// A flat user install under `%LOCALAPPDATA%\<sub>` with its key.
    fn user_install(self, sub: &str, exe: &str, product: &str) -> Self {
        let dir = local(sub);
        self.flat_install(&dir, exe, product)
            .registered(false, &dir, exe, product)
    }
}

impl Probe for FakeProbe {
    fn windows(&self) -> bool {
        self.win
    }

    fn uninstall_entries(&self) -> Vec<UninstallEntry> {
        self.keys.clone()
    }

    fn app_path(&self, exe: &str) -> Option<String> {
        let exe = exe.to_ascii_lowercase();
        self.app_path_values
            .iter()
            .find(|(name, _)| *name == exe)
            .map(|(_, value)| value.clone())
    }

    fn env(&self, var: &str) -> Option<String> {
        self.vars
            .iter()
            .find(|(name, _)| name == var)
            .map(|(_, value)| value.clone())
    }

    fn is_file(&self, path: &Path) -> bool {
        let path = norm(path);
        self.files
            .iter()
            .any(|(p, _)| p.eq_ignore_ascii_case(&path))
    }

    fn read(&self, path: &Path) -> Option<String> {
        let path = norm(path);
        self.files
            .iter()
            .find(|(p, _)| p.eq_ignore_ascii_case(&path))
            .map(|(_, text)| text.clone())
    }

    fn subdirs(&self, path: &Path) -> Vec<String> {
        let prefix = format!("{}/", norm(path));
        let mut out: Vec<String> = Vec::new();
        for (p, _) in &self.files {
            let Some(head) = p.get(..prefix.len()) else {
                continue;
            };
            if !head.eq_ignore_ascii_case(&prefix) {
                continue;
            }
            if let Some((dir, _)) = p[prefix.len()..].split_once('/') {
                if !out.iter().any(|d| d == dir) {
                    out.push(dir.to_string());
                }
            }
        }
        out
    }

    fn vswhere(&self) -> Option<String> {
        self.vswhere_json.clone()
    }
}

// ---- small builders and readers ----

fn ids(editors: &[Editor]) -> Vec<&str> {
    editors.iter().map(|e| e.id.as_str()).collect()
}

fn by_id<'a>(editors: &'a [Editor], id: &str) -> &'a Editor {
    editors
        .iter()
        .find(|e| e.id == id)
        .unwrap_or_else(|| panic!("no {id} among {:?}", ids(editors)))
}

fn editor(id: &str, family: Family, short: &str, version: &str) -> Editor {
    Editor {
        id: id.to_string(),
        family,
        short: short.to_string(),
        name: short.to_string(),
        version: version.to_string(),
        edition: None,
        year: None,
        prerelease: false,
        exe: PathBuf::from(format!("/opt/{id}/{id}")),
        dir: PathBuf::from(format!("/opt/{id}")),
        source: Source::Path,
        instance_id: None,
        application_name: None,
        name_short: None,
        data_folder_name: None,
    }
}

fn code_editor() -> Editor {
    editor("code", Family::Code, "VS Code", "1.141.0")
}

fn vs_editor(version: &str) -> Editor {
    editor(
        "vs-32c4cf50",
        Family::VisualStudio,
        "Visual Studio 2022",
        version,
    )
}

// ---- this machine ----

#[derive(Deserialize)]
struct RegKey {
    hive: String,
    key: String,
    #[serde(rename = "DisplayVersion")]
    display_version: String,
    #[serde(rename = "InstallLocation")]
    install_location: String,
    #[serde(rename = "DisplayIcon")]
    display_icon: String,
}

/// This machine as the captured fixtures describe it: the user install of VS
/// Code 1.141.0 under its build-commit folder, ZAP's HKLM key, Build Tools and
/// Community 2022 in vswhere.
fn this_machine_probe() -> FakeProbe {
    this_machine_with(true)
}

/// [`this_machine_probe`], with or without VS Code's product.json under the
/// commit folder: everything else on the machine is the same.
fn this_machine_with(product_json: bool) -> FakeProbe {
    let keys: Vec<RegKey> = serde_json::from_str(UNINSTALL_THIS_MACHINE).unwrap();
    let code_exe = format!(r"{CODE_DIR}\Code.exe");
    let mut probe = FakeProbe::on_windows()
        .env_var("LOCALAPPDATA", LOCAL)
        .env_var("ProgramFiles", r"C:\Program Files")
        .env_var(
            "PATH",
            &format!(r"{CODE_DIR}\bin;C:\Windows\system32;C:\Windows"),
        )
        .exe_at(&code_exe)
        .exe_at(&format!(r"{CODE_DIR}\unins000.exe"))
        .exe_at(&format!(r"{CODE_DIR}\bin\code.cmd"))
        .exe_at(r"C:\Program Files\ZAP\Zed Attack Proxy\ZAP.exe")
        .file(
            r"C:\Program Files\ZAP\Zed Attack Proxy\.install4j\installer.ico",
            "",
        )
        .with_vswhere(VSWHERE_THIS_MACHINE);
    if product_json {
        probe = probe.file(
            &format!(r"{CODE_DIR}\2a59476c9b\resources\app\product.json"),
            VSCODE_PRODUCT,
        );
    }
    for k in keys {
        probe = probe.key(
            k.hive == "HKLM",
            &k.key,
            &k.display_version,
            &k.install_location,
            &k.display_icon,
        );
    }
    probe
}

#[test]
fn this_machine_has_vs_code_and_visual_studio_and_nothing_else() {
    let found = detect(&this_machine_probe());
    assert_eq!(ids(&found), ["code", "vs-32c4cf50"]);

    let code = &found[0];
    assert_eq!(code.family, Family::Code);
    assert_eq!(code.short, "VS Code");
    assert_eq!(code.name, "Visual Studio Code");
    assert_eq!(code.version, "1.141.0");
    assert_eq!(code.edition.as_deref(), Some("stable"));
    assert!(!code.prerelease);
    assert_eq!(code.source, Source::UserInstall);
    assert_eq!(code.application_name.as_deref(), Some("code"));
    assert_eq!(code.name_short.as_deref(), Some("Code"));
    assert_eq!(code.data_folder_name.as_deref(), Some(".vscode"));
    assert_eq!(code.instance_id, None);
    assert_eq!(norm(&code.dir), norm(CODE_DIR));
    assert_eq!(norm(&code.exe), norm(format!(r"{CODE_DIR}\Code.exe")));

    let vs = &found[1];
    assert_eq!(vs.family, Family::VisualStudio);
    assert_eq!(vs.short, "Visual Studio 2022");
    assert_eq!(vs.name, "Visual Studio Community 2022");
    assert_eq!(vs.edition.as_deref(), Some("Community"));
    assert_eq!(vs.year.as_deref(), Some("2022"));
    assert_eq!(vs.version, "17.14.16");
    assert!(!vs.prerelease);
    assert_eq!(vs.source, Source::Vswhere);
    assert_eq!(vs.instance_id.as_deref(), Some("32c4cf50"));
    assert_eq!(
        norm(&vs.exe),
        "C:/Program Files/Microsoft Visual Studio/2022/Community/Common7/IDE/devenv.exe"
    );
}

#[test]
fn this_machines_build_tools_and_zap_are_not_editors() {
    let found = detect(&this_machine_probe());
    // Build Tools is vswhere instance 4b338933: its productPath is a .bat.
    assert!(!found.iter().any(|e| e.id == "vs-4b338933"));
    // ZAP's DisplayIcon is an .ico and its folder has no product.json.
    assert!(!found.iter().any(|e| norm(&e.dir).contains("ZAP")));
    assert!(!found.iter().any(|e| norm(&e.exe).contains("ZAP")));
}

#[test]
fn this_machines_vs_code_is_found_only_through_the_product_json_under_its_commit_folder() {
    // Code.exe, its uninstall key and `code.cmd` on PATH are all still there,
    // but the product.json that confirms them is not.
    let found = detect(&this_machine_with(false));
    assert_eq!(ids(&found), ["vs-32c4cf50"]);

    // The same file under a folder that isn't a build commit's is not looked at,
    // and none sits at the flat resources\app place either.
    let probe = this_machine_with(false).file(
        &format!(r"{CODE_DIR}\current\resources\app\product.json"),
        VSCODE_PRODUCT,
    );
    assert!(!is_commit_folder("current"));
    assert_eq!(ids(&detect(&probe)), ["vs-32c4cf50"]);

    // Under the commit folder it is VS Code again, with Code.exe at the root.
    let found = detect(&this_machine_with(true));
    assert_eq!(ids(&found), ["code", "vs-32c4cf50"]);
    assert_eq!(norm(found[0].exe.parent().unwrap()), norm(CODE_DIR));
}

// ---- builds mid-update ----

/// An update left two build folders (1.99.3 and 1.100.0); the uninstall key
/// says `display_version`.
fn two_builds(display_version: &str) -> Vec<Editor> {
    let old = product("code", "Code", "1.99.3");
    let new = product("code", "Code", "1.100.0");
    let probe = FakeProbe::on_windows()
        .exe_at(&format!(r"{CODE_DIR}\Code.exe"))
        .file(
            &format!(r"{CODE_DIR}\0a1b2c3d4e\resources\app\product.json"),
            &old,
        )
        .file(
            &format!(r"{CODE_DIR}\f5e6d7c8b9\resources\app\product.json"),
            &new,
        )
        .key(
            false,
            CODE_KEY,
            display_version,
            CODE_DIR,
            &format!(r"{CODE_DIR}\Code.exe"),
        );
    detect(&probe)
}

#[test]
fn mid_update_the_build_folder_the_uninstall_key_names_wins() {
    let found = two_builds("1.99.3");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].version, "1.99.3");

    let found = two_builds("1.100.0");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].version, "1.100.0");
}

#[test]
fn mid_update_with_no_display_version_the_newest_build_wins() {
    // 1.100.0 is newer than 1.99.3 even though "1.99.3" sorts after it as text.
    let found = two_builds("");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].version, "1.100.0");
}

#[test]
fn mid_update_a_display_version_that_names_no_build_falls_back_to_the_newest() {
    let found = two_builds("1.50.0");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].version, "1.100.0");
}

// ---- the forks ----

#[test]
fn flat_layout_forks_are_found_from_their_own_product_json() {
    let probe = FakeProbe::on_windows()
        .flat_install(VSCODIUM_DIR, "VSCodium.exe", VSCODIUM_PRODUCT)
        .key(
            true,
            "{88DA3577-054F-4CA1-8122-7D820494CFFB}_is1",
            "1.140.26280",
            r"C:\Program Files\VSCodium\",
            r"C:\Program Files\VSCodium\VSCodium.exe",
        )
        .flat_install(CURSOR_DIR, "Cursor.exe", CURSOR_PRODUCT)
        .key(
            false,
            "{DF6B21E3-78D9-4F0B-A3E5-2E4B5E1D8C47}_is1",
            "2.6.3",
            r"C:\Users\Jay\AppData\Local\Programs\cursor\",
            r#""C:\Users\Jay\AppData\Local\Programs\cursor\Cursor.exe",0"#,
        );
    let found = detect(&probe);
    assert_eq!(ids(&found), ["codium", "cursor"]);

    let codium = &found[0];
    assert_eq!(codium.short, "VSCodium");
    assert_eq!(codium.name, "VSCodium");
    assert_eq!(codium.version, "1.140.26280");
    assert_eq!(codium.edition.as_deref(), Some("stable"));
    assert_eq!(codium.source, Source::SystemInstall);
    assert_eq!(codium.name_short.as_deref(), Some("VSCodium"));
    assert_eq!(codium.data_folder_name.as_deref(), Some(".vscode-oss"));
    assert_eq!(
        norm(&codium.exe),
        norm(r"C:\Program Files\VSCodium\VSCodium.exe")
    );

    let cursor = &found[1];
    assert_eq!(cursor.short, "Cursor");
    assert_eq!(cursor.version, "2.6.3");
    assert_eq!(cursor.source, Source::UserInstall);
    assert_eq!(cursor.application_name.as_deref(), Some("cursor"));
    assert_eq!(cursor.data_folder_name.as_deref(), Some(".cursor"));
    assert_eq!(norm(&cursor.dir), norm(CURSOR_DIR));
}

#[test]
fn cursors_quoted_display_icon_with_an_index_names_its_exe() {
    let probe = FakeProbe::on_windows()
        .flat_install(CURSOR_DIR, "Cursor.exe", CURSOR_PRODUCT)
        // No InstallLocation: the folder comes from the exe in the icon.
        .key(
            false,
            "{DF6B21E3-78D9-4F0B-A3E5-2E4B5E1D8C47}_is1",
            "2.6.3",
            "",
            r#""C:\Users\Jay\AppData\Local\Programs\cursor\Cursor.exe",0"#,
        );
    let found = detect(&probe);
    assert_eq!(ids(&found), ["cursor"]);
    assert_eq!(
        norm(&found[0].exe),
        norm(format!(r"{CURSOR_DIR}\Cursor.exe"))
    );
    assert_eq!(norm(&found[0].dir), norm(CURSOR_DIR));
}

#[test]
fn vs_code_insiders_is_a_prerelease_under_its_own_commit_folder() {
    let probe = FakeProbe::on_windows().versioned_install(
        INSIDERS_DIR,
        "c4f8a27e90",
        "Code - Insiders.exe",
        INSIDERS_PRODUCT,
    );
    let probe = probe.registered(false, INSIDERS_DIR, "Code - Insiders.exe", INSIDERS_PRODUCT);
    let found = detect(&probe);
    assert_eq!(ids(&found), ["code-insiders"]);
    let insiders = &found[0];
    assert_eq!(insiders.short, "VS Code Insiders");
    assert_eq!(insiders.name, "Visual Studio Code - Insiders");
    assert_eq!(insiders.version, "1.142.0-insider");
    assert_eq!(insiders.edition.as_deref(), Some("insider"));
    assert!(insiders.prerelease);
    assert_eq!(insiders.name_short.as_deref(), Some("Code - Insiders"));
    assert_eq!(
        insiders.data_folder_name.as_deref(),
        Some(".vscode-insiders")
    );
    assert_eq!(
        norm(&insiders.exe),
        norm(format!(r"{INSIDERS_DIR}\Code - Insiders.exe"))
    );
}

#[test]
fn windsurf_is_found_through_its_known_folder_alone() {
    let dir = local(r"Programs\Windsurf");
    let probe = FakeProbe::on_windows()
        .env_var("LOCALAPPDATA", LOCAL)
        .flat_install(&dir, "Windsurf.exe", WINDSURF_PRODUCT);
    let found = detect(&probe);
    assert_eq!(ids(&found), ["windsurf"]);
    let windsurf = &found[0];
    assert_eq!(windsurf.short, "Windsurf");
    assert_eq!(windsurf.source, Source::KnownFolder);
    assert_eq!(windsurf.name_short.as_deref(), Some("Windsurf"));
    assert_eq!(windsurf.data_folder_name.as_deref(), Some(".windsurf"));
    assert_eq!(norm(&windsurf.exe), norm(format!(r"{dir}\Windsurf.exe")));
}

// ---- one editor, however it was reached ----

/// The user install's files and `code.cmd`, with nothing yet that points at them.
fn code_files() -> FakeProbe {
    FakeProbe::on_windows()
        .versioned_install(CODE_DIR, "2a59476c9b", "Code.exe", VSCODE_PRODUCT)
        .exe_at(&format!(r"{CODE_DIR}\bin\code.cmd"))
}

fn only_source(probe: FakeProbe) -> Source {
    let found = detect(&probe);
    assert_eq!(ids(&found), ["code"]);
    found[0].source
}

#[test]
fn one_exe_reached_four_ways_is_one_editor_from_the_uninstall_key() {
    let exe = format!(r"{CODE_DIR}\Code.exe");
    let probe = code_files()
        .key(false, CODE_KEY, "1.141.0", CODE_DIR, &exe)
        // App Paths often has the path in whatever case the installer typed.
        .app_path_at("Code.exe", &exe.to_lowercase())
        .env_var("LOCALAPPDATA", LOCAL)
        .env_var("PATH", &format!(r"{CODE_DIR}\bin;C:\Windows"));
    let found = detect(&probe);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].source, Source::UserInstall);
    assert_eq!(norm(&found[0].exe), norm(&exe));
}

#[test]
fn without_an_uninstall_key_app_paths_then_the_known_folder_then_path_name_the_source() {
    let exe = format!(r"{CODE_DIR}\Code.exe");
    let path = format!(r"{CODE_DIR}\bin;C:\Windows");

    let probe = code_files()
        .app_path_at("Code.exe", &exe)
        .env_var("LOCALAPPDATA", LOCAL)
        .env_var("PATH", &path);
    assert_eq!(only_source(probe), Source::AppPaths);

    let probe = code_files()
        .env_var("LOCALAPPDATA", LOCAL)
        .env_var("PATH", &path);
    assert_eq!(only_source(probe), Source::KnownFolder);

    let probe = code_files().env_var("PATH", &path);
    assert_eq!(only_source(probe), Source::Path);
}

// ---- order and words ----

fn forks(with_code: bool) -> Vec<Editor> {
    let kiro = product("kiro", "Kiro", "0.9.1");
    let foo = product("foo", "Foo IDE", "2.0.0");
    // Added in no particular order: the scan sorts them.
    let mut probe = FakeProbe::on_windows()
        .env_var("LOCALAPPDATA", LOCAL)
        .flat_install(
            &local(r"Programs\Windsurf"),
            "Windsurf.exe",
            WINDSURF_PRODUCT,
        )
        .user_install(r"Programs\Kiro", "Kiro.exe", &kiro)
        .user_install(r"Programs\cursor", "Cursor.exe", CURSOR_PRODUCT)
        .user_install(r"Programs\Foo IDE", "Foo IDE.exe", &foo)
        .flat_install(VSCODIUM_DIR, "VSCodium.exe", VSCODIUM_PRODUCT)
        .registered(true, VSCODIUM_DIR, "VSCodium.exe", VSCODIUM_PRODUCT)
        .versioned_install(
            INSIDERS_DIR,
            "c4f8a27e90",
            "Code - Insiders.exe",
            INSIDERS_PRODUCT,
        )
        .registered(false, INSIDERS_DIR, "Code - Insiders.exe", INSIDERS_PRODUCT);
    if with_code {
        probe = probe
            .versioned_install(CODE_DIR, "2a59476c9b", "Code.exe", VSCODE_PRODUCT)
            .registered(false, CODE_DIR, "Code.exe", VSCODE_PRODUCT);
    }
    detect(&probe)
}

#[test]
fn editors_come_in_preference_order_with_their_own_words() {
    let found = forks(true);
    assert_eq!(
        ids(&found),
        [
            "code",
            "code-insiders",
            "codium",
            "cursor",
            "windsurf",
            "foo",
            "kiro"
        ]
    );
    let shorts: Vec<&str> = found.iter().map(|e| e.short.as_str()).collect();
    assert_eq!(
        shorts,
        [
            "VS Code",
            "VS Code Insiders",
            "VSCodium",
            "Cursor",
            "Windsurf",
            // Kete has no words for these: nameShort for "foo", its own for "kiro".
            "Foo IDE",
            "Kiro",
        ]
    );
}

#[test]
fn the_code_default_is_vs_code_and_with_none_installed_insiders() {
    let found = forks(true);
    assert_eq!(
        default_of(&found, Family::Code).map(|e| e.id.as_str()),
        Some("code")
    );
    assert!(default_of(&found, Family::VisualStudio).is_none());

    let found = forks(false);
    assert_eq!(
        default_of(&found, Family::Code).map(|e| e.id.as_str()),
        Some("code-insiders")
    );
}

// ---- user and system installs ----

#[test]
fn user_and_system_installs_of_code_get_their_own_ids() {
    let system = r"C:\Program Files\Microsoft VS Code";
    // The system install is added first: the user's still comes first.
    let probe = FakeProbe::on_windows()
        .versioned_install(system, "2a59476c9b", "Code.exe", VSCODE_PRODUCT)
        .registered(true, system, "Code.exe", VSCODE_PRODUCT)
        .versioned_install(CODE_DIR, "2a59476c9b", "Code.exe", VSCODE_PRODUCT)
        .registered(false, CODE_DIR, "Code.exe", VSCODE_PRODUCT);
    let found = detect(&probe);
    assert_eq!(ids(&found), ["code", "code-system"]);
    assert_eq!(found[0].source, Source::UserInstall);
    assert_eq!(found[1].source, Source::SystemInstall);
    assert_eq!(norm(&found[0].exe), norm(format!(r"{CODE_DIR}\Code.exe")));
    assert_eq!(norm(&found[1].exe), norm(format!(r"{system}\Code.exe")));
    // Both are VS Code to a menu; the id is what tells them apart.
    assert_eq!(found[0].short, found[1].short);
}

#[test]
fn a_third_install_of_the_same_app_is_numbered() {
    let system = r"C:\Program Files\Microsoft VS Code";
    let portable = r"D:\Portable\VSCode";
    let probe = FakeProbe::on_windows()
        .versioned_install(CODE_DIR, "2a59476c9b", "Code.exe", VSCODE_PRODUCT)
        .registered(false, CODE_DIR, "Code.exe", VSCODE_PRODUCT)
        .versioned_install(system, "2a59476c9b", "Code.exe", VSCODE_PRODUCT)
        .registered(true, system, "Code.exe", VSCODE_PRODUCT)
        .versioned_install(portable, "2a59476c9b", "Code.exe", VSCODE_PRODUCT)
        .app_path_at("Code.exe", &format!(r"{portable}\Code.exe"));
    let found = detect(&probe);
    assert_eq!(ids(&found), ["code", "code-system", "code-2"]);
    assert_eq!(found[2].source, Source::AppPaths);
}

// ---- the exe ----

#[test]
fn an_uninstaller_display_icon_falls_back_to_the_name_short_exe() {
    let probe = FakeProbe::on_windows()
        .versioned_install(CODE_DIR, "2a59476c9b", "Code.exe", VSCODE_PRODUCT)
        .exe_at(&format!(r"{CODE_DIR}\unins000.exe"))
        .key(
            false,
            CODE_KEY,
            "1.141.0",
            CODE_DIR,
            &format!(r"{CODE_DIR}\unins000.exe"),
        );
    let found = detect(&probe);
    assert_eq!(ids(&found), ["code"]);
    assert_eq!(norm(&found[0].exe), norm(format!(r"{CODE_DIR}\Code.exe")));

    // nameShort is the exe's name whole, spaces and all.
    let probe = FakeProbe::on_windows()
        .versioned_install(
            INSIDERS_DIR,
            "c4f8a27e90",
            "Code - Insiders.exe",
            INSIDERS_PRODUCT,
        )
        .exe_at(&format!(r"{INSIDERS_DIR}\unins000.exe"))
        .key(
            false,
            "{217B4C08-948D-4EC8-90F8-4D6C2A0E02E5}_is1",
            "1.142.0-insider",
            INSIDERS_DIR,
            &format!(r"{INSIDERS_DIR}\unins000.exe"),
        );
    let found = detect(&probe);
    assert_eq!(ids(&found), ["code-insiders"]);
    assert_eq!(
        norm(&found[0].exe),
        norm(format!(r"{INSIDERS_DIR}\Code - Insiders.exe"))
    );
}

#[test]
fn a_display_icon_whose_file_is_gone_falls_back_to_the_name_short_exe() {
    let probe = FakeProbe::on_windows()
        .versioned_install(CODE_DIR, "2a59476c9b", "Code.exe", VSCODE_PRODUCT)
        .key(
            false,
            CODE_KEY,
            "1.141.0",
            CODE_DIR,
            &format!(r"{CODE_DIR}\Gone.exe"),
        );
    let found = detect(&probe);
    assert_eq!(ids(&found), ["code"]);
    assert_eq!(norm(&found[0].exe), norm(format!(r"{CODE_DIR}\Code.exe")));
}

// ---- what is not an editor ----

/// One uninstall key and its `Code.exe`, with `product` as the flat
/// product.json when given.
fn lone_install(product: Option<&str>) -> Vec<Editor> {
    let exe = format!(r"{CODE_DIR}\Code.exe");
    let mut probe = FakeProbe::on_windows()
        .exe_at(&exe)
        .key(false, CODE_KEY, "1.141.0", CODE_DIR, &exe);
    if let Some(text) = product {
        probe = probe.file(&format!(r"{CODE_DIR}\resources\app\product.json"), text);
    }
    detect(&probe)
}

#[test]
fn the_lone_install_helper_finds_a_good_product_json() {
    // The control for the negative cases below.
    let good = product("code", "Code", "1.141.0");
    assert_eq!(ids(&lone_install(Some(good.as_str()))), ["code"]);
}

#[test]
fn a_product_json_without_an_application_name_is_not_an_editor() {
    assert!(lone_install(Some(r#"{"nameShort":"Code","version":"1.141.0"}"#)).is_empty());
    assert!(lone_install(Some(r#"{"applicationName":"  ","nameShort":"Code"}"#)).is_empty());
}

#[test]
fn a_product_json_without_a_name_short_is_not_an_editor() {
    assert!(lone_install(Some(r#"{"applicationName":"code","version":"1.141.0"}"#)).is_empty());
    assert!(lone_install(Some(r#"{"applicationName":"code","nameShort":""}"#)).is_empty());
}

#[test]
fn a_product_json_that_is_not_json_is_not_an_editor() {
    assert!(lone_install(Some("not json at all")).is_empty());
}

#[test]
fn a_folder_with_no_product_json_is_not_an_editor_even_with_an_exe_icon() {
    assert!(lone_install(None).is_empty());
}

#[test]
fn a_product_json_with_no_exe_beside_it_is_not_an_editor() {
    let probe = FakeProbe::on_windows()
        .file(
            &format!(r"{CODE_DIR}\resources\app\product.json"),
            &product("code", "Code", "1.141.0"),
        )
        .key(
            false,
            CODE_KEY,
            "1.141.0",
            CODE_DIR,
            &format!(r"{CODE_DIR}\Code.exe"),
        );
    assert!(detect(&probe).is_empty());
}

#[test]
fn parse_product_reads_the_captured_product_json_past_a_bom_and_refuses_junk() {
    let p = parse_product(VSCODE_PRODUCT).expect("VS Code's product.json");
    assert_eq!(p.application_name, "code");
    assert_eq!(p.name_short, "Code");
    assert_eq!(p.name_long, "Visual Studio Code");
    assert_eq!(p.data_folder_name, ".vscode");
    assert_eq!(p.quality, "stable");
    assert_eq!(p.version, "1.141.0");

    let with_bom = format!("\u{feff}{VSCODE_PRODUCT}");
    assert_eq!(
        parse_product(&with_bom).map(|p| p.application_name),
        Some("code".to_string())
    );

    assert!(parse_product("").is_none());
    assert!(parse_product("[]").is_none());
    assert!(parse_product("{}").is_none());
}

// ---- PATH ----

#[test]
fn path_on_the_windows_layout_finds_vs_code_from_its_bin_code_cmd() {
    let probe = FakeProbe::on_windows()
        .versioned_install(CODE_DIR, "2a59476c9b", "Code.exe", VSCODE_PRODUCT)
        .exe_at(&format!(r"{CODE_DIR}\bin\code.cmd"))
        .env_var(
            "PATH",
            &format!(r"C:\Windows;{CODE_DIR}\bin;C:\Windows\system32"),
        );
    let found = detect(&probe);
    assert_eq!(ids(&found), ["code"]);
    assert_eq!(found[0].source, Source::Path);
    assert_eq!(found[0].version, "1.141.0");
    assert_eq!(norm(&found[0].dir), norm(CODE_DIR));
    assert_eq!(norm(&found[0].exe), norm(format!(r"{CODE_DIR}\Code.exe")));
}

#[test]
fn path_entries_may_be_quoted() {
    let probe = FakeProbe::on_windows()
        .versioned_install(CODE_DIR, "2a59476c9b", "Code.exe", VSCODE_PRODUCT)
        .exe_at(&format!(r"{CODE_DIR}\bin\code.cmd"))
        .env_var("PATH", &format!(r#"C:\Windows;"{CODE_DIR}\bin""#));
    assert_eq!(ids(&detect(&probe)), ["code"]);
}

#[test]
fn path_finds_a_fork_from_its_deeper_resources_app_bin_cmd_two_folders_up() {
    let bin = format!(r"{CURSOR_DIR}\resources\app\bin");
    let probe = FakeProbe::on_windows()
        .flat_install(CURSOR_DIR, "Cursor.exe", CURSOR_PRODUCT)
        .exe_at(&format!(r"{bin}\cursor.cmd"))
        .env_var("PATH", &format!(r"{bin};C:\Windows"));
    let found = detect(&probe);
    assert_eq!(ids(&found), ["cursor"]);
    assert_eq!(found[0].source, Source::Path);
    assert_eq!(norm(&found[0].dir), norm(CURSOR_DIR));
    assert_eq!(
        norm(&found[0].exe),
        norm(format!(r"{CURSOR_DIR}\Cursor.exe"))
    );
}

#[test]
fn a_code_cmd_in_a_folder_that_is_no_install_finds_nothing() {
    let probe = FakeProbe::on_windows()
        .exe_at(r"C:\tools\bin\code.cmd")
        .env_var("PATH", r"C:\tools\bin;C:\Windows");
    assert!(detect(&probe).is_empty());
}

// ---- off Windows ----

#[test]
fn off_windows_commands_on_path_become_editors_and_there_is_no_visual_studio() {
    let path = std::env::join_paths(["/usr/bin", "/opt/cursor/bin"])
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let probe = FakeProbe::on_unix()
        .exe_at("/usr/bin/code")
        .exe_at("/usr/bin/ls")
        .exe_at("/opt/cursor/bin/cursor")
        .env_var("PATH", &path)
        // vswhere output is there, but only the Windows layout reads it.
        .with_vswhere(VSWHERE_THIS_MACHINE);
    let found = detect(&probe);
    assert_eq!(ids(&found), ["code", "cursor"]);
    assert_eq!(found[0].short, "VS Code");
    assert_eq!(found[1].short, "Cursor");
    for e in &found {
        assert_eq!(e.family, Family::Code);
        assert_eq!(e.source, Source::Path);
        assert_eq!(e.version, "");
        assert_eq!(e.edition, None);
    }
    assert_eq!(norm(&found[0].exe), "/usr/bin/code");
    assert_eq!(norm(&found[0].dir), "/usr/bin");
    assert_eq!(norm(&found[1].exe), "/opt/cursor/bin/cursor");
    assert!(default_of(&found, Family::VisualStudio).is_none());
}

#[test]
fn off_windows_with_nothing_on_path_there_are_no_editors() {
    let probe = FakeProbe::on_unix()
        .exe_at("/usr/bin/ls")
        .env_var("PATH", "/usr/bin");
    assert!(detect(&probe).is_empty());
    assert!(detect(&FakeProbe::on_unix()).is_empty());
}

// ---- Visual Studio ----

fn hand_written() -> Vec<Editor> {
    visual_studio(VSWHERE_HAND_WRITTEN)
}

#[test]
fn visual_studio_keeps_only_complete_launchable_devenv_instances() {
    let found = hand_written();
    for left_out in [BUILD_TOOLS_2019, INCOMPLETE_2022, UNLAUNCHABLE_2019] {
        assert!(!ids(&found).contains(&left_out), "{left_out} is an IDE");
    }
    assert_eq!(found.len(), 7);
    assert!(found.iter().all(|e| e.family == Family::VisualStudio));
    assert!(found.iter().all(|e| e.source == Source::Vswhere));
    assert!(found
        .iter()
        .all(|e| norm(&e.exe).ends_with("/Common7/IDE/devenv.exe")));
}

#[test]
fn visual_studio_edition_comes_from_the_product_id() {
    let found = hand_written();
    let edition = |id: &str| by_id(&found, id).edition.clone();
    assert_eq!(edition(PRO_2022).as_deref(), Some("Professional"));
    assert_eq!(edition(ENTERPRISE_2022).as_deref(), Some("Enterprise"));
    assert_eq!(edition(PREVIEW_2022).as_deref(), Some("Community"));
    assert_eq!(edition(COMMUNITY_2019).as_deref(), Some("Community"));
}

#[test]
fn visual_studio_year_comes_from_the_catalog_else_from_the_major_version() {
    let found = hand_written();
    let year = |id: &str| by_id(&found, id).year.clone();
    assert_eq!(year(PRO_2022).as_deref(), Some("2022"));
    assert_eq!(year(COMMUNITY_2019).as_deref(), Some("2019"));
    assert_eq!(year(ENTERPRISE_2026).as_deref(), Some("2026"));
    // No `catalog` object at all: 18.x is the 2026 product line.
    assert_eq!(year(NO_CATALOG_2026).as_deref(), Some("2026"));
}

#[test]
fn visual_studio_version_is_the_catalog_display_versions_first_word_else_the_installation_version()
{
    let found = hand_written();
    let version = |id: &str| by_id(&found, id).version.clone();
    assert_eq!(version(PRO_2022), "17.14.16");
    // "17.14.41 (September 2026)"
    assert_eq!(version(ENTERPRISE_2022), "17.14.41");
    // "17.15.0 Preview 1.0"
    assert_eq!(version(PREVIEW_2022), "17.15.0");
    // "18.12.0 Insiders [11601.3]"
    assert_eq!(version(INSIDERS_2026), "18.12.0");
    assert_eq!(version(ENTERPRISE_2026), "18.11.0");
    // No catalog: installationVersion whole.
    assert_eq!(version(NO_CATALOG_2026), "18.2.11415.280");
}

#[test]
fn visual_studio_previews_say_their_channel() {
    let found = hand_written();
    let preview = by_id(&found, PREVIEW_2022);
    assert_eq!(preview.short, "Visual Studio 2022 Preview");
    assert_eq!(preview.name, "Visual Studio Community 2022 Preview");
    assert!(preview.prerelease);

    let insiders = by_id(&found, INSIDERS_2026);
    assert_eq!(insiders.short, "Visual Studio 2026 Insiders");
    assert_eq!(insiders.name, "Visual Studio Enterprise 2026 Insiders");
    assert!(insiders.prerelease);

    assert!(found
        .iter()
        .filter(|e| !e.prerelease)
        .all(|e| !e.short.contains("Preview") && !e.short.contains("Insiders")));
}

#[test]
fn visual_studio_lists_releases_newest_first_then_previews_newest_first() {
    let found = hand_written();
    assert_eq!(
        ids(&found),
        [
            ENTERPRISE_2026,
            NO_CATALOG_2026,
            ENTERPRISE_2022,
            PRO_2022,
            COMMUNITY_2019,
            // 18.12 Insiders is newer than 17.15 Preview, and both come after
            // every release.
            INSIDERS_2026,
            PREVIEW_2022,
        ]
    );
}

#[test]
fn visual_studio_tells_two_of_one_year_apart_by_edition() {
    let found = hand_written();
    // Two 2022 17.14 releases.
    assert_eq!(
        by_id(&found, PRO_2022).short,
        "Visual Studio 2022 Professional"
    );
    assert_eq!(
        by_id(&found, ENTERPRISE_2022).short,
        "Visual Studio 2022 Enterprise"
    );
    // Two 2026 releases, one without a catalog.
    assert_eq!(
        by_id(&found, ENTERPRISE_2026).short,
        "Visual Studio 2026 Enterprise"
    );
    assert_eq!(
        by_id(&found, NO_CATALOG_2026).short,
        "Visual Studio 2026 Community"
    );
    // Alone in its year and channel: no edition added.
    assert_eq!(by_id(&found, COMMUNITY_2019).short, "Visual Studio 2019");
}

#[test]
fn visual_studio_carries_the_instance_id_and_paths() {
    let found = hand_written();
    let pro = by_id(&found, PRO_2022);
    assert_eq!(pro.instance_id.as_deref(), Some("5f0a91c2"));
    assert_eq!(pro.name, "Visual Studio Professional 2022");
    assert_eq!(
        norm(&pro.dir),
        "C:/Program Files/Microsoft Visual Studio/2022/Professional"
    );
    assert_eq!(
        norm(&pro.exe),
        "C:/Program Files/Microsoft Visual Studio/2022/Professional/Common7/IDE/devenv.exe"
    );
    assert_eq!(pro.application_name, None);
    assert_eq!(pro.name_short, None);
    assert_eq!(pro.data_folder_name, None);
}

#[test]
fn visual_studio_reads_json_with_a_bom_and_survives_junk() {
    let with_bom = format!("\u{feff}{VSWHERE_HAND_WRITTEN}");
    assert_eq!(visual_studio(&with_bom), hand_written());

    assert!(visual_studio("").is_empty());
    assert!(visual_studio("not json").is_empty());
    assert!(visual_studio("[]").is_empty());
    assert!(visual_studio("{}").is_empty());
    assert!(visual_studio("[{\"instanceId\": 3}]").is_empty());
}

#[test]
fn visual_studio_on_this_machine_is_community_2022_alone() {
    let found = visual_studio(VSWHERE_THIS_MACHINE);
    assert_eq!(ids(&found), ["vs-32c4cf50"]);
}

#[test]
fn only_visual_studio_17_14_and_later_opens_slnx() {
    assert!(vs_editor("17.14.16").opens_slnx());
    assert!(vs_editor("17.14").opens_slnx());
    assert!(vs_editor("17.15.0").opens_slnx());
    assert!(vs_editor("18.0").opens_slnx());
    assert!(vs_editor("18.11.0").opens_slnx());
    assert!(!vs_editor("17.13.5").opens_slnx());
    assert!(!vs_editor("17.0").opens_slnx());
    assert!(!vs_editor("16.11").opens_slnx());
    assert!(!vs_editor("").opens_slnx());
    assert!(!code_editor().opens_slnx());
    // A Code-family version that would pass the numbers still is no Visual Studio.
    assert!(!editor("code", Family::Code, "VS Code", "18.0").opens_slnx());
}

// ---- picking ----

fn pick_list() -> Vec<Editor> {
    vec![
        editor("code", Family::Code, "VS Code", "1.141.0"),
        editor("cursor", Family::Code, "Cursor", "2.6.3"),
        editor(
            "vs-8d41e7b3",
            Family::VisualStudio,
            "Visual Studio 2022 Enterprise",
            "17.14.41",
        ),
        editor(
            "vs-5f0a91c2",
            Family::VisualStudio,
            "Visual Studio 2022 Professional",
            "17.14.16",
        ),
    ]
}

#[test]
fn pick_with_no_id_is_the_family_default() {
    let list = pick_list();
    let id_of = |id: Option<&str>, family| pick(&list, id, family).map(|e| e.id.as_str());
    assert_eq!(id_of(None, Family::Code), Some("code"));
    assert_eq!(id_of(None, Family::VisualStudio), Some("vs-8d41e7b3"));
    // A blank id is no id.
    assert_eq!(id_of(Some(""), Family::Code), Some("code"));
    assert_eq!(id_of(Some("  "), Family::VisualStudio), Some("vs-8d41e7b3"));
}

#[test]
fn pick_with_a_family_word_is_that_familys_default_whatever_family_was_asked() {
    let list = pick_list();
    let id_of = |id: &str, family| pick(&list, Some(id), family).map(|e| e.id.as_str());
    assert_eq!(id_of("vscode", Family::VisualStudio), Some("code"));
    assert_eq!(id_of("visual-studio", Family::Code), Some("vs-8d41e7b3"));
    assert_eq!(id_of(" vscode ", Family::Code), Some("code"));
    assert_eq!(id_of("vs", Family::Code), Some("vs-8d41e7b3"));
}

#[test]
fn pick_with_an_exact_id_is_that_install() {
    let list = pick_list();
    let id_of = |id: &str, family| pick(&list, Some(id), family).map(|e| e.id.as_str());
    assert_eq!(id_of("cursor", Family::Code), Some("cursor"));
    // The id wins over the family the verb asked for.
    assert_eq!(id_of("cursor", Family::VisualStudio), Some("cursor"));
    assert_eq!(
        id_of("vs-5f0a91c2", Family::VisualStudio),
        Some("vs-5f0a91c2")
    );
    assert_eq!(id_of("vs-5f0a91c2", Family::Code), Some("vs-5f0a91c2"));
}

#[test]
fn pick_with_an_unknown_id_is_none() {
    let list = pick_list();
    assert!(pick(&list, Some("windsurf"), Family::Code).is_none());
    assert!(pick(&list, Some("vs-ffffffff"), Family::VisualStudio).is_none());
    // A family word with none of that family installed is none too.
    let only_code = vec![code_editor()];
    assert!(pick(&only_code, Some("visual-studio"), Family::Code).is_none());
    assert!(pick(&only_code, None, Family::VisualStudio).is_none());
    assert!(pick(&[], None, Family::Code).is_none());
}

#[test]
fn family_words_round_trip() {
    for family in [Family::Code, Family::VisualStudio] {
        assert_eq!(Family::from_word(family.word()), Some(family));
    }
    assert_eq!(Family::Code.word(), "vscode");
    assert_eq!(Family::VisualStudio.word(), "visual-studio");
    assert_eq!(Family::from_word(" vscode "), Some(Family::Code));
    assert_eq!(Family::from_word("vs"), Some(Family::VisualStudio));
    // `code` is VS Code's install id, not a family word.
    for word in ["auto", "command", "kete", "", "cursor", "code"] {
        assert_eq!(Family::from_word(word), None, "{word:?}");
    }
}

// ---- reading registry values and versions ----

fn exe_str(value: &str) -> Option<String> {
    exe_of(value).map(|p| p.to_string_lossy().into_owned())
}

#[test]
fn exe_of_strips_quotes_and_the_icon_index() {
    assert_eq!(
        exe_str(r#""C:\Program Files\Foo\foo.exe",0"#).as_deref(),
        Some(r"C:\Program Files\Foo\foo.exe")
    );
    assert_eq!(
        exe_str(r#""C:\Program Files\Foo\foo.exe""#).as_deref(),
        Some(r"C:\Program Files\Foo\foo.exe")
    );
    assert_eq!(
        exe_str(r#""C:\Program Files\Foo\foo.exe", 3"#).as_deref(),
        Some(r"C:\Program Files\Foo\foo.exe")
    );
}

#[test]
fn exe_of_strips_an_unquoted_icon_index() {
    assert_eq!(
        exe_str(r"C:\Foo\foo.exe,0").as_deref(),
        Some(r"C:\Foo\foo.exe")
    );
    assert_eq!(
        exe_str(r"C:\Foo\foo.exe,-101").as_deref(),
        Some(r"C:\Foo\foo.exe")
    );
}

#[test]
fn exe_of_takes_a_plain_path_whole_whatever_its_case() {
    assert_eq!(
        exe_str(r"C:\Foo\foo.exe").as_deref(),
        Some(r"C:\Foo\foo.exe")
    );
    assert_eq!(
        exe_str(r"  C:\Foo\FOO.EXE  ").as_deref(),
        Some(r"C:\Foo\FOO.EXE")
    );
}

#[test]
fn exe_of_is_none_for_anything_that_is_not_an_exe() {
    assert_eq!(exe_str(r"C:\Foo\"), None);
    assert_eq!(exe_str(r"C:\Foo"), None);
    assert_eq!(exe_str(r"C:\Foo\foo.ico"), None);
    assert_eq!(exe_str(r"C:\Foo\foo.ico,0"), None);
    assert_eq!(
        exe_str(r"C:\Program Files\ZAP\Zed Attack Proxy\.install4j\installer.ico"),
        None
    );
    assert_eq!(exe_str(""), None);
}

#[test]
fn exe_of_keeps_a_comma_that_is_not_an_icon_index() {
    assert_eq!(
        exe_str(r"C:\Program Files\Foo, Inc\foo.exe").as_deref(),
        Some(r"C:\Program Files\Foo, Inc\foo.exe")
    );
}

#[test]
fn commit_folders_are_ten_hex_digits() {
    assert!(is_commit_folder("2a59476c9b"));
    assert!(is_commit_folder("2A59476C9B"));
    assert!(is_commit_folder("0123456789"));
    assert!(!is_commit_folder("2a59476c9"));
    assert!(!is_commit_folder("2a59476c9bf"));
    assert!(!is_commit_folder("2a59476c9g"));
    assert!(!is_commit_folder("bin"));
    assert!(!is_commit_folder("resources"));
    assert!(!is_commit_folder(".install4j"));
    assert!(!is_commit_folder(""));
}

#[test]
fn versions_compare_number_by_number() {
    use std::cmp::Ordering::{Equal, Greater, Less};
    assert_eq!(version_cmp("1.141.0", "1.99.3"), Greater);
    assert_eq!(version_cmp("1.99.3", "1.141.0"), Less);
    assert_eq!(version_cmp("17.14.16", "18.0"), Less);
    assert_eq!(version_cmp("18.0", "17.14.16"), Greater);
    assert_eq!(version_cmp("17.14.16", "17.14.16"), Equal);
    assert_eq!(version_cmp("17.14.41", "17.14.16"), Greater);
    // An Insiders version's suffix is not a number.
    assert_eq!(version_cmp("1.142.0-insider", "1.141.0"), Greater);
}

// ---- solutions ----

/// A checkout named `Api` in a temp folder, holding empty files at `files`
/// (slash-separated, relative).
fn checkout(files: &[&str]) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("Api");
    std::fs::create_dir_all(&root).unwrap();
    for rel in files {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "").unwrap();
    }
    (dir, root)
}

fn rels(found: &[Solution]) -> Vec<&str> {
    found.iter().map(|s| s.rel.as_str()).collect()
}

#[test]
fn a_solution_named_for_the_checkout_folder_comes_first() {
    let (_dir, root) = checkout(&["Aaa.sln", "api.sln"]);
    assert_eq!(rels(&solutions(&root)), ["api.sln", "Aaa.sln"]);
}

#[test]
fn a_solution_named_for_the_checkout_comes_first_even_in_a_subfolder() {
    let (_dir, root) = checkout(&["Core.sln", "src/Api.sln"]);
    assert_eq!(rels(&solutions(&root)), ["src/Api.sln", "Core.sln"]);
}

#[test]
fn the_roots_solutions_come_before_a_subfolders() {
    let (_dir, root) = checkout(&["src/Alpha.sln", "lib/Beta.sln", "Core.sln"]);
    assert_eq!(
        rels(&solutions(&root)),
        ["Core.sln", "lib/Beta.sln", "src/Alpha.sln"]
    );
}

#[test]
fn slnx_comes_before_sln_for_the_same_place() {
    let (_dir, root) = checkout(&["Core.sln", "Core.slnx"]);
    let found = solutions(&root);
    assert_eq!(rels(&found), ["Core.slnx", "Core.sln"]);
    assert_eq!(found[0].kind, SolutionKind::Slnx);
    assert_eq!(found[1].kind, SolutionKind::Sln);

    let (_dir, root) = checkout(&["src/Web.sln", "src/Web.slnx"]);
    assert_eq!(rels(&solutions(&root)), ["src/Web.slnx", "src/Web.sln"]);

    // The place outranks the format: a root .sln is before a subfolder's .slnx.
    let (_dir, root) = checkout(&["src/Web.slnx", "Core.sln"]);
    assert_eq!(rels(&solutions(&root)), ["Core.sln", "src/Web.slnx"]);
}

#[test]
fn one_level_down_is_searched_and_two_levels_down_is_not() {
    let (_dir, root) = checkout(&["src/Api.sln", "src/deep/Deep.sln", "a/b/c/Far.sln"]);
    assert_eq!(rels(&solutions(&root)), ["src/Api.sln"]);
}

#[test]
fn dependency_build_and_hidden_folders_are_not_searched() {
    let (_dir, root) = checkout(&[
        "node_modules/Dep.sln",
        "bin/Out.sln",
        "obj/Obj.sln",
        "target/Out3.sln",
        "Bin/Upper.sln",
        ".git/Hooks.sln",
        ".vs/Cache.sln",
        ".hidden/Secret.sln",
        "src/Api.sln",
    ]);
    assert_eq!(rels(&solutions(&root)), ["src/Api.sln"]);
}

#[test]
fn projects_are_offered_only_when_there_is_no_solution_and_all_of_them() {
    let (_dir, root) = checkout(&["native/Native.vcxproj", "lib/Lib.csproj", "Web.csproj"]);
    let found = solutions(&root);
    assert_eq!(
        rels(&found),
        ["Web.csproj", "lib/Lib.csproj", "native/Native.vcxproj"]
    );
    assert_eq!(found[0].kind, SolutionKind::Csproj);
    assert_eq!(found[2].kind, SolutionKind::Vcxproj);

    let (_dir, root) = checkout(&["lib/Lib.csproj", "Api.sln"]);
    assert_eq!(rels(&solutions(&root)), ["Api.sln"]);

    let (_dir, root) = checkout(&["Web.csproj", "src/Api.sln"]);
    assert_eq!(rels(&solutions(&root)), ["src/Api.sln"]);
}

#[test]
fn a_checkout_with_nothing_for_visual_studio_has_no_solutions() {
    let (_dir, root) = checkout(&["README.md", "Cargo.toml", "src/main.rs"]);
    assert!(solutions(&root).is_empty());

    let (_dir, root) = checkout(&[]);
    assert!(solutions(&root).is_empty());

    assert!(solutions(&root.join("missing")).is_empty());
}

#[test]
fn a_solutions_rel_is_slash_separated_and_its_path_is_real() {
    let (_dir, root) = checkout(&["src/Api.sln", "Root.slnx"]);
    let found = solutions(&root);
    let src = found.iter().find(|s| s.rel == "src/Api.sln").unwrap();
    assert_eq!(src.path, root.join("src").join("Api.sln"));
    assert!(src.path.is_file());
    let top = found.iter().find(|s| s.rel == "Root.slnx").unwrap();
    assert_eq!(top.path, root.join("Root.slnx"));
}

#[test]
fn extensions_are_matched_without_regard_to_case() {
    let (_dir, root) = checkout(&["Shout.SLN", "Quiet.SlnX"]);
    let found = solutions(&root);
    let kind_of = |rel: &str| found.iter().find(|s| s.rel == rel).map(|s| s.kind);
    assert_eq!(kind_of("Shout.SLN"), Some(SolutionKind::Sln));
    assert_eq!(kind_of("Quiet.SlnX"), Some(SolutionKind::Slnx));
}

#[test]
fn a_solutions_stem_is_its_file_name_without_the_extension() {
    let solution = Solution {
        path: PathBuf::from("C:/work/Api/Api.Core.sln"),
        rel: "Api.Core.sln".to_string(),
        kind: SolutionKind::Sln,
    };
    assert_eq!(solution.stem(), "Api.Core");

    let (_dir, root) = checkout(&["src/Web.slnx"]);
    assert_eq!(solutions(&root)[0].stem(), "Web");
}

// ---- argv ----

const FOLDER: &str = "C:/work/lane-editors";
const FILE: &str = "C:/work/lane-editors/src/actions.ts";
const SLN: &str = "C:/work/lane-editors/Api.sln";

fn argv_of(editor: &Editor, what: Open, reuse_window: bool) -> Result<Vec<String>, String> {
    argv(editor, &what, &Options { reuse_window }).map(|args| {
        args.iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    })
}

fn args_of(items: &[&str]) -> Result<Vec<String>, String> {
    Ok(items.iter().map(|s| s.to_string()).collect())
}

fn file_open(
    folder: Option<&'static str>,
    line: Option<u32>,
    column: Option<u32>,
) -> Open<'static> {
    Open::File {
        folder: folder.map(Path::new),
        file: Path::new(FILE),
        line,
        column,
    }
}

#[test]
fn code_opens_a_folder_with_the_path_and_reuse_window_adds_r_first() {
    let code = code_editor();
    let open = Open::Folder(Path::new(FOLDER));
    assert_eq!(argv_of(&code, open, false), args_of(&[FOLDER]));
    assert_eq!(argv_of(&code, open, true), args_of(&["-r", FOLDER]));
}

#[test]
fn code_opens_a_file_at_a_line_and_column_in_its_folders_window() {
    let code = code_editor();
    let at = format!("{FILE}:12:3");
    assert_eq!(
        argv_of(&code, file_open(Some(FOLDER), Some(12), Some(3)), false),
        args_of(&[FOLDER, "-g", at.as_str()])
    );
    assert_eq!(
        argv_of(&code, file_open(Some(FOLDER), Some(12), Some(3)), true),
        args_of(&["-r", FOLDER, "-g", at.as_str()])
    );
}

#[test]
fn code_opens_a_file_at_a_line_without_a_column_or_a_folder() {
    let code = code_editor();
    let at = format!("{FILE}:12");
    assert_eq!(
        argv_of(&code, file_open(Some(FOLDER), Some(12), None), false),
        args_of(&[FOLDER, "-g", at.as_str()])
    );
    assert_eq!(
        argv_of(&code, file_open(None, Some(12), None), false),
        args_of(&["-g", at.as_str()])
    );
}

#[test]
fn code_turns_line_0_into_line_1() {
    let code = code_editor();
    let line_1 = format!("{FILE}:1");
    assert_eq!(
        argv_of(&code, file_open(Some(FOLDER), Some(0), None), false),
        args_of(&[FOLDER, "-g", line_1.as_str()])
    );
    let col_1 = format!("{FILE}:1:1");
    assert_eq!(
        argv_of(&code, file_open(Some(FOLDER), Some(0), Some(0)), false),
        args_of(&[FOLDER, "-g", col_1.as_str()])
    );
}

#[test]
fn code_opens_a_file_with_no_line_as_a_plain_path() {
    let code = code_editor();
    assert_eq!(
        argv_of(&code, file_open(Some(FOLDER), None, None), false),
        args_of(&[FOLDER, FILE])
    );
    assert_eq!(
        argv_of(&code, file_open(None, None, None), false),
        args_of(&[FILE])
    );
    // A column means nothing without its line.
    assert_eq!(
        argv_of(&code, file_open(None, None, Some(5)), false),
        args_of(&[FILE])
    );
}

#[test]
fn code_cannot_open_a_solution() {
    let err = argv_of(&code_editor(), Open::Solution(Path::new(SLN)), false).unwrap_err();
    assert!(err.contains("VS Code"), "{err}");
    assert!(err.contains("not a solution"), "{err}");
}

#[test]
fn visual_studio_opens_a_solution_by_its_path() {
    let vs = vs_editor("17.14.16");
    let open = Open::Solution(Path::new(SLN));
    assert_eq!(argv_of(&vs, open, false), args_of(&[SLN]));
    // `-r` is VS Code's.
    assert_eq!(argv_of(&vs, open, true), args_of(&[SLN]));
}

#[test]
fn visual_studio_opens_a_file_with_edit_and_ignores_the_line() {
    let vs = vs_editor("17.14.16");
    assert_eq!(
        argv_of(&vs, file_open(Some(FOLDER), Some(12), Some(3)), false),
        args_of(&["/Edit", FILE])
    );
    assert_eq!(
        argv_of(&vs, file_open(None, None, None), true),
        args_of(&["/Edit", FILE])
    );
}

#[test]
fn visual_studio_cannot_open_a_folder() {
    let err = argv_of(
        &vs_editor("17.14.16"),
        Open::Folder(Path::new(FOLDER)),
        false,
    )
    .unwrap_err();
    assert!(err.contains("solution or a project"), "{err}");
}

// ---- the environment a launched editor must not inherit ----

const PROFILES: &str = "C:/Kete/profiles";

fn stripped(name: &str, value: &str) -> bool {
    strips(OsStr::new(name), OsStr::new(value), Path::new(PROFILES))
}

fn os_vars(pairs: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
    pairs
        .iter()
        .map(|(n, v)| (OsString::from(*n), OsString::from(*v)))
        .collect()
}

#[test]
fn kete_electron_and_host_session_variables_are_stripped() {
    for name in [
        "NEBULA_AGENT_ID",
        "NEBULA_DATA_DIR",
        "KETE_SHELL",
        "BRO_SHELL",
        "ELECTRON_RUN_AS_NODE",
        "VSCODE_IPC_HOOK_CLI",
        "CLAUDECODE",
        "CLAUDE_CODE_SESSION_ID",
    ] {
        assert!(stripped(name, "1"), "{name} should be stripped");
    }
}

#[test]
fn the_users_own_variables_are_kept() {
    for name in [
        "PATH",
        "USERPROFILE",
        "ANTHROPIC_API_KEY",
        "CLAUDE_CODE_USE_BEDROCK",
    ] {
        assert!(!stripped(name, "1"), "{name} should be kept");
    }
}

#[test]
fn a_profiles_config_dir_variable_is_stripped_when_it_points_into_the_profiles_root() {
    for var in [
        "CLAUDE_CONFIG_DIR",
        "GH_CONFIG_DIR",
        "CODEX_HOME",
        "COPILOT_HOME",
    ] {
        assert!(
            stripped(var, "C:/Kete/profiles/work/claude"),
            "{var} inside"
        );
        assert!(stripped(var, "C:/Kete/profiles"), "{var} at the root");
        assert!(
            stripped(var, "C:/Kete/profiles/"),
            "{var} at the root, slash"
        );
        assert!(
            stripped(var, r"C:\Kete\profiles\work\claude"),
            "{var} with backslashes"
        );
    }
}

#[test]
fn a_config_dir_variable_the_user_set_elsewhere_is_kept() {
    for var in [
        "CLAUDE_CONFIG_DIR",
        "GH_CONFIG_DIR",
        "CODEX_HOME",
        "COPILOT_HOME",
    ] {
        assert!(!stripped(var, "C:/Users/Jay/.claude"), "{var} elsewhere");
        // A sibling that merely starts with the root's name is not inside it.
        assert!(
            !stripped(var, "C:/Kete/profiles-old/claude"),
            "{var} sibling"
        );
        assert!(!stripped(var, "C:/Kete"), "{var} above the root");
    }
    // With no root to compare against, nothing is inside it.
    assert!(!strips(
        OsStr::new("CLAUDE_CONFIG_DIR"),
        OsStr::new("C:/Kete/profiles/work"),
        Path::new("")
    ));
}

#[test]
fn a_variable_that_is_not_a_config_dir_is_kept_even_inside_the_profiles_root() {
    assert!(!stripped("MY_NOTES", "C:/Kete/profiles/work"));
}

#[test]
fn launch_env_removals_names_the_stripped_variables_in_order() {
    let vars = os_vars(&[
        ("PATH", "C:/Windows"),
        ("NEBULA_AGENT_ID", "a1"),
        ("USERPROFILE", "C:/Users/Jay"),
        ("KETE_SHELL", "pwsh"),
        ("CLAUDE_CONFIG_DIR", "C:/Kete/profiles/work/claude"),
        ("GH_CONFIG_DIR", "C:/Users/Jay/.config/gh"),
        ("CODEX_HOME", "C:/Kete/profiles"),
    ]);
    assert_eq!(
        launch_env_removals(vars, Path::new(PROFILES)),
        vec![
            OsString::from("NEBULA_AGENT_ID"),
            OsString::from("KETE_SHELL"),
            OsString::from("CLAUDE_CONFIG_DIR"),
            OsString::from("CODEX_HOME"),
        ]
    );
}

#[test]
fn command_is_the_exe_the_argv_and_the_stripped_environment_in_the_folder() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("profiles");
    let inside = root
        .join("work")
        .join("claude")
        .to_string_lossy()
        .into_owned();
    let folder = dir.path().join("Api");
    std::fs::create_dir(&folder).unwrap();
    let code = code_editor();
    let vars = os_vars(&[
        ("PATH", "/usr/bin"),
        ("USERPROFILE", "C:/Users/Jay"),
        ("NEBULA_AGENT_ID", "agent-1"),
        ("KETE_SHELL", "1"),
        ("ELECTRON_RUN_AS_NODE", "1"),
        ("CLAUDE_CONFIG_DIR", inside.as_str()),
        ("GH_CONFIG_DIR", "C:/Users/Jay/.config/gh"),
        ("ANTHROPIC_API_KEY", "sk-test"),
    ]);

    let cmd = command(
        &code,
        &Open::Folder(&folder),
        &Options { reuse_window: true },
        vars,
        &root,
    )
    .unwrap();

    assert_eq!(cmd.get_program(), code.exe.as_os_str());
    let args: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        args,
        ["-r".to_string(), folder.to_string_lossy().into_owned()]
    );

    // Each stripped name is removed (set to None); a kept one is not touched.
    let mut envs: Vec<(String, Option<String>)> = cmd
        .get_envs()
        .map(|(n, v)| {
            (
                n.to_string_lossy().into_owned(),
                v.map(|v| v.to_string_lossy().into_owned()),
            )
        })
        .collect();
    envs.sort();
    let removed: Vec<(String, Option<String>)> = [
        "CLAUDE_CONFIG_DIR",
        "ELECTRON_RUN_AS_NODE",
        "KETE_SHELL",
        "NEBULA_AGENT_ID",
    ]
    .iter()
    .map(|n| (n.to_string(), None))
    .collect();
    assert_eq!(envs, removed);

    assert_eq!(cmd.get_current_dir(), Some(folder.as_path()));
}

#[test]
fn command_has_no_working_folder_when_the_folder_is_gone() {
    let dir = tempfile::tempdir().unwrap();
    let gone = dir.path().join("missing");
    let cmd = command(
        &code_editor(),
        &Open::Folder(&gone),
        &Options::default(),
        Vec::new(),
        dir.path(),
    )
    .unwrap();
    assert_eq!(cmd.get_current_dir(), None);
}

#[test]
fn command_works_in_the_files_folder_or_the_solutions_folder() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("Api");
    std::fs::create_dir(&folder).unwrap();
    let opts = Options::default();

    let file = folder.join("lib.rs");
    let open = Open::File {
        folder: None,
        file: &file,
        line: None,
        column: None,
    };
    let cmd = command(&code_editor(), &open, &opts, Vec::new(), dir.path()).unwrap();
    assert_eq!(cmd.get_current_dir(), Some(folder.as_path()));

    let sln = folder.join("Api.sln");
    let cmd = command(
        &vs_editor("17.14.16"),
        &Open::Solution(&sln),
        &opts,
        Vec::new(),
        dir.path(),
    )
    .unwrap();
    assert_eq!(cmd.get_current_dir(), Some(folder.as_path()));
    let args: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert_eq!(args, [sln.to_string_lossy().into_owned()]);
}

#[test]
fn command_refuses_what_the_editor_cannot_open() {
    let dir = tempfile::tempdir().unwrap();
    let sln = dir.path().join("Api.sln");
    let result = command(
        &code_editor(),
        &Open::Solution(&sln),
        &Options::default(),
        Vec::new(),
        dir.path(),
    );
    assert!(result.is_err());
}

// ---- window titles ----

const CHECKOUT: &str = "C:/Code/bro/.claude/worktrees/lane-editors";

fn code_title_matches(title: &str, folder: &str) -> bool {
    titles::matches(title, Family::Code, Path::new(folder), &[])
}

fn vs_title_matches(title: &str, stems: &[&str]) -> bool {
    let stems: Vec<String> = stems.iter().map(|s| s.to_string()).collect();
    titles::matches(title, Family::VisualStudio, Path::new(CHECKOUT), &stems)
}

#[test]
fn a_vs_code_title_matches_its_folder_without_regard_to_case() {
    let title = "actions.ts - lane-editors - Visual Studio Code";
    assert!(code_title_matches(title, CHECKOUT));
    assert!(code_title_matches(
        title,
        "C:/Code/bro/.claude/worktrees/Lane-Editors"
    ));
    assert!(code_title_matches(
        "LANE-EDITORS - Visual Studio Code",
        CHECKOUT
    ));
}

#[test]
fn a_vs_code_title_does_not_match_a_folder_whose_name_only_starts_the_same() {
    let title = "actions.ts - lane-editors - Visual Studio Code";
    assert!(!code_title_matches(
        title,
        "C:/Code/bro/.claude/worktrees/lane-editor"
    ));
    assert!(!code_title_matches(
        title,
        "C:/Code/bro/.claude/worktrees/lane"
    ));
    assert!(!code_title_matches(title, "C:/Code/bro"));
}

#[test]
fn a_vs_code_title_with_unsaved_changes_still_matches() {
    assert!(code_title_matches(
        "\u{25cf} actions.ts - bro - Visual Studio Code",
        "C:/Code/bro"
    ));
}

#[test]
fn a_vs_code_workspace_title_matches() {
    assert!(code_title_matches(
        "bro (Workspace) - Visual Studio Code",
        "C:/Code/bro"
    ));
}

#[test]
fn a_vs_code_title_with_a_profile_part_matches() {
    assert!(code_title_matches(
        "x.ts - bro - Work - Visual Studio Code",
        "C:/Code/bro"
    ));
}

#[test]
fn a_vs_code_title_with_only_the_app_name_matches_nothing() {
    assert!(!code_title_matches("Visual Studio Code", "C:/Code/bro"));
    assert!(!code_title_matches(
        "Welcome - Visual Studio Code",
        "C:/Code/bro"
    ));
}

#[test]
fn a_folder_with_no_name_matches_no_vs_code_title() {
    assert!(!code_title_matches("bro - Visual Studio Code", ""));
}

#[test]
fn a_visual_studio_title_matches_its_solutions_stem() {
    assert!(vs_title_matches("Api - Microsoft Visual Studio", &["api"]));
    assert!(vs_title_matches("Api - Microsoft Visual Studio", &["Api"]));
    assert!(vs_title_matches(
        "Api - Microsoft Visual Studio",
        &["Web", "Api"]
    ));
}

#[test]
fn an_elevated_visual_studio_title_matches_too() {
    assert!(vs_title_matches(
        "Api - Microsoft Visual Studio (Administrator)",
        &["api"]
    ));
}

#[test]
fn a_visual_studio_title_while_it_runs_or_debugs_still_matches() {
    assert!(vs_title_matches(
        "Api (Running) - Microsoft Visual Studio",
        &["api"]
    ));
    assert!(vs_title_matches(
        "Api (Debugging) - Microsoft Visual Studio (Administrator)",
        &["api"]
    ));
    assert!(!vs_title_matches(
        "Web (Running) - Microsoft Visual Studio",
        &["api"]
    ));
}

#[test]
fn a_one_part_title_named_like_the_folder_is_the_app_alone() {
    // A checkout folder called `cursor` and Cursor's empty window.
    assert!(!code_title_matches("Cursor", "C:/Code/cursor"));
}

#[test]
fn a_visual_studio_title_for_another_solution_does_not_match() {
    assert!(!vs_title_matches("Web - Microsoft Visual Studio", &["api"]));
    assert!(!vs_title_matches(
        "Api2 - Microsoft Visual Studio",
        &["api"]
    ));
    assert!(!vs_title_matches("Api - Microsoft Visual Studio", &[]));
}

#[test]
fn an_empty_title_matches_nothing() {
    assert!(!code_title_matches("", CHECKOUT));
    assert!(!code_title_matches("   ", CHECKOUT));
    assert!(!code_title_matches(" - ", CHECKOUT));
    assert!(!vs_title_matches("", &["api", ""]));
    assert!(!vs_title_matches(" - ", &["api", ""]));
}

// ---- dock ----

/// Windows 11's invisible borders on a normal window.
const WIN11: Insets = Insets {
    left: 7,
    top: 0,
    right: 7,
    bottom: 7,
};

/// The frame a window draws inside its outer rectangle.
fn drawn(outer: Rect, i: Insets) -> Rect {
    Rect::new(
        outer.left + i.left,
        outer.top + i.top,
        outer.right - i.right,
        outer.bottom - i.bottom,
    )
}

#[test]
fn kete_mostly_on_the_left_keeps_the_left_and_the_editor_gets_the_right() {
    let work = Rect::new(0, 0, 1920, 1040);
    let kete = Rect::new(100, 100, 900, 800);
    let plan = dock::plan(work, kete, Insets::default(), WIN11);
    assert_eq!(
        plan,
        Plan {
            kete_side: Side::Left,
            kete: Rect::new(0, 0, 960, 1040),
            // The right half grown by the editor's borders: 7 left and right,
            // 7 below, none above.
            editor: Rect::new(953, 0, 1927, 1047),
        }
    );
}

#[test]
fn kete_on_the_right_keeps_the_right_and_the_editor_gets_the_left() {
    let work = Rect::new(0, 0, 1920, 1040);
    let kete = Rect::new(1000, 100, 1900, 800);
    let plan = dock::plan(work, kete, Insets::default(), WIN11);
    assert_eq!(
        plan,
        Plan {
            kete_side: Side::Right,
            kete: Rect::new(960, 0, 1920, 1040),
            editor: Rect::new(-7, 0, 967, 1047),
        }
    );
}

#[test]
fn each_outer_rectangle_is_its_half_grown_by_that_windows_own_insets() {
    let work = Rect::new(0, 0, 1920, 1040);
    let kete = Rect::new(100, 100, 900, 800);
    let kete_insets = Insets {
        left: 1,
        top: 2,
        right: 3,
        bottom: 4,
    };
    let plan = dock::plan(work, kete, kete_insets, WIN11);
    assert_eq!(plan.kete, Rect::new(-1, -2, 963, 1044));
    assert_eq!(plan.editor, Rect::new(953, 0, 1927, 1047));
}

#[test]
fn the_window_mostly_on_a_half_is_judged_by_its_centre() {
    let work = Rect::new(0, 0, 1920, 1040);
    let none = Insets::default();
    // Straddling the split, centre at 900 and 1050.
    let side = |kete| dock::plan(work, kete, none, none).kete_side;
    assert_eq!(side(Rect::new(400, 100, 1400, 800)), Side::Left);
    assert_eq!(side(Rect::new(600, 100, 1500, 800)), Side::Right);
    // Centred exactly on the split counts as the right.
    assert_eq!(side(Rect::new(460, 100, 1460, 800)), Side::Right);
}

#[test]
fn an_odd_work_width_gives_the_extra_pixel_to_the_right_half() {
    let (left, right) = dock::halves(Rect::new(0, 0, 1921, 1040));
    assert_eq!(left, Rect::new(0, 0, 960, 1040));
    assert_eq!(right, Rect::new(960, 0, 1921, 1040));
    assert_eq!((left.width(), right.width()), (960, 961));
}

#[test]
fn a_work_area_that_starts_right_of_zero_splits_at_its_own_middle() {
    // The taskbar docked on the left: the work area starts at x = 48.
    let work = Rect::new(48, 0, 1920, 1080);
    let (left, right) = dock::halves(work);
    assert_eq!(left, Rect::new(48, 0, 984, 1080));
    assert_eq!(right, Rect::new(984, 0, 1920, 1080));

    let plan = dock::plan(
        work,
        Rect::new(200, 100, 1000, 800),
        Insets::default(),
        WIN11,
    );
    assert_eq!(plan.kete_side, Side::Left);
    assert_eq!(plan.kete, Rect::new(48, 0, 984, 1080));
    assert_eq!(plan.editor, Rect::new(977, 0, 1927, 1087));
}

#[test]
fn a_second_monitor_at_negative_x_splits_correctly() {
    // A monitor to the left of the primary one.
    let work = Rect::new(-1920, 0, 0, 1040);
    let (left, right) = dock::halves(work);
    assert_eq!(left, Rect::new(-1920, 0, -960, 1040));
    assert_eq!(right, Rect::new(-960, 0, 0, 1040));

    let on_left = dock::plan(
        work,
        Rect::new(-1800, 100, -1000, 800),
        Insets::default(),
        WIN11,
    );
    assert_eq!(on_left.kete_side, Side::Left);
    assert_eq!(on_left.kete, left);
    assert_eq!(on_left.editor, Rect::new(-967, 0, 7, 1047));

    let on_right = dock::plan(
        work,
        Rect::new(-900, 100, -100, 800),
        Insets::default(),
        WIN11,
    );
    assert_eq!(on_right.kete_side, Side::Right);
    assert_eq!(on_right.kete, right);
    assert_eq!(on_right.editor, Rect::new(-1927, 0, -953, 1047));

    // An odd width at negative x still gives the odd pixel to the right.
    let (left, right) = dock::halves(Rect::new(-1921, 0, 0, 1040));
    assert_eq!((left.width(), right.width()), (960, 961));
    assert_eq!(left.right, right.left);
}

#[test]
fn the_two_drawn_frames_meet_with_no_gap_and_no_overlap() {
    let kete_insets = Insets {
        left: 7,
        top: 0,
        right: 7,
        bottom: 7,
    };
    let works = [
        Rect::new(0, 0, 1920, 1040),
        Rect::new(0, 0, 1921, 1040),
        Rect::new(48, 0, 1920, 1080),
        Rect::new(-1920, 0, 0, 1040),
        Rect::new(-1921, 20, 0, 1040),
        Rect::new(1920, 0, 4480, 1400),
    ];
    for work in works {
        let (left, right) = dock::halves(work);
        for kete in [
            Rect::new(left.left + 10, 100, left.left + 400, 600),
            Rect::new(right.right - 400, 100, right.right - 10, 600),
        ] {
            let plan = dock::plan(work, kete, kete_insets, WIN11);
            let (kete_frame, editor_frame) =
                (drawn(plan.kete, kete_insets), drawn(plan.editor, WIN11));
            let (a, b) = match plan.kete_side {
                Side::Left => (kete_frame, editor_frame),
                Side::Right => (editor_frame, kete_frame),
            };
            assert_eq!(a.left, work.left, "{work:?}");
            assert_eq!(a.right, b.left, "{work:?}");
            assert_eq!(b.right, work.right, "{work:?}");
            assert_eq!((a.top, a.bottom), (work.top, work.bottom), "{work:?}");
            assert_eq!((b.top, b.bottom), (work.top, work.bottom), "{work:?}");
        }
    }
}

#[test]
fn insets_are_how_far_the_outer_rectangle_reaches_past_the_drawn_frame() {
    let outer = Rect::new(93, 100, 907, 807);
    let frame = Rect::new(100, 100, 900, 800);
    let insets = Insets::between(outer, frame);
    assert_eq!(insets, WIN11);
    assert_eq!(insets.grow(frame), outer);
}

#[test]
fn insets_are_never_negative() {
    // A frame that reaches past its outer rectangle on every side.
    let insets = Insets::between(Rect::new(10, 10, 90, 90), Rect::new(0, 0, 100, 100));
    assert_eq!(insets, Insets::default());
    // And one side at a time.
    let insets = Insets::between(Rect::new(0, 10, 100, 100), Rect::new(5, 0, 100, 100));
    assert_eq!(
        insets,
        Insets {
            left: 5,
            top: 0,
            right: 0,
            bottom: 0
        }
    );
}

// ---- icons, and everything else that needs Windows ----

#[test]
fn the_icon_cache_is_under_the_data_dir() {
    assert_eq!(
        norm(icon_cache_dir(Path::new("/var/kete"))),
        "/var/kete/cache/editor-icons"
    );
    assert_eq!(
        icon_cache_dir(Path::new("data")),
        Path::new("data").join("cache").join("editor-icons")
    );
}

#[cfg(windows)]
#[test]
fn encode_png_round_trips_a_two_by_two_image() {
    // Opaque red, half-transparent green, fully transparent blue, grey.
    let rgba: Vec<u8> = vec![
        255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0, 10, 20, 30, 255,
    ];
    let bytes = super::win::encode_png(&rgba, 2, 2).expect("a PNG");
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");

    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!((info.width, info.height), (2, 2));
    assert_eq!(info.color_type, png::ColorType::Rgba);
    assert_eq!(info.bit_depth, png::BitDepth::Eight);
    assert_eq!(&buf[..info.buffer_size()], rgba.as_slice());
}

#[cfg(not(windows))]
#[test]
fn off_windows_the_window_calls_say_so_and_find_nothing() {
    let editors = vec![code_editor()];
    assert!(windows(&editors).is_empty());
    assert!(open_on(&editors, Path::new("/work/bro"), &[]).is_empty());
    assert!(console_window().is_none());
    assert_eq!(icon(&editors[0], 32, Path::new("/tmp/kete-icons")), None);
    let err = dock(&editors, Path::new("/work/bro"), &[], 0).unwrap_err();
    assert!(err.contains("Windows-only"), "{err}");
}

// ---- serde ----

#[test]
fn an_editor_serialises_with_kebab_case_names_and_skips_what_it_lacks() {
    let code = Editor {
        source: Source::UserInstall,
        application_name: Some("code".to_string()),
        ..code_editor()
    };
    let json = serde_json::to_value(&code).unwrap();
    assert_eq!(json["family"], "code");
    assert_eq!(json["source"], "user-install");
    assert_eq!(json["application_name"], "code");
    assert!(json.get("instance_id").is_none());
    assert!(json.get("name_short").is_none());
    assert!(json.get("data_folder_name").is_none());
    let back: Editor = serde_json::from_value(json).unwrap();
    assert_eq!(back, code);

    let vs = Editor {
        source: Source::Vswhere,
        instance_id: Some("32c4cf50".to_string()),
        ..vs_editor("17.14.16")
    };
    let json = serde_json::to_value(&vs).unwrap();
    assert_eq!(json["family"], "visual-studio");
    assert_eq!(json["source"], "vswhere");
    assert_eq!(json["instance_id"], "32c4cf50");
    assert!(json.get("application_name").is_none());
    let back: Editor = serde_json::from_value(json).unwrap();
    assert_eq!(back, vs);
}

#[test]
fn every_source_has_a_kebab_case_name() {
    for (source, name) in [
        (Source::UserInstall, "user-install"),
        (Source::SystemInstall, "system-install"),
        (Source::AppPaths, "app-paths"),
        (Source::KnownFolder, "known-folder"),
        (Source::Path, "path"),
        (Source::Vswhere, "vswhere"),
    ] {
        assert_eq!(serde_json::to_value(source).unwrap(), name);
    }
}

#[test]
fn a_solution_kind_serialises_in_lower_case() {
    assert_eq!(serde_json::to_value(SolutionKind::Slnx).unwrap(), "slnx");
    assert_eq!(
        serde_json::to_value(SolutionKind::Vcxproj).unwrap(),
        "vcxproj"
    );
}
