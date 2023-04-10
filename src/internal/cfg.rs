use crate::internal::common::*;
use anyhow::{anyhow, Context};
use serde::Deserialize;
use std::{
    fmt::Display,
    {collections::BTreeMap, fs},
};

/// Variables in configuration files
const CFG_VAR_ARCH: &str = "$arch";
const CFG_VAR_REPO: &str = "$repo";
const CFG_VAR_DB: &str = "$db";

/// File and directory names
const CFG_REPOS_FILE: &str = "repos.conf";
const CFG_FILE_PATH: &str = "/etc/repman.conf";

/// To store of configuration file
#[derive(Debug, Deserialize)]
pub struct Cfg {
    pub vcs_suffixes: Vec<String>,
}

/// Retrieves repman config from configuration file
pub fn cfg() -> anyhow::Result<Cfg> {
    toml::from_str(
        &fs::read_to_string(CFG_FILE_PATH).with_context(|| "Cannot read configuration file")?,
    )
    .with_context(|| "Cannot parse configuration file")
}

// To store content for one repository from repositories configuration file
#[derive(Clone, Debug, Deserialize)]
pub struct CfgRepo {
    #[serde(alias = "DBName")]
    pub db_name: Option<String>,
    #[serde(alias = "Server")]
    pub server: String,
    #[serde(alias = "SignDB")]
    pub sign_db: bool,
}

// To store content from repositories configuration file
pub type CfgRepos = BTreeMap<String, CfgRepo>;

pub fn repo<S>(name: S) -> anyhow::Result<CfgRepo>
where
    S: AsRef<str> + Display,
{
    repos()?
        .get(name.as_ref())
        .ok_or_else(|| anyhow!("Repository {} is not configured", name))
        .cloned()
}

/// Retrieves repository configurations from the configuration file and returns
/// them as B-tree map
pub fn repos() -> anyhow::Result<CfgRepos> {
    let err_msg = "Cannot read repositories configuration file";

    let mut repos: CfgRepos = toml::from_str(
        &fs::read_to_string(config_dir().context(err_msg)?.join(CFG_REPOS_FILE))
            .context(err_msg)?,
    )
    .context("Cannot parse configuration file")?;

    // Replace variables for architecture, repository name and
    // (if specified) DB name with their corresponding values
    for (name, repo) in repos.iter_mut() {
        repo.server = repo
            .server
            .replace(CFG_VAR_ARCH, &arch()?.to_string())
            .replace(CFG_VAR_REPO, name);
        if let Some(db_name) = &repo.db_name {
            repo.server = repo.server.replace(CFG_VAR_DB, db_name)
        }
    }

    Ok(repos)
}
