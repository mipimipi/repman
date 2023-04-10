use crate::internal::common::*;
use anyhow::{anyhow, Context};
use arch_msgs::*;
use once_cell::sync::OnceCell;
use serde::Deserialize;
use std::{collections::BTreeMap, fs};
use toml::Value;

/// Keys used in configuration file
const CFG_KEY_SERVER: &str = "Server";
const CFG_KEY_DB_NAME: &str = "DBName";
const CFG_KEY_SIGN_DB: &str = "SignDB";

/// Variables in configuration files
const CFG_VAR_ARCH: &str = "$arch";
const CFG_VAR_REPO: &str = "$repo";
const CFG_VAR_DB: &str = "$db";

/// File and directory names
const CFG_REPOS_FILE: &str = "repos.conf";
const CFG_FILE_PATH: &str = "/etc/repman.conf";

/// Content of configuraion file
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

/// Represents one repository entry of the repositories configuration file
#[derive(Clone, Debug)]
pub struct CfgRepo {
    pub db_name: Option<String>, // Name of the repository DB (defaults to Name)
    pub server: String,          // Server url of the repository
    pub sign_db: bool,           // Sign the repo DB
}

pub type CfgRepos = BTreeMap<String, CfgRepo>;

/// Retrieves repository configurations from the configuration file and stores
/// them in a repository B-tree map, which is returned.
/// Reading of the configuration file takes only place once. The result is
/// buffered in a static variable.
pub fn repos() -> anyhow::Result<&'static CfgRepos> {
    static REPOS: OnceCell<CfgRepos> = OnceCell::new();

    REPOS.get_or_try_init(|| {
	let err_msg = "Cannot read repositories configuration file".to_string();
        let mut repos = CfgRepos::new();
	let config_file = config_dir().with_context(||err_msg.clone())?.join(CFG_REPOS_FILE);

	match fs::read_to_string(config_file)
	    .with_context(|| err_msg.clone())?
	    .parse::<Value>()
	    .with_context(|| "Cannot parse repositories configuration file")? {
            Value::Table(t) => {
                for (name, data) in t.iter() {
                    let mut server: String = "".to_string();
                    let mut db_name: Option<String> = None;
                    let mut sign_db: bool = false;
                    match &data {
                        Value::Table(t) => {
                            for (k, v) in t.iter() {
                                match k.as_ref() {
                                    CFG_KEY_SERVER => {
                                        if let Value::String(s) = v {
					    server = s.to_string();
                                        } else {
                                            return Err(anyhow!(
                                                "Server URL of repository '{name}' has incorrect structure"
                                            ));
                                        }
                                    }
                                    CFG_KEY_DB_NAME => {
                                        if let Value::String(s) = v {
                                            db_name = Some(s.to_string())
                                        } else {
                                            return Err(anyhow!(
                                                "DBName field of repository '{name}' has incorrect structure"
                                            ));
                                        }
                                    }
                                    CFG_KEY_SIGN_DB => {
                                        if let Value::Boolean(b) = v {
                                            sign_db = *b
                                        } else {
                                            return Err(anyhow!(
                                                "SignDB flag of repository '{name}' has incorrect structure"
                                            ));
                                        }
                                    }
                                    &_ => {
                                        warning!("Unknown field '{k}' in configuration file");
                                        continue
				    }
				};
                            }
                        }
                        _ => {
                            return Err(anyhow!(
                                "Configuration of repository '{name}' has incorrect structure"
                            ));
                        }
                    }
		    // Replace variables for architecture, repository name and
		    // (if specified) DB name with their corresponding values
                    server = server.replace(CFG_VAR_ARCH, &arch()?.to_string()).replace(CFG_VAR_REPO, name);
		    if let Some(s) = &db_name {
			server = server.replace(CFG_VAR_DB, s)
		    }
                    repos.insert(name.to_string(), CfgRepo{
			server,
			db_name,
			sign_db
		    });
                }
            }
            _ => {
                return Err(anyhow!("Configuration file has incorrect structure"));
            }
        }

        Ok(repos)
    })
}
