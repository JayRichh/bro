//! `kete ado`: the desktop's Azure DevOps powers from the command line, for
//! the orchestrator (and anyone at a shell). Everything goes through the
//! shared `kete-ado` crate, so the rules are the desktop's: the same team,
//! the same Start task, the same composer checks and co-author cleaning.
//!
//! Sign-in is never interactive. The contexts here are the silent ones
//! (GCM with `credential.interactive=never`, then `az account
//! get-access-token`), and `GCM_INTERACTIVE=never` / `GIT_TERMINAL_PROMPT=0`
//! are set for every git this process runs (the push included). A missing
//! credential is an error that says how to sign in, never a prompt.

mod daemon;

use crate::cli::{AdoCommand, AdoPrAction};
use anyhow::{anyhow, bail, Result};
use kete_ado::client::{AdoError, CmdError, ErrorKind};
use kete_ado::prs::{self, PrCtx, PrSummary};
use kete_ado::store::Reviewer;
use kete_ado::tasks::{self, StartTask, StartedTask, TaskList, TaskScope, WorkItem};
use kete_ado::{AdoContext, AdoState, NewAgent};
use serde_json::json;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// What every sign-in failure ends with.
pub const SIGN_IN_HINT: &str =
    "Azure DevOps sign-in needed: sign in from the desktop app or run `az login`, then try again.";

pub fn run_ado(command: AdoCommand) -> Result<()> {
    // Before any thread or child exists: every git (GCM's `credential
    // fill`, the PR push) inherits these, so nothing can open a sign-in
    // window or ask on the terminal.
    std::env::set_var("GCM_INTERACTIVE", "never");
    std::env::set_var("GIT_TERMINAL_PROMPT", "0");
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let out = rt.block_on(run(command))?;
    if !out.is_empty() {
        println!("{}", out.trim_end());
    }
    Ok(())
}

async fn run(command: AdoCommand) -> Result<String> {
    let state = AdoState::system();
    let ctx = cwd_context(&state).await?;
    match command {
        AdoCommand::Tasks { all, open, json } => {
            let scope = if all {
                TaskScope::SprintAll
            } else if open {
                TaskScope::MyOpen
            } else {
                TaskScope::CurrentSprint
            };
            let list = tasks::tasks_or_cached(&ctx, &scope, &tasks::today_utc())
                .await
                .map_err(ado_err)?;
            Ok(if json {
                serde_json::to_string_pretty(&list)?
            } else {
                format_tasks(&list)
            })
        }
        AdoCommand::StartTask {
            id,
            agent,
            no_agent,
            branch,
            base,
            no_assign,
            no_activate,
            prompt,
            model,
            effort,
            profile,
            json,
        } => {
            let branch = match branch
                .map(|b| b.trim().to_string())
                .filter(|b| !b.is_empty())
            {
                Some(b) => b,
                None => {
                    let item = tasks::work_item(&ctx, id).await.map_err(|e| match e {
                        AdoError::NotFound => anyhow!("Work item {id} was not found."),
                        e => ado_err(e),
                    })?;
                    suggest_branch(&my_initials(&ctx.path).await, &item.title)
                }
            };
            let spawn = (!no_agent).then(|| NewAgent {
                worktree: String::new(),
                kind: agent.unwrap_or_default(),
                profile,
                prompt,
                model,
                effort,
            });
            let ws = daemon::DaemonWorkspace::connect(&ctx).await?;
            let req = StartTask {
                id,
                branch: branch.clone(),
                base,
                assign_to_me: !no_assign,
                activate: !no_activate,
                spawn,
            };
            let started = tasks::start_task(&ctx, &ws, req).await.map_err(cmd_err)?;
            let path = ws.worktree_path(&started.worktree_id);
            Ok(if json {
                serde_json::to_string_pretty(&json!({
                    "branch": branch,
                    "worktreeId": started.worktree_id,
                    "worktreePath": path,
                    "agentId": started.agent_id,
                    "problems": started.problems,
                }))?
            } else {
                format_started(&branch, path.as_deref(), &started)
            })
        }
        AdoCommand::Prs { json } => {
            let ctx = PrCtx::new(ctx).await.map_err(ado_err)?;
            let me = prs::my_id(&ctx.ado.client).await.map_err(ado_err)?;
            let (mine, review) = prs::list(&ctx.ado.client, &ctx.repo_id, &me)
                .await
                .map_err(ado_err)?;
            Ok(if json {
                serde_json::to_string_pretty(&json!({ "mine": mine, "review": review }))?
            } else {
                format_prs(&mine, &review)
            })
        }
        AdoCommand::Pr {
            id,
            json,
            action: None,
        } => {
            let id = id.ok_or_else(|| anyhow!("Which pull request? `kete ado pr <id>`."))?;
            let ctx = PrCtx::new(ctx).await.map_err(ado_err)?;
            let (pr, threads) = prs::get(&ctx.ado.client, &ctx.repo_id, id)
                .await
                .map_err(|e| match e {
                    AdoError::NotFound => anyhow!("Pull request {id} was not found."),
                    e => ado_err(e),
                })?;
            let policies =
                prs::policies_or_note(&ctx.ado.client, &ctx.project_id, id, pr.is_draft).await;
            Ok(if json {
                serde_json::to_string_pretty(&json!({
                    "pr": pr,
                    "policies": policies,
                    "threads": threads,
                }))?
            } else {
                format_pr(&pr, &policies, &threads)
            })
        }
        AdoCommand::Pr {
            action:
                Some(AdoPrAction::Create {
                    title,
                    summary,
                    testing,
                    target,
                    source,
                    task,
                    no_task,
                    reviewers,
                    no_reviewer,
                    publish,
                    json,
                }),
            ..
        } => {
            let compose = Compose {
                title,
                summary: summary.unwrap_or_default(),
                testing: testing.unwrap_or_default(),
                target,
                source,
                task,
                no_task,
                reviewers,
                no_reviewer,
                draft: !publish,
            };
            let (url, result) = create_pr(ctx, compose).await?;
            Ok(if json {
                serde_json::to_string_pretty(&json!({ "url": url, "result": result }))?
            } else {
                format_created(&url, &result)
            })
        }
        AdoCommand::Vote { id, vote } => {
            let ctx = PrCtx::new(ctx).await.map_err(ado_err)?;
            prs::vote(&ctx, id, vote).await.map_err(ado_err)?;
            Ok(format!("Voted {} on !{id}.", prs::vote_label(vote)))
        }
        AdoCommand::Reply { id, thread, text } => {
            let text = text.join(" ");
            if text.trim().is_empty() {
                bail!("The reply is empty.");
            }
            let ctx = PrCtx::new(ctx).await.map_err(ado_err)?;
            prs::reply(&ctx, id, thread, text.trim())
                .await
                .map_err(ado_err)?;
            Ok(format!("Replied on !{id}, thread {thread}."))
        }
    }
}

/// The ADO context of the checkout the command runs in.
async fn cwd_context(state: &AdoState) -> Result<AdoContext> {
    let cwd = std::env::current_dir()?;
    context_at(state, &cwd).await
}

async fn context_at(state: &AdoState, dir: &Path) -> Result<AdoContext> {
    let top = kete_ado::git_out(dir, &["rev-parse", "--show-toplevel"])
        .await
        .map_err(|e| anyhow!("Not inside a git checkout: {e}"))?;
    let top = PathBuf::from(top.trim());
    state
        .silent_context(&top)
        .await
        .map_err(|e| anyhow!(e))?
        .ok_or_else(|| anyhow!("This repo has no Azure DevOps remote."))
}

/// An ADO failure in words, with the way to sign in when that is the cause.
pub fn ado_err(e: AdoError) -> anyhow::Error {
    let kind = e.kind();
    explain(kind, e.user_message())
}

fn cmd_err(e: CmdError) -> anyhow::Error {
    explain(e.kind, e.message)
}

fn explain(kind: ErrorKind, message: String) -> anyhow::Error {
    if kind == ErrorKind::Auth {
        anyhow!("{message}\n{SIGN_IN_HINT}")
    } else {
        anyhow!(message)
    }
}

async fn my_initials(repo: &Path) -> String {
    let repo = repo.to_path_buf();
    tokio::task::spawn_blocking(move || kete_ado::branches::my_initials(&repo))
        .await
        .unwrap_or_default()
}

/// `{initials}/{slug}` for a task, or the slug alone with no initials: the
/// desktop's Start task suggestion.
pub fn suggest_branch(initials: &str, title: &str) -> String {
    let slug = kete_ado::naming::slug(title);
    if initials.is_empty() {
        slug
    } else {
        format!("{initials}/{slug}")
    }
}

// ---- pull request create ----

/// What `kete ado pr create` was asked for.
#[derive(Debug, Clone, Default)]
pub struct Compose {
    pub title: Option<String>,
    pub summary: String,
    pub testing: String,
    pub target: Option<String>,
    pub source: Option<String>,
    pub task: Option<u32>,
    pub no_task: bool,
    pub reviewers: Vec<String>,
    pub no_reviewer: bool,
    pub draft: bool,
}

/// The composer's description: Summary, Changes, Testing and Task, each a
/// `##` section, as the desktop writes it.
pub fn describe(summary: &str, changes: &str, testing: &str, task: &str) -> String {
    let part = |h: &str, body: &str| {
        if body.trim().is_empty() {
            format!("## {h}\n")
        } else {
            format!("## {h}\n{}\n", body.trim())
        }
    };
    [
        part("Summary", summary),
        part("Changes", changes),
        part("Testing", testing),
        part("Task", task),
    ]
    .join("\n")
}

/// The composer's checks before anything is pushed: a task unless
/// `no_task`, a reviewer unless `no_reviewer`.
pub fn preflight(
    task: Option<u32>,
    no_task: bool,
    reviewers: usize,
    no_reviewer: bool,
) -> Result<()> {
    let mut missing = Vec::new();
    if !no_task && task.is_none() {
        missing.push("link a task (--task <id>, or --no-task)");
    }
    if !no_reviewer && reviewers == 0 {
        missing.push("add a reviewer (--reviewer <name or email>, or --no-reviewer)");
    }
    if missing.is_empty() {
        Ok(())
    } else {
        bail!("Before a pull request: {}.", missing.join("; "))
    }
}

/// The one identity `query` names: an exact email or name match among
/// `found`, else the only result. Several or none is an error naming them.
pub fn pick_reviewer(query: &str, found: &[Reviewer]) -> Result<Reviewer> {
    let q = query.trim().to_lowercase();
    let exact: Vec<&Reviewer> = found
        .iter()
        .filter(|r| r.email.to_lowercase() == q || r.name.to_lowercase() == q)
        .collect();
    match (exact.as_slice(), found) {
        ([one], _) => Ok((*one).clone()),
        (_, [one]) => Ok(one.clone()),
        (_, []) => bail!("No Azure DevOps identity matches `{query}`."),
        _ => bail!(
            "`{query}` matches several people: {}. Give the email.",
            found
                .iter()
                .map(|r| format!("{} <{}>", r.name, r.email))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

async fn resolve_reviewer(ctx: &PrCtx, query: &str) -> Result<Reviewer> {
    let recent = ctx
        .ado
        .store
        .read_repo(&ctx.ado.key, |s| s.recent_reviewers.clone());
    let q = query.trim().to_lowercase();
    if let Some(r) = recent
        .iter()
        .find(|r| r.email.to_lowercase() == q || r.name.to_lowercase() == q)
    {
        return Ok(r.clone());
    }
    let found = prs::search_identities(ctx, query.trim())
        .await
        .map_err(ado_err)?;
    pick_reviewer(query, &found)
}

/// Push the branch and open its PR, as the desktop's composer does. The
/// PR's web URL and the outcome.
async fn create_pr(ado: AdoContext, c: Compose) -> Result<(String, prs::CreateResult)> {
    let source = match c
        .source
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    {
        Some(s) => s,
        None => {
            let head = kete_ado::git_out(&ado.path, &["symbolic-ref", "--short", "-q", "HEAD"])
                .await
                .unwrap_or_default();
            let head = head.trim().to_string();
            if head.is_empty() {
                bail!("No branch is checked out here: pass --source <branch>.");
            }
            head
        }
    };
    let seed = prs::compose_seed(&ado, &source).await;
    let task = if c.no_task {
        None
    } else {
        c.task.or(seed.task)
    };
    preflight(task, c.no_task, c.reviewers.len(), c.no_reviewer)?;
    let ctx = PrCtx::new(ado).await.map_err(ado_err)?;
    let task_title = match task {
        Some(id) if seed.task == Some(id) && seed.task_title.is_some() => seed.task_title.clone(),
        Some(id) => tasks::work_item(&ctx.ado, id).await.ok().map(|w| w.title),
        None => None,
    };
    let title = c
        .title
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .or_else(|| task_title.clone().filter(|t| !t.is_empty()))
        .or_else(|| (seed.subjects.len() == 1).then(|| seed.subjects[0].clone()))
        .ok_or_else(|| anyhow!("Add a title: --title \"...\"."))?;
    let mut reviewers = Vec::new();
    if !c.no_reviewer {
        for q in &c.reviewers {
            reviewers.push(resolve_reviewer(&ctx, q).await?);
        }
    }
    let task_line = if c.no_task {
        "No task (n/a)".to_string()
    } else {
        task.map(|id| {
            format!("#{id} {}", task_title.unwrap_or_default())
                .trim()
                .to_string()
        })
        .unwrap_or_default()
    };
    let changes = seed
        .subjects
        .iter()
        .map(|s| format!("- {s}"))
        .collect::<Vec<_>>()
        .join("\n");
    let target = c
        .target
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| {
            if seed.target.is_empty() {
                "main".into()
            } else {
                seed.target.clone()
            }
        });
    let pr = prs::NewPr {
        source,
        target,
        title,
        description: describe(&c.summary, &changes, &c.testing, &task_line),
        draft: c.draft,
        reviewers: reviewers.clone(),
        work_item: task,
    };
    let out = prs::create(&ctx, pr).await.map_err(ado_err)?;
    // An existing PR was not made with these reviewers: nothing to remember.
    if !matches!(out.outcome, prs::CreateOutcome::Existing(_)) {
        for r in reviewers.into_iter().rev() {
            ctx.ado.store.remember_reviewer(&ctx.ado.key, r);
        }
    }
    let id = match &out.outcome {
        prs::CreateOutcome::Created(id)
        | prs::CreateOutcome::CreatedTaskNotLinked(id, _)
        | prs::CreateOutcome::Existing(id) => *id,
    };
    let url = format!(
        "{}/{}/_git/{}/pullrequest/{id}",
        ctx.ado.remote.org_url.trim_end_matches('/'),
        kete_ado::enc(&ctx.ado.remote.project),
        kete_ado::enc(&ctx.ado.remote.repo)
    );
    Ok((url, out))
}

// ---- text ----

fn item_line(out: &mut String, indent: &str, w: &WorkItem) {
    let who = w
        .assigned_to
        .as_deref()
        .map(|a| format!("  ({a})"))
        .unwrap_or_default();
    let _ = writeln!(
        out,
        "{indent}{} {}  [{}]  {}{who}",
        w.kind, w.id, w.state, w.title
    );
}

pub fn format_tasks(list: &TaskList) -> String {
    let mut out = String::new();
    match &list.iteration {
        Some(it) => {
            let _ = writeln!(out, "Sprint: {}", it.name);
        }
        None => {
            let _ = writeln!(out, "No current sprint.");
        }
    }
    if let (Some(at), Some(why)) = (list.cached_at, list.cached_reason.as_deref()) {
        let _ = writeln!(out, "(saved list from {at} ms: {why})");
        if list.cached_because == Some(ErrorKind::Auth) {
            let _ = writeln!(out, "{SIGN_IN_HINT}");
        }
    }
    if list.stale {
        if let Some(next) = &list.next {
            let _ = writeln!(out, "(the sprint has ended; next is {})", next.name);
        }
    }
    if list.groups.iter().all(|g| g.items.is_empty()) {
        let _ = writeln!(out, "No tasks.");
        return out;
    }
    for g in &list.groups {
        out.push('\n');
        match &g.parent {
            Some(p) => item_line(&mut out, "", p),
            None => {
                let _ = writeln!(out, "No parent");
            }
        }
        for w in &g.items {
            item_line(&mut out, "  ", w);
        }
    }
    let _ = writeln!(
        out,
        "\nStart one: kete ado start-task <id>   (branch + worktree + agent)"
    );
    out
}

fn pr_line(out: &mut String, p: &PrSummary) {
    let draft = if p.is_draft { "draft " } else { "" };
    let votes = p
        .reviewers
        .iter()
        .filter(|r| r.vote != 0)
        .map(|r| format!("{} {}", r.name, r.label))
        .collect::<Vec<_>>()
        .join(", ");
    let _ = writeln!(
        out,
        "  !{} {draft}{} -> {}  {}{}",
        p.id,
        p.source,
        p.target,
        p.title,
        if votes.is_empty() {
            String::new()
        } else {
            format!("  [{votes}]")
        }
    );
}

pub fn format_prs(mine: &[PrSummary], review: &[PrSummary]) -> String {
    let mut out = String::new();
    for (heading, list) in [("Mine", mine), ("Waiting on my review", review)] {
        let _ = writeln!(out, "{heading} ({})", list.len());
        for p in list {
            pr_line(&mut out, p);
        }
    }
    out
}

pub fn format_pr(pr: &PrSummary, policies: &[prs::PolicyRow], threads: &[prs::Thread]) -> String {
    let mut out = String::new();
    let state = if pr.is_draft {
        "draft"
    } else {
        pr.status.as_str()
    };
    let _ = writeln!(out, "!{} {}  ({state})", pr.id, pr.title);
    let _ = writeln!(out, "{} -> {}   by {}", pr.source, pr.target, pr.created_by);
    let _ = writeln!(out, "{}", pr.url);
    if !pr.work_items.is_empty() {
        let ids: Vec<String> = pr.work_items.iter().map(|i| format!("#{i}")).collect();
        let _ = writeln!(out, "Work items: {}", ids.join(", "));
    }
    let _ = writeln!(out, "\nReviewers");
    for r in &pr.reviewers {
        let req = if r.required { " (required)" } else { "" };
        let _ = writeln!(out, "  {}{req}: {}", r.name, r.label);
    }
    let _ = writeln!(out, "\nPolicies");
    for p in policies {
        let blocking = if p.blocking { "" } else { " (optional)" };
        let _ = writeln!(out, "  {:?}: {}{blocking}", p.state, p.name);
    }
    let _ = writeln!(out, "\nThreads ({})", threads.len());
    for t in threads {
        let at = match (&t.file, t.line) {
            (Some(f), Some(l)) => format!(" {f}:{l}"),
            (Some(f), None) => format!(" {f}"),
            _ => String::new(),
        };
        let _ = writeln!(out, "  thread {} [{}]{at}", t.id, t.status);
        for c in &t.comments {
            let first = c.text.lines().next().unwrap_or_default();
            let _ = writeln!(out, "    {}: {first}", c.author);
        }
    }
    out
}

fn format_started(branch: &str, path: Option<&str>, s: &StartedTask) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "Started {branch} in worktree {}{}",
        s.worktree_id,
        path.map(|p| format!(" ({p})")).unwrap_or_default()
    );
    if let Some(a) = &s.agent_id {
        let _ = writeln!(out, "Agent {a} is working on it.");
    }
    for p in &s.problems {
        let _ = writeln!(out, "  {}: {}", p.step, p.message);
    }
    out
}

fn format_created(url: &str, r: &prs::CreateResult) -> String {
    let mut out = match &r.outcome {
        prs::CreateOutcome::Created(id) => format!("Opened !{id}: {url}"),
        prs::CreateOutcome::CreatedTaskNotLinked(id, why) => {
            format!("Opened !{id}: {url}\nThe task was not linked: {why}")
        }
        prs::CreateOutcome::Existing(id) => {
            format!("An active pull request from this branch already exists: !{id} {url}")
        }
    };
    if let Some(note) = &r.note {
        out.push('\n');
        out.push_str(note);
    }
    out
}

#[cfg(test)]
mod tests;
