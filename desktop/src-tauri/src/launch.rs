//! The quick prompt's Start: into the picked worktree or a new one cut
//! for it (named as the TUI's quick prompt names it), a session with the
//! picked harness, profile, model and effort, its task wrapped by the
//! picked agent preset's prefix and postfix — or a plain terminal.

use crate::daemon::Client;
use crate::{NewAgent, Res};
use nebula_config::Config;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickLaunch {
    pub project: String,
    /// The worktree to start in; ignored when `new_worktree` is set.
    #[serde(default)]
    pub worktree: String,
    #[serde(default)]
    pub new_worktree: bool,
    /// A harness id, or `terminal` for a shell.
    pub kind: String,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
    /// An agent preset whose text wraps the task.
    #[serde(default)]
    pub preset: Option<String>,
}

/// What started, for the window to open.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Launched {
    pub worktree: String,
    pub agent: Option<String>,
    pub terminal: Option<String>,
}

pub const TERMINAL: &str = "terminal";

/// The starting prompt: the task, wrapped by the preset when one is
/// picked.
pub fn prompt_for(store: &std::path::Path, preset: Option<&str>, task: &str) -> Res<String> {
    match preset.map(str::trim).filter(|p| !p.is_empty()) {
        Some(name) => crate::presets::compose_at(store, name, task),
        None => Ok(task.trim().to_string()),
    }
}

pub async fn quick_launch_in(
    client: &Client,
    cfg: &Config,
    store: &std::path::Path,
    spec: QuickLaunch,
) -> Res<Launched> {
    let prompt = prompt_for(store, spec.preset.as_deref(), &spec.prompt)?;
    let worktree = if spec.new_worktree {
        crate::projects::create_worktree_in(client, spec.project.clone(), String::new(), None)
            .await?
    } else if spec.worktree.is_empty() {
        return Err("Pick a worktree first.".into());
    } else {
        spec.worktree.clone()
    };
    if spec.kind == TERMINAL {
        let terminal = crate::new_terminal(client, worktree.clone()).await?;
        return Ok(Launched {
            worktree,
            agent: None,
            terminal: Some(terminal),
        });
    }
    let agent = crate::spawn_agent_with(
        client,
        cfg,
        NewAgent {
            worktree: worktree.clone(),
            kind: spec.kind,
            profile: spec.profile,
            prompt: Some(prompt),
            model: spec.model.filter(|m| !m.trim().is_empty()),
            effort: spec.effort.filter(|e| !e.trim().is_empty()),
        },
    )
    .await?;
    Ok(Launched {
        worktree,
        agent: Some(agent),
        terminal: None,
    })
}

#[tauri::command]
pub async fn quick_launch(client: State<'_, Arc<Client>>, spec: QuickLaunch) -> Res<Launched> {
    quick_launch_in(
        &client,
        &Config::load(),
        &nebula_config::presets::store_path(),
        spec,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::testkit::run;
    use nebula_core::{ClientRequest, EntityId, Project, ProjectId, Worktree, WorktreeId};
    use std::path::PathBuf;

    fn spec(kind: &str) -> QuickLaunch {
        QuickLaunch {
            project: "p1".into(),
            worktree: "w1".into(),
            new_worktree: false,
            kind: kind.into(),
            profile: None,
            prompt: "  fix the totals ".into(),
            model: Some("sonnet".into()),
            effort: None,
            preset: None,
        }
    }

    fn client() -> (
        Arc<Client>,
        tokio::sync::mpsc::UnboundedReceiver<ClientRequest>,
    ) {
        let (client, rx) = Client::connected_for_test();
        client.set_model(|m| {
            m.projects = vec![Project {
                id: ProjectId("p1".into()),
                name: "repo".into(),
                repo_path: PathBuf::from("/repo"),
                sort_order: 0,
            }];
            m.worktrees = vec![Worktree {
                id: WorktreeId("w1".into()),
                project_id: ProjectId("p1".into()),
                path: PathBuf::from("/repo"),
                branch: "main".into(),
                is_main: true,
                sort_order: 0,
            }];
        });
        (client, rx)
    }

    fn created(req: &ClientRequest) -> Result<Option<EntityId>, String> {
        Ok(Some(match req {
            ClientRequest::CreateWorktree { .. } => EntityId::Worktree(WorktreeId("w9".into())),
            ClientRequest::CreateAgent { .. } => EntityId::Agent(nebula_core::AgentId("a9".into())),
            ClientRequest::CreateTerminal { .. } => {
                EntityId::Terminal(nebula_core::TerminalId("t9".into()))
            }
            other => panic!("unexpected {other:?}"),
        }))
    }

    #[tokio::test]
    async fn a_launch_into_a_new_worktree_cuts_it_then_starts_the_session_there() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("config.json");
        std::fs::write(&cfg, "{}").unwrap();
        let store = dir.path().join("agent_presets.json");
        std::fs::write(
            &store,
            r#"[{"name": "strict", "kind": "claude", "prefix": "Be strict.", "postfix": "Run the tests."}]"#,
        )
        .unwrap();
        let (client, mut rx) = client();
        let mut s = spec("claude");
        s.new_worktree = true;
        s.preset = Some("strict".into());
        let cfg = Config::load_at(&cfg, &dir.path().join("config.local.json"));
        let (r, seen) = run(
            &client,
            &mut rx,
            quick_launch_in(&client, &cfg, &store, s),
            created,
        )
        .await;
        assert_eq!(
            r.unwrap(),
            Launched {
                worktree: "w9".into(),
                agent: Some("a9".into()),
                terminal: None
            }
        );
        let ClientRequest::CreateWorktree { branch, base, .. } = &seen[0] else {
            panic!("{seen:?}")
        };
        assert_eq!(branch.split('-').count(), 3, "a made-up name: {branch}");
        assert!(base.is_none());
        let ClientRequest::CreateAgent {
            worktree,
            starting_prompt,
            model,
            ..
        } = &seen[1]
        else {
            panic!("{seen:?}")
        };
        assert_eq!(worktree.0, "w9");
        assert_eq!(
            starting_prompt.as_deref(),
            Some("Be strict.\n\nfix the totals\n\nRun the tests.")
        );
        assert_eq!(model.as_deref(), Some("sonnet"));
    }

    #[tokio::test]
    async fn a_terminal_launch_opens_a_shell_in_the_picked_worktree() {
        let dir = tempfile::tempdir().unwrap();
        let (client, mut rx) = client();
        let (r, seen) = run(
            &client,
            &mut rx,
            quick_launch_in(
                &client,
                &Config::default(),
                &dir.path().join("p.json"),
                spec(TERMINAL),
            ),
            created,
        )
        .await;
        assert_eq!(r.unwrap().terminal.as_deref(), Some("t9"));
        assert!(
            matches!(&seen[0], ClientRequest::CreateTerminal { worktree, .. } if worktree.0 == "w1")
        );
    }

    #[test]
    fn no_preset_sends_the_trimmed_task() {
        let dir = tempfile::tempdir().unwrap();
        let store = dir.path().join("p.json");
        assert_eq!(prompt_for(&store, None, "  do it \n").unwrap(), "do it");
        assert_eq!(prompt_for(&store, Some(" "), "x").unwrap(), "x");
        assert!(prompt_for(&store, Some("gone"), "x").is_err());
    }
}
