//! Azure DevOps: remote parsing, branch naming, tokens, the REST client
//! and the `ado.json` store. The Tauri commands that use these land later.
#![allow(dead_code)]

pub mod auth;
pub mod client;
pub mod naming;
pub mod remote;
