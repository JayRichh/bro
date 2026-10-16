//! `kete editors list|open`: the editors the menus open, from the command
//! line, for scripts and for a look at what Kete found. The work is
//! `nebula_core::editors`; this resolves what was asked into which install
//! opens what.

use anyhow::{bail, Result};
use nebula_core::editors::{self, Editor, Family, SolutionKind};
use std::path::{Path, PathBuf};

/// `kete editors list`.
pub(crate) fn run_list(json: bool) -> Result<()> {
    let found = editors::detected();
    if json {
        println!("{}", serde_json::to_string_pretty(&*found)?);
        return Ok(());
    }
    if found.is_empty() {
        println!("No VS Code (or a fork) and no Visual Studio found.");
        return Ok(());
    }
    print!("{}", table(&found));
    Ok(())
}

/// The list as a table: one row per install, the default of each family
/// marked.
fn table(found: &[Editor]) -> String {
    let id_w = found.iter().map(|e| e.id.len()).max().unwrap_or(2).max(2);
    let name_w = found.iter().map(|e| e.name.len()).max().unwrap_or(4).max(4);
    let mut out = String::new();
    for (i, e) in found.iter().enumerate() {
        let default = found.iter().position(|x| x.family == e.family) == Some(i);
        let edition = match e.family {
            Family::VisualStudio => e.edition.clone().unwrap_or_default(),
            Family::Code => String::new(),
        };
        out.push_str(&format!(
            "{} {:<id_w$}  {:<name_w$}  {:<12} {:<12} {}\n",
            if default { "*" } else { " " },
            e.id,
            e.name,
            e.version,
            edition,
            e.exe.display()
        ));
    }
    out
}

/// What `kete editors open` will start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Plan {
    Folder(PathBuf),
    File {
        folder: Option<PathBuf>,
        file: PathBuf,
        line: Option<u32>,
    },
    Solution(PathBuf),
}

fn is_solution(path: &Path) -> bool {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    matches!(ext.as_str(), "sln" | "slnx" | "csproj" | "vcxproj")
}

/// The family `--with` names: `vscode` (or nothing), `vs`, or an
/// install's id.
fn family_of(found: &[Editor], with: Option<&str>, path: &Path) -> Family {
    match with.map(str::trim) {
        None | Some("") => {
            if is_solution(path) {
                Family::VisualStudio
            } else {
                Family::Code
            }
        }
        Some(w) => Family::from_word(w)
            .or_else(|| found.iter().find(|e| e.id == w).map(|e| e.family))
            .unwrap_or(Family::Code),
    }
}

/// The git checkout a path is in: the nearest folder up with a `.git`.
fn checkout_of(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|p| p.join(".git").exists())
        .map(Path::to_path_buf)
}

/// Which install opens `path` (a folder when `is_dir`), and how. Err is
/// the line to print: nothing installed of the kind asked for, an id no
/// install has, Visual Studio on a folder with no solution, VS Code on a
/// solution file.
pub(crate) fn plan<'a>(
    found: &'a [Editor],
    path: &Path,
    is_dir: bool,
    with: Option<&str>,
    line: Option<u32>,
    solutions: &[editors::Solution],
) -> Result<(&'a Editor, Plan), String> {
    let family = family_of(found, with, path);
    let editor =
        editors::pick(found, with.filter(|w| !w.trim().is_empty()), family).ok_or_else(|| {
            match with {
                Some(w) if Family::from_word(w).is_none() && !w.trim().is_empty() => {
                    format!("No install has the id {w:?}: `kete editors list` shows them")
                }
                _ => match family {
                    Family::Code => "VS Code isn't installed (or Kete hasn't found it)".to_string(),
                    Family::VisualStudio => {
                        "Visual Studio isn't installed (or Kete hasn't found it)".to_string()
                    }
                },
            }
        })?;
    let what = match (editor.family, is_dir) {
        (Family::Code, true) => Plan::Folder(path.to_path_buf()),
        (Family::Code, false) if is_solution(path) => {
            return Err(format!("{} opens a folder, not a solution", editor.short))
        }
        (Family::Code, false) => Plan::File {
            folder: checkout_of(path).filter(|c| c != path),
            file: path.to_path_buf(),
            line,
        },
        (Family::VisualStudio, false) if is_solution(path) => Plan::Solution(path.to_path_buf()),
        (Family::VisualStudio, false) => Plan::File {
            folder: None,
            file: path.to_path_buf(),
            line: None,
        },
        (Family::VisualStudio, true) => {
            let pick = solutions
                .iter()
                .find(|s| s.kind != SolutionKind::Slnx || editor.opens_slnx())
                .ok_or_else(|| {
                    format!(
                        "Visual Studio opens a solution or a project, and {} has none",
                        path.display()
                    )
                })?;
            Plan::Solution(pick.path.clone())
        }
    };
    Ok((editor, what))
}

/// `kete editors open`.
pub(crate) fn run_open(path: &Path, with: Option<&str>, line: Option<u32>) -> Result<()> {
    let path = std::path::absolute(path)?;
    if !path.exists() {
        bail!("{} is not there", path.display());
    }
    let found = editors::detected();
    let is_dir = path.is_dir();
    let sols = if is_dir {
        editors::solutions(&path)
    } else {
        Vec::new()
    };
    let (editor, what) =
        plan(&found, &path, is_dir, with, line, &sols).map_err(anyhow::Error::msg)?;
    let open = match &what {
        Plan::Folder(f) => editors::Open::Folder(f),
        Plan::File { folder, file, line } => editors::Open::File {
            folder: folder.as_deref(),
            file,
            line: *line,
            column: None,
        },
        Plan::Solution(s) => editors::Open::Solution(s),
    };
    let opts = editors::Options {
        reuse_window: nebula_config::desktop::DesktopPrefs::load().editor_reuse == "on",
    };
    editors::launch(editor, &open, &opts).map_err(anyhow::Error::msg)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{Cli, Command, EditorsCommand};
    use clap::Parser;
    use nebula_core::editors::Source;

    fn install(id: &str, family: Family, short: &str, version: &str) -> Editor {
        Editor {
            id: id.into(),
            family,
            short: short.into(),
            name: short.into(),
            version: version.into(),
            edition: (family == Family::VisualStudio).then(|| "Community".into()),
            year: None,
            prerelease: false,
            exe: PathBuf::from(format!("/x/{id}")),
            dir: PathBuf::from("/x"),
            source: Source::Path,
            instance_id: None,
            application_name: None,
            name_short: None,
            data_folder_name: None,
        }
    }

    fn found() -> Vec<Editor> {
        vec![
            install("code", Family::Code, "VS Code", "1.141.0"),
            install("cursor", Family::Code, "Cursor", "1.7.0"),
            install(
                "vs-1",
                Family::VisualStudio,
                "Visual Studio 2022",
                "17.14.16",
            ),
        ]
    }

    fn sln(rel: &str, kind: SolutionKind) -> editors::Solution {
        editors::Solution {
            path: PathBuf::from("/r").join(rel),
            rel: rel.into(),
            kind,
        }
    }

    #[test]
    fn the_subcommands_parse() {
        let cli = Cli::try_parse_from(["kete", "editors", "list", "--json"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Editors {
                command: EditorsCommand::List { json: true }
            })
        ));
        let cli = Cli::try_parse_from([
            "kete", "editors", "open", "src/a.rs", "--with", "vs", "--line", "42",
        ])
        .unwrap();
        match cli.command {
            Some(Command::Editors {
                command: EditorsCommand::Open { path, with, line },
            }) => {
                assert_eq!(path, PathBuf::from("src/a.rs"));
                assert_eq!(with.as_deref(), Some("vs"));
                assert_eq!(line, Some(42));
            }
            _ => panic!("not editors open"),
        }
        assert!(Cli::try_parse_from(["kete", "editors", "open"]).is_err());
        assert!(Cli::try_parse_from(["kete", "editors", "open", ".", "--line", "x"]).is_err());
    }

    #[test]
    fn a_folder_opens_in_vs_code_and_with_vs_its_solution() {
        let f = found();
        let r = Path::new("/r");
        let (e, what) = plan(&f, r, true, None, None, &[]).unwrap();
        assert_eq!((e.id.as_str(), what), ("code", Plan::Folder(r.into())));
        let sols = [sln("Api.sln", SolutionKind::Sln)];
        let (e, what) = plan(&f, r, true, Some("vs"), None, &sols).unwrap();
        assert_eq!(
            (e.id.as_str(), what),
            ("vs-1", Plan::Solution("/r/Api.sln".into()))
        );
        assert!(plan(&f, r, true, Some("vs"), None, &[]).is_err());
        let (e, _) = plan(&f, r, true, Some("cursor"), None, &[]).unwrap();
        assert_eq!(e.id, "cursor");
        let err = plan(&f, r, true, Some("zed"), None, &[]).unwrap_err();
        assert!(err.contains("kete editors list"), "{err}");
    }

    #[test]
    fn a_file_opens_at_its_line_and_a_solution_file_in_visual_studio() {
        let f = found();
        let file = Path::new("/r/src/a.rs");
        let (e, what) = plan(&f, file, false, None, Some(7), &[]).unwrap();
        assert_eq!(e.id, "code");
        assert!(matches!(what, Plan::File { line: Some(7), .. }));
        // Visual Studio takes the file, not the line.
        let (_, what) = plan(&f, file, false, Some("visual-studio"), Some(7), &[]).unwrap();
        assert!(matches!(what, Plan::File { line: None, .. }));
        let api = Path::new("/r/Api.slnx");
        let (e, what) = plan(&f, api, false, None, None, &[]).unwrap();
        assert_eq!((e.id.as_str(), what), ("vs-1", Plan::Solution(api.into())));
        assert!(plan(&f, api, false, Some("vscode"), None, &[]).is_err());
    }

    #[test]
    fn an_old_visual_studio_skips_an_slnx_for_the_sln_beside_it() {
        let mut f = found();
        f[2].version = "17.10.0".into();
        let sols = [
            sln("Api.slnx", SolutionKind::Slnx),
            sln("Api.sln", SolutionKind::Sln),
        ];
        let (_, what) = plan(&f, Path::new("/r"), true, Some("vs"), None, &sols).unwrap();
        assert_eq!(what, Plan::Solution("/r/Api.sln".into()));
        assert!(plan(&f, Path::new("/r"), true, Some("vs"), None, &sols[..1]).is_err());
    }

    #[test]
    fn nothing_installed_says_so() {
        let err = plan(&[], Path::new("/r"), true, None, None, &[]).unwrap_err();
        assert!(err.contains("VS Code isn't installed"), "{err}");
        let err = plan(&[], Path::new("/r/A.sln"), false, None, None, &[]).unwrap_err();
        assert!(err.contains("Visual Studio isn't installed"), "{err}");
    }

    #[test]
    fn the_list_marks_each_kinds_default() {
        let t = table(&found());
        let lines: Vec<&str> = t.lines().collect();
        assert!(lines[0].starts_with("* code"), "{t}");
        assert!(lines[1].starts_with("  cursor"), "{t}");
        assert!(lines[2].starts_with("* vs-1"), "{t}");
        assert!(lines[2].contains("Community"), "{t}");
    }
}
