// SPDX-FileCopyrightText: 2019-2024 Michael Picht <mipi@fsfe.org>
//
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::internal::{cfg, common::*};
use alpm::vercmp;
use anyhow::{anyhow, Context};
use arch_msgs::*;
use const_format::concatcp;
use duct::cmd;
use regex::Regex;
use std::{
    cmp::Eq,
    collections::HashMap,
    fmt::Display,
    hash::Hash,
    path::{Path, PathBuf},
    str::from_utf8,
};

/// Names of optional dependencies
const PKG_NAME_GIT: &str = "git";

/// AUR URI's
const AUR_URI: &str = "https://aur.archlinux.org/";
const AUR_INFO_URI: &str = concatcp!(AUR_URI, "rpc/?v=5&type=info");

/// Structures to store the result of an AUR web api call
#[derive(serde::Deserialize, Debug, Default)]
#[serde(default)]
struct AurHeader {
    #[serde(rename = "results")]
    items: Vec<AurItem>,
}
#[derive(serde::Deserialize, Debug, Default)]
#[serde(default)]
struct AurItem {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "PackageBase")]
    pkg_base: String,
    #[serde(rename = "Version")]
    version: String,
    #[serde(rename = "OutOfDate")]
    out_of_date: Option<u32>,
}

/// Mapping between package names and the corresponding packages bases. In case
/// of a [split package](https://man.archlinux.org/man/PKGBUILD.5#PACKAGE_SPLITTING),
/// the mapping could be like so:
///     pkg_name1 -> pkg_base1
///     pkg_name2 -> pkg_base1
pub type PkgName2Base = HashMap<String, String>;

/// Ppackage info from AUR
struct PkgInfo {
    pkg_base: String,
    version: String,
}
type PkgInfos = HashMap<String, PkgInfo>;

/// Information about package updates
pub struct PkgUpd<'a> {
    pub name: &'a str,
    pub old_version: &'a str,
    pub new_version: &'a str,
    pub pkg_base: &'a str,
}

/// Types and variables to store data retrieve from the AUR web interface.
/// Two data structures are used:
/// - pkg_infos only contains information on base package level. I.e., it
///   contains one item per base package
/// - pkg_name2base contains a mapping between package names and their
///   corresponding package bases. I.e., in case of split packages their
///   could be entries like so:
///   pkg_name1 -> pkg_base1
///   pkg_name2 -> pkg_base1
///   In this case pkg_infos would only contain an entry for pkg_base1
pub struct AurData {
    pkg_name2base: PkgName2Base,
    pkg_infos: PkgInfos,
}

impl AurData {
    /// Creates an instance of AurData and retrieves information from AUR about
    /// the packages in pkg_names. If check_exists is true, error messages are
    /// printed for packages that could not be found in AUR
    pub fn new<S>(pkg_names: &[S], check_exists: bool) -> anyhow::Result<AurData>
    where
        S: AsRef<str> + Display + Eq + Hash,
    {
        let mut aur_data = AurData {
            pkg_name2base: PkgName2Base::new(),
            pkg_infos: PkgInfos::new(),
        };

        if !pkg_names.is_empty() {
            let err_msg = "Cannot retrieve package information from AUR".to_string();

            // Assemble URI
            let mut aur_uri: String = AUR_INFO_URI.to_string();
            for pkg_name in pkg_names {
                aur_uri = format!("{}&arg[]={}", aur_uri, pkg_name);
            }

            // Request package information from AUR
            let response = reqwest::blocking::get(aur_uri).with_context(|| err_msg.clone())?;
            if response.status() != reqwest::StatusCode::OK {
                return Err(anyhow!("HTTP error from AUR: {}", response.status()).context(err_msg));
            }

            for item in &response.json::<AurHeader>().with_context(|| err_msg)?.items {
                aur_data
                    .pkg_name2base
                    .insert(item.name.clone(), item.pkg_base.clone());

                if !aur_data.pkg_infos.contains_key(&item.pkg_base) {
                    aur_data.pkg_infos.insert(
                        item.pkg_base.clone(),
                        PkgInfo {
                            pkg_base: item.pkg_base.clone(),
                            version: item.version.clone(),
                        },
                    );

                    // Warn in case package is out-of-date
                    if item.out_of_date.is_some() {
                        warning!("AUR package '{}' is flagged as out-of-date", &item.name);
                    }
                }
            }

            if check_exists {
                // Print error messages for packages that could not be retrieved from AUR
                for pkg_name in pkg_names {
                    if !aur_data.pkg_name2base.contains_key(pkg_name.as_ref()) {
                        error!(
                            "No information could not be retrieved from AUR for package {}",
                            pkg_name
                        );
                    }
                }
            }
        }

        Ok(aur_data)
    }

    /// Clones package repositories to dir. If pkg_names is Some(...) only
    /// packages are cloned whose names are contained in Some(pkg_names).
    /// Otherwise, all package repositories are cloned where the package base is
    /// part of self.pkg_infos
    pub fn clone_pkg_repos<P, S>(&self, pkg_names: Option<&[S]>, dir: P) -> Vec<PathBuf>
    where
        P: AsRef<Path>,
        S: AsRef<str> + Display + Eq + Hash,
    {
        let to_be_cloned_pkg_names: Vec<&str> = match pkg_names {
            Some(pkg_names) => pkg_names
                .iter()
                .filter_map(|pkg_name| {
                    if self.pkg_infos.contains_key(pkg_name.as_ref()) {
                        Some(pkg_name.as_ref())
                    } else {
                        None
                    }
                })
                .collect(),
            None => self.pkg_infos.keys().map(AsRef::as_ref).collect(),
        };

        let mut pkg_repo_dirs: Vec<PathBuf> = vec![];
        for pkg_name in to_be_cloned_pkg_names {
            match clone_pkg_repo(pkg_name, &dir) {
                Ok(dir) => {
                    pkg_repo_dirs.push(dir);
                }
                Err(err) => {
                    error!("{:?}", err);
                }
            }
        }

        pkg_repo_dirs
    }

    /// Filter packages that are not tied to a specific version from all
    /// packages. These packages are identified by their suffix. If their
    /// name ends with one of the VCS suffixes maintained in the repman
    /// configuration files, they are considered being release independent.
    pub fn pkg_name2base_no_version(&self) -> anyhow::Result<Vec<(&str, &str)>> {
        // Create regex from configured VSC suffixes
        let mut re_str = ".+-(".to_string();
        for (i, suffix) in cfg::cfg()
            .context("Cannot determine release independent packages")?
            .vcs_suffixes
            .iter()
            .enumerate()
        {
            if i > 0 {
                re_str.push('|');
            }
            re_str.push_str(suffix);
        }
        re_str.push(')');
        let re = Regex::new(&re_str).unwrap();

        // Filter release independent packages from all packages
        let mut pkgs: Vec<(&str, &str)> = vec![];
        for (pkg_name, pkg_base) in &self.pkg_name2base {
            if re.is_match(pkg_name) {
                pkgs.push((pkg_name, pkg_base));
            }
        }

        Ok(pkgs)
    }

    /// Determines relevant updates from AUR for packages with names in db_pkgs.
    /// db_pkgs contains information about all packages currently contained in the
    /// repository DB.
    /// Update information is returned as a vector of a struct consisting of:
    /// - package name,
    /// - version currently contained in repository DB
    /// - version currently available in AUR (which is of course greater than their
    ///   other version)
    /// - package base
    ///
    /// Package base is required to be able to clone the package repository lateron
    pub fn pkg_updates<'a>(
        &'a self,
        db_pkgs: &'static repodb_parser::Pkgs,
    ) -> anyhow::Result<Vec<PkgUpd<'a>>> {
        let mut pkg_upds: Vec<PkgUpd> = vec![];

        for (pkg_name, pkg_base) in &self.pkg_name2base {
            let db_pkg = db_pkgs
                .get(pkg_name)
                .unwrap_or_else(|| panic!("Could not get package data from repository DB"));
            let pkg_info = self
                .pkg_infos
                .get(pkg_base)
                .unwrap_or_else(|| panic!("Could not get package information retrieved from AUR"));

            if vercmp(db_pkg.version.as_str(), pkg_info.version.as_str())
                == core::cmp::Ordering::Less
            {
                pkg_upds.push(PkgUpd {
                    name: db_pkg.name.as_str(),
                    old_version: db_pkg.version.as_str(),
                    new_version: pkg_info.version.as_str(),
                    pkg_base: pkg_info.pkg_base.as_str(),
                })
            }
        }

        Ok(pkg_upds)
    }
}

/// Clones the package repository for pkg_base from AUR to dir
fn clone_pkg_repo<P, S>(pkg_base: S, dir: P) -> anyhow::Result<PathBuf>
where
    P: AsRef<Path>,
    S: AsRef<str> + Display,
{
    let err_msg = format!("Cannot clone package '{}' from AUR", pkg_base);

    // Package git must be installed to be able to clone packages from AUR
    if !is_pkg_installed(PKG_NAME_GIT).with_context(|| err_msg.clone())? {
        return Err(anyhow!(
            "Cloning a package from AUR requires package {} being installed",
            PKG_NAME_GIT
        ))
        .context(err_msg);
    }

    msg!("Cloning repository of package {} from AUR ...", pkg_base);

    let pkg_repo_dir = dir.as_ref().join(pkg_base.as_ref());

    let output = cmd!(
        "git",
        "clone",
        format!("{}{}.git", AUR_URI, pkg_base),
        &pkg_repo_dir,
    )
    .stdout_null()
    .stderr_capture()
    .unchecked()
    .run()
    .with_context(|| err_msg.clone())?;

    if output.status.success() {
        Ok(pkg_repo_dir)
    } else {
        Err(anyhow!(
            "git clone: {}",
            from_utf8(&output.stderr)
                .unwrap_or_else(|_| panic!("Cannot retrieve stderr for 'git clone ...'"))
        )
        .context(err_msg))
    }
}
