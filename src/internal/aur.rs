use alpm::vercmp;
use anyhow::{anyhow, Context};
use arch_msgs::*;
use const_format::concatcp;
use duct::cmd;
use once_cell::sync::OnceCell;
use std::{
    cmp::Eq,
    collections::HashMap,
    fmt::Display,
    hash::Hash,
    path::{Path, PathBuf},
    str::from_utf8,
};

/// AUR URI's
const AUR_URI: &str = "https://aur.archlinux.org/";
const AUR_INFO_URI: &str = concatcp!(AUR_URI, "rpc/?v=5&type=info");

/// AurHeader and AurItem to store the result data of an AUR web api call
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
///
/// ```
/// pkg_name1 -> pkg_base1
/// pkg_name2 -> pkg_base1
/// ```
type PkgName2Base = HashMap<String, String>;

/// Relevant package info from AUR. The package base is used as key for
/// `AurPkgInfos`
struct PkgInfo {
    pkg_base: String,
    version: String,
}
type PkgInfos = HashMap<String, PkgInfo>;

/// Types and variables to store data retrieve from the AUR web interface. Since
/// each execution of a repman sub command requires to retrieve this data only
/// once, the data is stored in static (global) variables.
/// Two data structures are used:
/// - A mapping between package names and its corresponding packages bases and
/// - a hash map with the info from AUR per package base
struct AurData(PkgName2Base, PkgInfos);
static AUR_DATA: OnceCell<AurData> = OnceCell::new();

/// Retrieves information from AUR for packages with names contained in
/// pkg_names. The data is stored in global variables. If `check_exists` is
/// true, an error messages are printed for packages that could be found
/// in AUR
pub fn try_init<S>(pkg_names: &[S], check_exists: bool) -> anyhow::Result<()>
where
    S: AsRef<str> + Display + Eq + Hash,
{
    let mut pkg_infos = PkgInfos::new();
    let mut pkg_name2base = PkgName2Base::new();

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

        // Convert AUR infos into result hash maps:
        // - aur_pkg_infos only contains information on base package level. I.e.,
        //   it contains one item per base package
        // - pkg_name2base contains a mapping between package names and their
        //   corresponding package bases. I.e., in case of split packages their
        //   could be entries like so:
        //     pkg_name1 -> pkg_base1
        //     pkg_name2 -> pkg_base1
        //   aur_pkg_infos would only contain an entry for pkg_base1
        for item in &response.json::<AurHeader>().with_context(|| err_msg)?.items {
            pkg_name2base.insert(item.name.clone(), item.pkg_base.clone());

            if !pkg_infos.contains_key(&item.pkg_base) {
                pkg_infos.insert(
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
                if !pkg_name2base.contains_key(pkg_name.as_ref()) {
                    error!(
                        "No information could not be retrieved from AUR for package {}",
                        pkg_name
                    );
                }
            }
        }
    }

    AUR_DATA
        .set(AurData(pkg_name2base, pkg_infos))
        .unwrap_or_else(|_| panic!("Cannot initialize AUR data"));

    Ok(())
}

/// Access functions to global AUR data
fn aur_data() -> &'static AurData {
    AUR_DATA
        .get()
        .unwrap_or_else(|| panic!("Cannot access AUR data"))
}
fn pkg_name2base() -> &'static PkgName2Base {
    &aur_data().0
}
fn pkg_infos() -> &'static PkgInfos {
    &aur_data().1
}

/// Clones a package repository from AUR to dir
fn clone_pkg_repo<P, S>(pkg_base: S, dir: P) -> anyhow::Result<PathBuf>
where
    P: AsRef<Path>,
    S: AsRef<str> + Display,
{
    let err_msg = format!("Cannot clone package '{}' from AUR", pkg_base);

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

/// Clones package repositories to dir. If `pkg_names` is Some(...) only packages
/// are cloned whose names are contained in `Some(pkg_names)`. Otherwise, all
/// package repositories are cloned where the package base is part of `AUR_DATA`
pub fn clone_pkg_repos<P, S>(pkg_names: Option<&[S]>, dir: P) -> Vec<PathBuf>
where
    P: AsRef<Path>,
    S: AsRef<str> + Display + Eq + Hash,
{
    let to_be_cloned_pkg_names: Vec<&str> = match pkg_names {
        Some(pkg_names) => pkg_names
            .iter()
            .filter_map(|pkg_name| {
                if pkg_infos().contains_key(pkg_name.as_ref()) {
                    Some(pkg_name.as_ref())
                } else {
                    None
                }
            })
            .collect(),
        None => pkg_infos().keys().map(AsRef::as_ref).collect(),
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

/// Information about package updates
pub struct PkgUpd<'a> {
    pub name: &'a str,
    pub old_version: &'a str,
    pub new_version: &'a str,
    pub pkg_base: &'a str,
}

/// Determines relevant updates from AUR for packages with names in pkg_names.
/// db_pkgs contains information about all packages currently contained in the
/// repository DB.
/// Update information is returned as a vector of a struct consisting of:
/// - package name,
/// - version currently contained in repository DB
/// - version currently available in AUR (which is of course greater than their
///   other version)
/// - package base
/// Package base is required to be able to clone the package repository lateron
pub fn pkg_updates<'a>(db_pkgs: &'static repodb_parser::PkgMap) -> anyhow::Result<Vec<PkgUpd<'a>>> {
    let mut pkg_upds: Vec<PkgUpd> = vec![];

    let pkg_infos = pkg_infos();
    let pkg_name2base = pkg_name2base();

    for (pkg_name, pkg_base) in pkg_name2base {
        let db_pkg = db_pkgs
            .get(pkg_name)
            .unwrap_or_else(|| panic!("Could not get package data from repository DB"));
        let pkg_info = pkg_infos
            .get(pkg_base)
            .unwrap_or_else(|| panic!("Could not get package information retrieved from AUR"));

        if vercmp(db_pkg.version.as_str(), pkg_info.version.as_str()) == core::cmp::Ordering::Less {
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
