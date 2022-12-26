//! Function, macros, etc. for working on a repository

use crate::internal::{aur, cfg, common::*, deps::Deps, pkg::Pkg, pkgbuild::PkgBuild};
use anyhow::{anyhow, Context};
use arch_msgs::*;
use const_format::concatcp;
use dialoguer::Confirm;
use duct::cmd;
use glob::glob;
use lazy_static::lazy_static;
use once_cell::sync::OnceCell;
use regex::Regex;
use scopeguard::defer;
use std::{
    cmp::Eq,
    env,
    ffi::OsStr,
    fmt::Display,
    fs::{self, File},
    hash::Hash,
    io::{prelude::*, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
    process,
    str::from_utf8,
};
use url::Url;

/// Constants for URL schemes (repman only supports rsync and file)
const SCHEME_FILE: &str = "file";
const SCHEME_RSYNC: &str = "rsync";

/// File suffixes
const DB_SUFFIX: &str = ".db";
const DB_ARCHIVE_SUFFIX: &str = concatcp!(DB_SUFFIX, ".tar.xz");

/// File and directory names
const CHROOT_SUB_PATH: &str = "chroots";
const CHROOT_ROOT_SUB_PATH: &str = "root";
const REPOS_SUB_PATH: &str = "repos";
const PKG_SUB_PATH: &str = "pkg";
const PKGBUILD_SUB_PATH: &str = "pkgbuild";
const ADJUST_CHROOT_FILE_NAME: &str = "adjustchroot";

/// Creates lock file for a repository and registers the removal of such file when
/// leaving the current scope
macro_rules! lock {
    () => {
        lock()?;
        defer! {
            unlock().unwrap_or_else(|_| panic!("Cannot unlock repository {}", name()));
        }
    };
}

/// Executes a code block on the current repository. I.e., in case it is remote,
/// the repository data (DB, packages, etc.) is downloaded, the code is executed
/// on that data, and the changed data is uploaded. In case of a local repository
/// the code block is executed directly on the repository data with copying it
macro_rules! exec_on_repo {
    ($code:block) => {
        download()?;
        $code
        upload()?;
    };
}

/// Generates the directory for temporary data for the current process, registers
/// the removal of that data when leaving the current scope, and executes a code
/// block
macro_rules! exec_with_tmp_data {
    ($code:block) => {
        let _ = ensure_tmp_dir()?;
        defer! {
            fs::remove_dir_all(
		tmp_dir().unwrap_or_else(|_| panic!("Cannot assemble path of temporary directory"))
	    ).unwrap_or_else(|_| panic!("Cannot remove temporary directory for PID '{}'", process::id()));
        }
	$code
    }
}

/// Repository attributes. Since each execution of a repman sub command affects
/// at most one repository, these attributes are static (global) variables
static NAME: OnceCell<String> = OnceCell::new();
static DB_NAME: OnceCell<String> = OnceCell::new();
static SIGN_DB: OnceCell<bool> = OnceCell::new();
static IS_REMOTE: OnceCell<bool> = OnceCell::new();
static REMOTE_DIR: OnceCell<Option<String>> = OnceCell::new();
static LOCAL_DIR: OnceCell<PathBuf> = OnceCell::new();
static CHROOT_DIR: OnceCell<PathBuf> = OnceCell::new();

/// Access functions for attributes of the current repository
fn name() -> &'static str {
    NAME.get().unwrap().as_str()
}
fn db_name() -> &'static str {
    DB_NAME.get().unwrap().as_str()
}
fn sign_db() -> bool {
    *SIGN_DB.get().unwrap()
}
pub fn is_remote() -> bool {
    *IS_REMOTE.get().unwrap()
}
fn remote_dir() -> Option<&'static str> {
    match REMOTE_DIR.get().unwrap() {
        Some(remote_dir) => Some(remote_dir.as_str()),
        None => None,
    }
}
fn local_dir() -> &'static Path {
    LOCAL_DIR.get().unwrap().as_path()
}
fn chroot_dir() -> &'static Path {
    CHROOT_DIR.get().unwrap().as_path()
}

/// Initializes static repository attributes for repository `repo_name` with data
/// retrieved from the repman configuration file
pub fn try_init<S>(repo_name: S) -> anyhow::Result<()>
where
    S: AsRef<str> + Display + Eq + Hash,
{
    match cfg::repos()?.get(repo_name.as_ref()) {
        Some(cfg_repo) => {
            let url = Url::parse(cfg_repo.server.as_str()).with_context(|| {
                format!("Server URL of repository {} could not be parsed", repo_name)
            })?;

            if url.scheme() != SCHEME_FILE && url.scheme() != SCHEME_RSYNC {
                return Err(anyhow!(
                    "Server URL of repository {} has unsupported scheme '{}'",
                    repo_name,
                    url.scheme()
                ));
            }

            NAME.set(repo_name.to_string())
                .unwrap_or_else(|_| panic!("Cannot initialize repository name"));
            DB_NAME
                .set(if let Some(db_name) = &cfg_repo.db_name {
                    db_name.to_string()
                } else {
                    repo_name.to_string()
                })
                .unwrap_or_else(|_| panic!("Cannot initialize DB name of repository"));
            SIGN_DB
                .set(cfg_repo.sign_db)
                .unwrap_or_else(|_| panic!("Cannot initialize sign DB indicator of repository"));
            IS_REMOTE
                .set(url.scheme() == SCHEME_RSYNC)
                .unwrap_or_else(|_| panic!("Cannot initialize remote flag of repository"));
            REMOTE_DIR
                .set(if url.scheme() == SCHEME_FILE {
                    None
                } else {
                    Some(ssh_path_from_url(&url))
                })
                .unwrap_or_else(|_| panic!("Cannot initialize remote directory of repository"));
            LOCAL_DIR
                .set(if url.scheme() == SCHEME_FILE {
                    PathBuf::from(url.path())
                } else {
                    cache_dir()
                        .with_context(|| {
                            format!(
                                "Cannot assemble path of local directory for repository {}",
                                repo_name
                            )
                        })?
                        .join(REPOS_SUB_PATH)
                        .join(repo_name.as_ref())
                })
                .unwrap_or_else(|_| panic!("Cannot initialize local directory of repository"));
            CHROOT_DIR
                .set(
                    cache_dir()
                        .with_context(|| {
                            format!(
                                "Cannot assemble path of chroot directory for repository {}",
                                repo_name
                            )
                        })?
                        .join(CHROOT_SUB_PATH)
                        .join(repo_name.as_ref()),
                )
                .unwrap_or_else(|_| panic!("Cannot initialize chroot directory of repository"));

            // Make sure that local repo directory exists
            ensure_dir(local_dir())?;

            Ok(())
        }
        None => Err(anyhow!("Repository {} is not configured", repo_name)),
    }
}

/// Adds all packages whose names are contained in `pkg_names` to the current
/// repository. If `no_chroot` is true, building the new packages is not done via
/// `makepkg`, otherwise via `makechrootpkg`. If `clean_chroot` is true, the
/// chroot will be removed after all packages have been built. If `sign` is true,
/// the files of the new packages will be signed.
pub fn add<S>(
    aur_pkg_names: &[S],
    pkgbuild_dirs: &[PathBuf],
    no_chroot: bool,
    clean_chroot: bool,
    sign: bool,
) -> anyhow::Result<()>
where
    S: AsRef<str> + Display + Eq + Hash,
{
    let err_msg = format!("Cannot add packages to repository {}", name());

    if sign && gpg_key().is_none() {
        return Err(anyhow!(
            "New packages shall be signed but GPG key is not set"
        ));
    }

    // Initialize AUR information from AUR web interface
    aur::try_init(aur_pkg_names, true).with_context(|| err_msg.clone())?;

    exec_with_tmp_data!({
        // Create tmp dirs for PKGBUILD scripts and package file
        let (pkgbuild_dir, pkg_dir) = ensure_pkg_tmp_dirs().with_context(|| err_msg.clone())?;

        // Collect paths to PKGBUILD scripts ...
        let mut pkgbuilds: Vec<PkgBuild> = vec![];
        // ... from local directories ...
        for pkgbuild in PkgBuild::from_dirs(pkgbuild_dirs).with_context(|| err_msg.clone())? {
            pkgbuilds.push(pkgbuild);
        }
        // ... and by downloading package PKGBUILD files from AUR
        for pkgbuild in PkgBuild::from_aur(Some(aur_pkg_names), pkgbuild_dir)
            .with_context(|| err_msg.clone())?
        {
            pkgbuilds.push(pkgbuild);
        }

        if !pkgbuilds.is_empty() {
            lock!();
            exec_on_repo!({
                // Create (empty) repository DB if no DB exists
                ensure_db().with_context(|| err_msg.clone())?;

                if !no_chroot {
                    // Create or update chroot container
                    prepare_chroot().with_context(|| err_msg.clone())?;
                }

                // Build packages
                let mut built_pkgs: Vec<Pkg> = vec![];
                for pkgbuild in pkgbuilds {
                    match Pkg::build(
                        &pkgbuild,
                        no_chroot,
                        Some(sign),
                        gpg_key(),
                        local_dir(),
                        chroot_dir(),
                        &pkg_dir,
                    ) {
                        Err(err) => {
                            error!("{:?}", err);
                            continue;
                        }
                        Ok(pkgs) => built_pkgs.extend(pkgs),
                    }
                }

                // Add the successfully built packages to respository DB
                add_pkgs_to_db(&built_pkgs).with_context(|| err_msg.clone())?;

                if clean_chroot {
                    remove_chroot_dir().with_context(|| err_msg.clone())?;
                }
            });
        }
    });

    Ok(())
}

/// Add packages to the DB of the current repository
fn add_pkgs_to_db(pkgs: &[Pkg]) -> anyhow::Result<()> {
    if pkgs.is_empty() {
        return Ok(());
    }

    let err_msg = format!("Cannot add packages to DB of repository {}", name());

    // In case the repository is signed but will not be signed after adding
    // packages, the signature file are removed. This is required since
    // `repo-add` does not remove such files
    if !sign_db() && is_db_signed() {
        remove_db_sig_files().with_context(|| err_msg.clone())?;
    }

    if sign_db() && gpg_key().is_none() {
        return Err(
            anyhow!("Repository DB shall be signed but GPG key is not set").context(err_msg),
        );
    }

    // Assemble arguments for repo-add
    let repo_file = local_dir().join(db_name().to_string() + DB_ARCHIVE_SUFFIX);
    let mut args: Vec<&OsStr> = vec![OsStr::new("--remove"), OsStr::new("--verify")];
    if sign_db() {
        args.extend([
            OsStr::new("--sign"),
            OsStr::new("--key"),
            OsStr::new(gpg_key().unwrap_or_else(|| panic!("GPG_KEY is not set"))),
        ]);
    }
    args.push(repo_file.as_os_str());
    args.extend(
        pkgs.iter()
            .map(|pkg| pkg.as_ref().as_os_str())
            .collect::<Vec<&OsStr>>(),
    );

    // Execute repo-add ...
    let output = cmd("repo-add", &args)
        .stdout_null()
        .stderr_capture()
        .unchecked()
        .run()
        .with_context(|| err_msg.clone())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(anyhow!("repo-add: {}", from_utf8(&output.stderr).unwrap()).context(err_msg))
    }
}

/// Determines if a script for adjusting the chroot container of the current
/// repository exists and - if it exists - executes it. This is done in the
/// following sequence (assuming that `~/.config/repman` is the path to the
/// repman config directory):
/// 1) ~/.config/repman/adjustchroot-<REPOSITORY-NAME>
/// 2) ~/.config/repman/adjustchroot
/// The script must be executable
fn adjust_chroot() -> anyhow::Result<Option<PathBuf>> {
    let config_dir = config_dir().with_context(|| {
        format!(
            "Cannot determine if adjustchroot exists for repository {}",
            name()
        )
    })?;
    let paths: [PathBuf; 2] = [
        config_dir.join(ADJUST_CHROOT_FILE_NAME.to_string() + "-" + name()),
        config_dir.join(ADJUST_CHROOT_FILE_NAME),
    ];
    for path in paths {
        if path.exists() {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

/// Returns true if chroot directory for the current rrepository exists,
/// otherwise false
pub fn chroot_exists() -> bool {
    chroot_dir().exists()
}

/// Cleans up the current repository. I.e., checks if the repository DB and the
/// package files are consistent. Removes obsolete artefacts
pub fn clean_up() -> anyhow::Result<()> {
    lock!();
    exec_on_repo!({
        let err_msg = format!("Cannot clean up repository {}", name());
        let db_pkgs = db_pkgs().with_context(|| err_msg.clone())?;

        // Check #1: Do all packages contained in the repository DB have a
        // corresponding package file in the repository directory?
        // -> Remove packages from the DB where that is not the case
        {
            let mut to_be_deleted_pkg_names: Vec<&str> = vec![];
            for db_pkg in db_pkgs.values() {
                if Pkg::from_meta_data(
                    &db_pkg.name,
                    &db_pkg.version,
                    &db_pkg.arch,
                    local_dir(),
                    pkg_ext().with_context(|| err_msg.clone())?,
                )
                .is_err()
                {
                    error!(
                        "Package {} is in repository DB, but package file does not exist",
                        db_pkg.name
                    );
                    to_be_deleted_pkg_names.push(&db_pkg.name);
                }
            }
            if !to_be_deleted_pkg_names.is_empty() {
                remove_pkgs_from_db(&to_be_deleted_pkg_names).with_context(|| err_msg.clone())?;
                msg!("Removed obsolete package entries from repository DB");
            }
        }

        // Check #2: Do all package files in the repository DB have a package
        // entry in the repository DB?
        // -> Remove package files where that is not the case
        {
            let pattern = format!("{}/*-*-*-*{}", local_dir().display(), pkg_ext()?);
            for file in glob(&pattern)
                .unwrap_or_else(|_| panic!("Pattern '{}' is not supported", pattern))
                .flatten()
            {
                if file.is_file() {
                    if let Ok(pkg) = Pkg::try_from(file.clone()) {
                        if !db_pkgs.contains_key(&pkg.name()) {
                            if let Err(err) = fs::remove_file(&file) {
                                error!(
                                    "{:?}",
                                    anyhow!(err).context(format!(
                                        "Cannot remove obsolete package file '{}'",
                                        file.display()
                                    ))
                                );
                            } else {
                                msg!("Removed obsolete package file '{}'", &file.display());
                            }
                        }
                    }
                }
            }
        }

        // Check #3: Do all *.sig files in the repository directory have a
        // corresponding file in that directory?
        // -> Remove *.sig files where that is not the case
        {
            let pattern = format!("{}/*.sig", local_dir().display());
            for sig_file in glob(&pattern)
                .unwrap_or_else(|_| panic!("Pattern '{}' is not supported", pattern))
                .flatten()
            {
                if (sig_file.is_file() || sig_file.is_symlink())
                    && !sig_file.with_extension("").exists()
                {
                    if let Err(err) = fs::remove_file(&sig_file) {
                        error!(
                            "{:?}",
                            anyhow!(err).context(format!(
                                "Cannot remove obsolete signature file '{}'",
                                sig_file.display()
                            ))
                        );
                    } else {
                        msg!("Removed obsolete signature file '{}'", &sig_file.display());
                    }
                }
            }
        }
    });

    Ok(())
}

/// Returns true if the DB of the current repository contains a package with name
/// `pkg_name`
fn contains_pkg<S>(pkg_name: S) -> anyhow::Result<bool>
where
    S: AsRef<str> + Display,
{
    Ok(db_pkgs()
        .with_context(|| {
            format!(
                "Cannot check if repository {} contains package {}",
                name(),
                pkg_name
            )
        })?
        .contains_key(pkg_name.as_ref()))
}

/// Creates a chroot container for the current repository. The chroot is
/// initialized with the packages base-devel and (provided distributed build is
/// configured in the relevant makepkg.conf) distcc.
fn create_chroot() -> anyhow::Result<()> {
    let err_msg = format!("Cannot create chroot for repository {}", name());

    // Create chroot directory if it does not exist
    ensure_dir(chroot_dir()).with_context(|| err_msg.clone())?;

    // Determine path of makepkg.conf file to be used in chroot
    let makepkg_conf = makepkg_conf().with_context(|| err_msg.clone())?;

    // Prepare a pacman.conf file to be used in chroot
    let pacman_conf = pacman_conf_for_chroot().with_context(|| err_msg.clone())?;

    // Determine if distributed build is wanted
    lazy_static! {
        static ref RE_DISTCC: Regex =
            Regex::new(r#"\n[^#]*BUILDENV *= *[^\)]*[^!]+distcc"#).unwrap();
    }
    let content = fs::read_to_string(makepkg_conf).with_context(|| err_msg.clone())?;
    let captures = RE_DISTCC.captures(content.as_str());
    #[allow(clippy::unnecessary_unwrap)]
    let distcc = captures.is_some() && captures.as_ref().unwrap().get(0).is_some();

    msg!("Creating chroot for repository {} ...", name());

    // Assemble arguments for mkarchroot
    let chroot_dir = chroot_dir().join(CHROOT_ROOT_SUB_PATH);
    let mut args: Vec<&OsStr> = vec![
        OsStr::new("-C"),
        pacman_conf.as_os_str(),
        OsStr::new("-M"),
        makepkg_conf.as_os_str(),
        chroot_dir.as_os_str(),
        OsStr::new("base-devel"),
    ];
    if distcc {
        args.push(OsStr::new("distcc"))
    };

    let reader = cmd("mkarchroot", &args)
        .stderr_to_stdout()
        .stderr_capture()
        .reader()
        .with_context(|| err_msg.clone())?;
    for line in BufReader::new(reader).lines() {
        match line {
            Ok(text) => println!("{}", text),
            Err(err) => return Err(anyhow!(err).context(err_msg)),
        }
    }

    // Exec adjust chroot script (if there is any)
    exec_adjust_chroot().with_context(|| err_msg.clone())?;

    // Check if distcc is installed and print warning if that is not the
    // case. Background: For some reason, Arch Linux requires distcc being
    // installed even if the build is done in a chroot container and distcc
    // is already installed in that container
    if distcc && is_pkg_installed("distcc").with_context(|| err_msg)? {
        warning!("Package 'distcc' must be installed on the system since otherwise distributed builds are not possible in the chroot");
    }

    Ok(())
}

/// Returns true is the DB of the current repository exists, false otherwise
fn db_exists() -> bool {
    local_dir().join(db_name().to_string() + DB_SUFFIX).exists()
}

/// Retrieves content from the DB of the current repository. This is only done
/// once. The result is stored in a static variable
fn db_pkgs() -> anyhow::Result<&'static repodb_parser::PkgMap> {
    static DB_PKGS: OnceCell<repodb_parser::PkgMap> = OnceCell::new();
    DB_PKGS.get_or_try_init(|| {
        if !db_exists() {
            return Err(anyhow!("DB of repository {} does not exist", name()));
        }

        repodb_parser::parse(
            local_dir()
                .join(db_name().to_string() + DB_SUFFIX)
                .as_path(),
        )
    })
}

// Retrieves dependencies from DB of the current repository
fn deps() -> anyhow::Result<Deps<'static>> {
    Deps::new(db_pkgs().with_context(|| {
        format!(
            "Cannot retrieve dependencies from DB for repository {}",
            name()
        )
    })?)
}

/// Downloads the files of the current repository to a local directory, if the
/// repository is remote. If the function is called for a local repository, it
/// does not do anything
fn download() -> anyhow::Result<()> {
    // Nothing to do if self is a local repository
    if !is_remote() {
        return Ok(());
    }

    let remote_dir: &str = remote_dir().unwrap();

    msg!(
        "Downloading repository {} from {} ... (this may take a while)",
        name(),
        remote_dir
    );

    let err_msg = format!("Cannot download repository {}", name());

    // Sync changes from remote directory to local cache directory
    let output = cmd!(
        "rsync",
        "-a",
        "-z",
        "--delete",
        format!("{}/", remote_dir),
        local_dir(),
    )
    .stdout_null()
    .stderr_capture()
    .unchecked()
    .run()
    .with_context(|| err_msg.clone())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(anyhow!(from_utf8(&output.stderr).unwrap().to_string()).context(err_msg))
    }
}

/// Create an empty DB for the current repository if no DB exists. A repository
/// DB must exist when `makepkgchroot` is called, even if it is empty
fn ensure_db() -> anyhow::Result<()> {
    let err_msg = format!(
        "Cannot ensure that repository DB exists for repository {}",
        name()
    );

    if db_exists() {
        return Ok(());
    }

    msg!("Creating empty repository DB ...");

    let output = cmd!(
        "repo-add",
        "-n",
        "-R",
        local_dir().join(db_name().to_string() + DB_ARCHIVE_SUFFIX)
    )
    .stdout_null()
    .stderr_capture()
    .unchecked()
    .run()
    .with_context(|| err_msg.clone())?;

    if output.status.success() {
        Ok(())
    } else {
        Err(anyhow!("repo-add: {}", from_utf8(&output.stderr).unwrap()).context(err_msg))
    }
}

/// Creates temporary directories for PKGBUILD files and for package files
/// resulting from build steps
fn ensure_pkg_tmp_dirs() -> anyhow::Result<(PathBuf, PathBuf)> {
    let err_msg = format!(
        "Cannot ensure temporary directories for repository {}",
        name()
    );

    let tmp_dir: _ = ensure_tmp_dir().with_context(|| err_msg.clone())?;

    Ok((
        ensure_dir(&tmp_dir.join(PKGBUILD_SUB_PATH)).with_context(|| err_msg.clone())?,
        ensure_dir(&tmp_dir.join(PKG_SUB_PATH)).with_context(|| err_msg.clone())?,
    ))
}

/// Executes a script to adjust the chroot container if such a script is
/// maintained
fn exec_adjust_chroot() -> anyhow::Result<()> {
    if let Some(adjust_chroot) =
        adjust_chroot().with_context(|| "Cannot check if an adjustchroot script is maintained")?
    {
        let err_msg = format!(
            "Cannot execute '{}' for repository {}",
            adjust_chroot.display(),
            name()
        );
        msg!(
            "Executing '{}'",
            adjust_chroot
                .file_name()
                .unwrap_or_else(|| panic!(
                    "Cannot extract file name from path of adjust chroot script"
                ))
                .to_str()
                .unwrap_or_else(|| panic!(
                    "File name of adjust chroot script has some weird format"
                ))
        );
        let output = cmd!(
            adjust_chroot,
            name(),
            chroot_dir().join(CHROOT_ROOT_SUB_PATH)
        )
        .stderr_capture()
        .unchecked()
        .run()
        .with_context(|| err_msg.clone())?;

        if output.status.success() {
            Ok(())
        } else {
            Err(anyhow!("gpg: {}", from_utf8(&output.stderr).unwrap()).context(err_msg))
        }
    } else {
        Ok(())
    }
}

/// Retrieves the GPG key to be used to sign package files or the repository DB.
/// First, it is tried to get it from the environment variable GPG_KEY. If that
/// is not possible, it is tried to extract it from the relevant `makepkg.conf`
/// file. The retrievela is only done once. The result is stored in a static
/// variable
fn gpg_key() -> Option<&'static str> {
    static GPG_KEY: OnceCell<Option<String>> = OnceCell::new();
    match GPG_KEY.get_or_init(|| match env::var("GPGKEY") {
        Ok(value) => Some(value),
        _ => {
            lazy_static! {
                static ref RE_GPG_KEY: Regex = Regex::new(r#"GPGKEY=([^\n]+)\n.*"#).unwrap();
            }

            match fs::read_to_string(
                makepkg_conf().unwrap_or_else(|_| panic!("Cannot read from makepkg.conf")),
            ) {
                Err(_) => None,
                Ok(content) => {
                    let captures = RE_GPG_KEY.captures(content.as_str());

                    #[allow(clippy::unnecessary_unwrap)]
                    if captures.is_some() && captures.as_ref().unwrap().get(1).is_some() {
                        Some(
                            captures
                                .unwrap()
                                .get(1)
                                .unwrap_or_else(|| {
                                    panic!("Cannot extract GPG key from makepkg.conf")
                                })
                                .as_str()
                                .to_string(),
                        )
                    } else {
                        None
                    }
                }
            }
        }
    }) {
        Some(key) => Some(key.as_str()),
        None => None,
    }
}

/// Returns true if the repository DB is signed, false otherwise. The
/// determination whether the DB is signed or not is only done once. The result
/// is stored in a static variable
fn is_db_signed() -> bool {
    static IS_DB_SIGNED: OnceCell<bool> = OnceCell::new();
    *IS_DB_SIGNED.get_or_init(|| {
        let sig_file_name = local_dir().join(db_name().to_string() + DB_SUFFIX + SIG_SUFFIX);
        Path::new(&sig_file_name).exists()
    })
}

///  Prints a list of the packages of a repository incl. some of their meta data
pub fn list() -> anyhow::Result<()> {
    exec_on_repo!({
        if db_exists() {
            // Retrieve dependencies and packages
            let deps = deps()?;
            let db_pkgs = db_pkgs()
                .with_context(|| format!("Cannot list packages of repository {}", name()))?;

            // Determine max length of all package name and all architecture
            // strings
            let (max_name_len, max_arch_len) = db_pkgs
                .values()
                .map(|db_pkg| (db_pkg.name.len(), db_pkg.arch.len()))
                .fold((0, 0), |(x, y), (max_x, max_y)| {
                    (usize::max(x, max_x), usize::max(y, max_y))
                });

            println!("{}  [{}]", if is_db_signed() { "s" } else { "-" }, name());

            for db_pkg in db_pkgs.values() {
                println!(
                    "{0}{1} {2: <3$} {4: <5$} {6}",
                    if pkg(&db_pkg.name)?.is_signed() {
                        "s"
                    } else {
                        "-"
                    },
                    if deps.contains_key(&db_pkg.name) {
                        "d"
                    } else {
                        "-"
                    },
                    db_pkg.arch,
                    max_arch_len,
                    db_pkg.name,
                    max_name_len,
                    db_pkg.version
                );
            }
        }
    });

    Ok(())
}

/// Creates a lock (i.e., a file with the current process ID)
fn lock() -> anyhow::Result<()> {
    let err_msg = format!("Cannot create lock for repository {}", name());
    let lock_file = lock_file()?;

    if lock_file.exists() {
        let pid = pid_from_file(&lock_file).with_context(|| err_msg.clone())?;
        return if pid != process::id() {
            Err(anyhow!(
                "Lock file '{}' exists: repository {} is locked by process {}",
                lock_file.display(),
                name(),
                pid
            ))
        } else {
            Ok(())
        };
    }

    let mut f = fs::File::create(lock_file).with_context(|| err_msg.clone())?;
    write!(f, "{}", process::id()).with_context(|| err_msg)?;

    Ok(())
}

/// Returns the path to lock file of the repository
fn lock_file() -> anyhow::Result<PathBuf> {
    let err_msg = format!("Cannot determine lock file for repository {}", name());
    Ok(ensure_dir(&locks_dir().with_context(|| err_msg.clone())?)
        .with_context(|| err_msg)?
        .join(name()))
}

/// Creates a chroot container. First, a lock is created for the current
/// repository
pub fn make_chroot() -> anyhow::Result<()> {
    let err_msg = format!("Cannot make chroot for repository {}", name());

    // Since the repository will be changed it must be locked
    lock!();

    exec_with_tmp_data!({
        create_chroot().with_context(|| err_msg)?;
    });

    Ok(())
}

/// Determines the path of the relevant makepkg.conf file. This is done in the
/// following sequence (assuming that `~/.config/repman` is the path to the
/// repman config directory):
/// 1) ~/.config/repman/makepkg-<REPOSITORY-NAME>.conf
/// 2) ~/.config/repman/makepkg.conf
/// 3) /etc/makepkg.conf
/// The determination is only donw once. The result is stored in a static variable
fn makepkg_conf() -> anyhow::Result<&'static Path> {
    static MAKEPKG_CONF: OnceCell<PathBuf> = OnceCell::new();
    Ok(MAKEPKG_CONF
        .get_or_try_init(|| {
            // Assemble path of makepkg.conf file to be used for building
            // packages
            let err_msg = format!(
                "Cannot determine path to makepkg.conf for repository {}",
                name()
            );
            let config_dir = config_dir().with_context(|| err_msg.clone())?;
            let paths: [PathBuf; 3] = [
                config_dir.join("makepkg-".to_string() + name() + ".conf"),
                config_dir.join("makepkg.conf"),
                PathBuf::from("/etc/makepkg.conf"),
            ];
            for path in paths {
                if path.exists() {
                    return Ok(path);
                }
            }
            Err(anyhow!(
                "None of the possible makepkg.conf files exists for repository {}",
                name()
            ))
        })?
        .as_path())
}

/// Determines the path of the relevant pacman.conf file. This is done in the
/// following sequence (assuming that `~/.config/repman` is the path to the
/// repman config directory):
/// 1) ~/.config/repman/pacman-<REPOSITORY-NAME>.conf
/// 2) ~/.config/repman/pacman.conf
/// 3) /etc/pacman.conf
/// The determination is only donw once. The result is stored in a static variable
fn pacman_conf() -> anyhow::Result<&'static Path> {
    static PACMAN_CONF: OnceCell<PathBuf> = OnceCell::new();
    Ok(PACMAN_CONF
        .get_or_try_init(|| {
            // Assemble path of pacman.conf file to be used for building
            // packages
            let config_dir = config_dir().with_context(|| {
                format!(
                    "Cannot determine path to pacman.conf for repository {}",
                    name()
                )
            })?;
            let paths: [PathBuf; 3] = [
                config_dir.join("pacman-".to_string() + name() + ".conf"),
                config_dir.join("pacman.conf"),
                PathBuf::from("/etc/pacman.conf"),
            ];
            for path in paths {
                if path.exists() {
                    return Ok(path);
                }
            }
            Err(anyhow!(
                "None of the possible pacman.conf files exists for repository {}",
                name()
            ))
        })?
        .as_path())
}

/// Takes the pacman.conf file returned by pacman_conf() as template and creates
/// a temporary pacman.conf at .../tmp/<PID>/pacman.conf from it. The temporary
/// pacman.conf contains an entry for the current repository where the local
/// repository directory is configured as server. This is important for
/// dependencies from AUR. If such dependencies have been added to the current
/// repository before, the build process can "see" them. But therefore, the
/// current repository must be configured in the pacman.conf that is used format
/// the build process. The server entry for the current repository points to the
/// local (and not the remote) directory since dependencies of a packages are
/// added to the repository in the same repman call.
/// Note: The tempory directory for the current process must have been created
/// before calling this function
fn pacman_conf_for_chroot() -> anyhow::Result<PathBuf> {
    let err_msg = "Cannot prepare pacman.conf file for chroot";

    // pacman.conf which is used as template
    let pacman_conf_reader = BufReader::new(
        File::open(pacman_conf().with_context(|| err_msg)?).with_context(|| err_msg)?,
    );
    // pacman.conf for the new chroot
    let mut pacman_conf_new = PathBuf::new();
    pacman_conf_new.push(tmp_dir().with_context(|| err_msg)?.join("pacman.conf"));
    let mut pacman_conf_writer =
        BufWriter::new(File::create(&pacman_conf_new).with_context(|| err_msg)?);

    // Copy all lines of pacman_conf_reader to pacman_conf_writer, except those
    // that (potentially) configure the current repository in pacman_conf_reader.
    // Such a configuration could be there, but does not have to
    let mut it_is_me = false;
    for line in pacman_conf_reader.lines() {
        let line = line.with_context(|| err_msg)?;

        if line.starts_with(&format!("[{}]", db_name())) {
            it_is_me = true;
            continue;
        }

        if it_is_me {
            if !line.starts_with('[') {
                continue;
            }
            it_is_me = false;
        }

        pacman_conf_writer
            .write((line + "\n").as_bytes())
            .with_context(|| err_msg)?;
    }

    // Add section for current repository with local repository directory as
    // server/source to pacman_conf_new
    pacman_conf_writer
        .write(
            format!(
                "\n[{}]\nSigLevel = Optional TrustAll\nServer = file://{}\n",
                db_name(),
                local_dir().display()
            )
            .as_bytes(),
        )
        .with_context(|| err_msg)?;

    // Write buffer content to file
    pacman_conf_writer.flush().with_context(|| err_msg)?;

    Ok(pacman_conf_new)
}

/// Creates a package instance for the package name `pkg_name`. The package meta
/// data is retrieved from the repository DB. Thus, the repository must contain
/// the package
fn pkg<S>(pkg_name: S) -> anyhow::Result<Pkg>
where
    S: AsRef<str> + Display,
{
    let db_path = local_dir().join(db_name().to_string() + DB_SUFFIX);
    let db_pkg = db_pkgs()
        .with_context(|| {
            format!(
                "Cannot retrieve package {} from repository {}",
                pkg_name,
                name()
            )
        })?
        .get(pkg_name.as_ref())
        .ok_or_else(|| {
            anyhow!(
                "Package {} is not contained in repository {}",
                pkg_name,
                name()
            )
        })?;

    Pkg::from_meta_data(
        &db_pkg.name,
        &db_pkg.version,
        &db_pkg.arch,
        db_path.parent().unwrap(),
        pkg_ext()?,
    )
}

/// Determines the extension of package files from the relevant makepkg.conf
/// file. The determination is only donw once. The result is stored in a static
/// variable
fn pkg_ext() -> anyhow::Result<&'static str> {
    static PKG_EXT: OnceCell<String> = OnceCell::new();
    Ok(PKG_EXT
        .get_or_try_init(|| {
            let err_msg = format!(
                "Cannot determine package extension (PKG_EXT) for repository {}",
                name()
            );

            lazy_static! {
                static ref RE_PKG_EXT: Regex =
                    Regex::new(r#"PKGEXT= *['|"]{1}(.+)['|"]{1}.*"#).unwrap();
            }

            let content = fs::read_to_string(makepkg_conf()?).with_context(|| err_msg.clone())?;

            let captures = RE_PKG_EXT.captures(content.as_str());

            #[allow(clippy::unnecessary_unwrap)]
            if captures.is_some() && captures.as_ref().unwrap().get(1).is_some() {
                Ok(captures.unwrap().get(1).unwrap().as_str().to_string())
            } else {
                Err(anyhow!(err_msg))
            }
        })?
        .as_str())
}

/// Prepares the chroot container for usage. I.e., if the container exists, it is
/// updated. If it does not exist, it is being created
fn prepare_chroot() -> anyhow::Result<()> {
    let err_msg = format!("Cannot prepare chroot for repository {}", name());

    if chroot_exists() {
        msg!("Updating chroot for repository {} ...", name());

        // Update chroot
        let reader = cmd!(
            "arch-nspawn",
            chroot_dir().join(CHROOT_ROOT_SUB_PATH),
            format!("--bind-ro={}", local_dir().display()),
            "pacman",
            "-Syu",
            "--noconfirm",
        )
        .stderr_to_stdout()
        .stderr_capture()
        .reader()
        .with_context(|| err_msg.clone())?;
        for line in BufReader::new(reader).lines() {
            match line {
                Ok(text) => println!("{}", text),
                Err(err) => return Err(anyhow!(err).context(err_msg)),
            }
        }
    } else {
        create_chroot().with_context(|| err_msg.clone())?;
    }
    Ok(())
}

/// Removes packages with names contained in `pkg_names` from the repository DB
/// and removes the corresponding package files from the local repository
/// (cache) directory.
pub fn remove<S>(pkg_names: &[S], no_confirm: bool) -> anyhow::Result<()>
where
    S: AsRef<str> + Display,
{
    lock!();
    exec_on_repo!({
        if db_exists() {
            // Determine the names of the to-be-removed packages
            let deps = deps()?;
            let valid_pkg_names = valid_pkg_names(Some(pkg_names))
                .with_context(|| format!("Cannot remove packages from repository {}", name()))?;
            let to_be_removed_pkg_names: Vec<&str> = valid_pkg_names
                .into_iter()
                .filter(|pkg_name| {
                    no_confirm
                        || !deps.contains_key(pkg_name)
                        || Confirm::new()
                            .with_prompt(format!(
                "The following package(s) depend on {1}: {0}. Do you really want to remove {1}?",
                            deps.get(pkg_name).unwrap(),
                            pkg_name
                        ))
                            .default(false)
                            .show_default(true)
                            .interact()
                            .unwrap()
                })
                .collect();

            // Remove packages from repository DB and remove package files
            remove_pkgs::<&str>(&to_be_removed_pkg_names)
                .with_context(|| format!("Cannot remove packages from repository {}", name()))?;
        }
    });
    Ok(())
}

/// Removes the local cache directory of a remote repository (i.e., the directory
/// where repository data from the remote directory is copied for manipulation).
/// If the current repository is local, an error is returned
pub fn remove_cache_dir() -> anyhow::Result<()> {
    if !is_remote() {
        warning!(
            "Since '{}' is a local repository, there is no cache directory to be removed",
            name()
        );
        return Ok(());
    }

    let err_msg = format!("Cannot remove cache directory for repository {}", name());

    if !local_dir().exists() {
        msg!(
            "Cache directory for repository {} does not exist. Nothing to remove",
            name()
        );
        return Ok(());
    }

    lock!();
    fs::remove_dir_all(local_dir()).with_context(|| err_msg)?;

    Ok(())
}

/// Removes chroot directory of the current repository
pub fn remove_chroot_dir() -> anyhow::Result<()> {
    if !chroot_exists() {
        msg!(
            "Chroot directory for repository {} does not exist. Nothing to remove",
            name()
        );
        return Ok(());
    }

    let err_msg = format!("Cannot remove chroot directory for repository {}", name());

    lock!();

    // fs::remove_dir_all() can only be used if repman is running as root.
    // Otherwise "rm", run via sudo or su, is be used
    if sudo::check() == sudo::RunningAs::Root {
        fs::remove_dir_all(chroot_dir()).with_context(|| err_msg.clone())
    } else {
        let output = if is_pkg_installed("sudo").with_context(|| err_msg.clone())? {
            cmd!("sudo", "rm", "-rdf", chroot_dir(),)
                .stdout_null()
                .stderr_capture()
                .unchecked()
                .run()
                .with_context(|| err_msg.clone())?
        } else {
            cmd!("su", "root", "-c", "rm", "-rdf", chroot_dir(),)
                .stdout_null()
                .stderr_capture()
                .unchecked()
                .run()
                .with_context(|| err_msg.clone())?
        };
        if output.status.success() {
            Ok(())
        } else {
            Err(anyhow!("gpg: {}", from_utf8(&output.stderr).unwrap()).context(err_msg))
        }
    }
}

/// Removes signature files for the current repository
fn remove_db_sig_files() -> anyhow::Result<()> {
    let err_msg = format!("Cannot remove DB sig files of repository {}", name());
    let patterns: Vec<&str> = vec!["db", "files"];

    for pattern in patterns {
        for path in
            (glob(format!("{}/{}.{}*.sig", local_dir().display(), name(), pattern).as_str())
                .with_context(|| err_msg.clone())?)
            .flatten()
        {
            if path.is_file() {
                fs::remove_file(path).with_context(|| err_msg.clone())?;
            }
        }
    }

    Ok(())
}

/// Removes packages with names contained in `pkg_names` from the repository DB
/// and removes the corresponding package files from the local repository (cache)
/// directory. It is not checked if the to-be-removed packages are really
/// contained in the DB. Thus, this must be  checked before calling this function
fn remove_pkgs<S>(pkg_names: &[S]) -> anyhow::Result<()>
where
    S: AsRef<str> + Display,
{
    let to_be_removed_pkg_names: Vec<&str> = pkg_names
        .iter()
        .filter(|pkg_name| {
            match pkg(pkg_name)
                .unwrap_or_else(|_| {
                    // This code should never be reached since it was
                    // checked already that a package of name pkg_name is
                    // contained in the repository DB
                    panic!(
                        "Cannot retrieve package {} from repository {}",
                        pkg_name,
                        name()
                    )
                })
                .remove_from_dir(local_dir())
            {
                Ok(_) => true,
                Err(err) => {
                    error!(
                        "{:?}",
                        anyhow!(err.context(format!(
                            "Cannot remove package {} from repository {}",
                            pkg_name,
                            name()
                        )))
                    );
                    false
                }
            }
        })
        .map(AsRef::<str>::as_ref)
        .collect();

    remove_pkgs_from_db(&to_be_removed_pkg_names)?;

    Ok(())
}

/// Removes packages with names contained in `pkg_names` from the repository DB.
/// It is not checked if the to-be-removed packages are really contained in the
/// DB. Thus, this must be  checked before calling this function
fn remove_pkgs_from_db<S>(pkg_names: &[S]) -> anyhow::Result<()>
where
    S: AsRef<str>,
{
    if pkg_names.is_empty() {
        return Ok(());
    }

    let err_msg = format!("Cannot remove packages from DB of repository {}", name());

    // In case the repository is signed but will not be signed after removing
    // packages, the signature file are removed. This is required since
    // `repo-remove` does not remove such files
    if !sign_db() && is_db_signed() {
        remove_db_sig_files().with_context(|| err_msg.clone())?;
    }

    if sign_db() && gpg_key().is_none() {
        return Err(
            anyhow!("Repository DB shall be signed but GPG key is not set").context(err_msg),
        );
    }

    // Assemble args for repo-remove
    let repo_file = local_dir().join(db_name().to_string() + DB_ARCHIVE_SUFFIX);
    let mut args: Vec<&OsStr> = vec![OsStr::new("--verify")];
    if sign_db() {
        args.extend([
            OsStr::new("--sign"),
            OsStr::new("--key"),
            OsStr::new(gpg_key().unwrap()),
        ]);
    }
    args.push(repo_file.as_os_str());
    for pkg_name in pkg_names {
        args.push(OsStr::new(pkg_name.as_ref()))
    }

    // Execute repo-remove
    let output = cmd("repo-remove", &args)
        .stdout_null()
        .stderr_capture()
        .unchecked()
        .run()
        .with_context(|| err_msg.clone())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(anyhow!("repo-remove: {}", from_utf8(&output.stderr).unwrap()).context(err_msg))
    }
}

/// Signs packaage file for packages whose names are contained in `pkg_names`.
pub fn sign<S>(pkg_names: Option<&[S]>) -> anyhow::Result<()>
where
    S: AsRef<str> + Display,
{
    lock!();
    exec_on_repo!({
        let err_msg = format!("Cannot sign packages of repository {}", name());
        // Signing packages makes only sense if there is a repository DB
        if db_exists() {
            // Sign the relevant packages
            let gpg_key = gpg_key().with_context(|| err_msg.clone())?;
            for pkg_name in valid_pkg_names(pkg_names).with_context(|| err_msg.clone())? {
                if let Err(err) = pkg(pkg_name)
                    .with_context(|| err_msg.clone())?
                    .sign(gpg_key)
                {
                    error!(
                        "{:?}",
                        anyhow!(err.context(format!("Cannot sign package {}", pkg_name)))
                    );
                }
            }
        }
    });

    Ok(())
}

/// Converts an ULR into a path that can be used for ssh
fn ssh_path_from_url(url: &Url) -> String {
    format!(
        "{}{}:{}",
        url.username(),
        if let Some(host) = url.host_str() {
            format!(
                "{}{}",
                if url.username().is_empty() { "" } else { "@" },
                host
            )
        } else {
            " ".to_string()
        },
        url.path()
    )
}

/// Unlocks the current repository. I.e., removed the corresponding lock file
fn unlock() -> anyhow::Result<()> {
    let err_msg = format!("Cannot create lock for repository {}", name());
    let lock_file = lock_file()?;

    if lock_file.exists() {
        let pid = pid_from_file(&lock_file).with_context(|| err_msg.clone())?;
        if pid != process::id() {
            return Err(anyhow!(
                "Lock file '{}' exists: repository {} is locked by process {}",
                lock_file.display(),
                name(),
                pid
            )
            .context(err_msg));
        }
    }

    fs::remove_file(lock_file).with_context(|| err_msg)?;
    Ok(())
}

/// Updates all packages whose names are contained in `pkg_names`. If `no_chroot`
/// is true, building the new packages is not done via `makepkg`, otherwise via
/// `makechrootpkg`. If `clean_chroot` is true, the chroot will be removed after
/// all packages have been built. If `no_confirm` is true, the user will not be
/// asked for confirmations.
pub fn update<S>(
    pkg_names: Option<&[S]>,
    no_chroot: bool,
    clean_chroot: bool,
    no_confirm: bool,
) -> anyhow::Result<()>
where
    S: AsRef<str> + Display + Eq + Hash,
{
    let err_msg = format!("Cannot update packages of repository {}", name());

    lock!();
    exec_on_repo!({
        if db_exists() {
            // Extract names of packages that are contained in the current
            // repository
            let valid_pkg_names = valid_pkg_names(pkg_names).with_context(|| err_msg.clone())?;

            // Initialize AUR information from AUR web interface. If names of to
            // be updated packages were submitted (i.e., `pkg_names` is
            // `Some(...)`), error messages are printed if these package could
            // not be found in AUR. If no packages names were submitted, no
            // messages will be printed
            aur::try_init(&valid_pkg_names, pkg_names.is_some())
                .with_context(|| err_msg.clone())?;

            // Determine for which of these packages there are updates available
            // in AUR
            let pkg_upds = aur::pkg_updates::<S>(db_pkgs().with_context(|| err_msg.clone())?)
                .with_context(|| err_msg.clone())?;

            if pkg_upds.is_empty() {
                msg!("No updates available");
                return Ok(());
            }

            if !no_confirm {
                msg!("Updates available");
                for pkg_upd in &pkg_upds {
                    println!(
                        "    {} {} -> {}",
                        pkg_upd.name, pkg_upd.old_version, pkg_upd.new_version
                    );
                }
                if !Confirm::new()
                    .with_prompt("Continue?")
                    .default(true)
                    .show_default(true)
                    .interact()
                    .unwrap()
                {
                    return Ok(());
                }
                print!("\n");
            }

            // Execute package updates
            exec_with_tmp_data!({
                if !no_chroot {
                    // Create or update chroot container
                    prepare_chroot().with_context(|| err_msg.clone())?;
                }

                let (pkgbuild_dir, pkg_dir) =
                    ensure_pkg_tmp_dirs().with_context(|| err_msg.clone())?;
                let aur_pkg_names: Vec<&str> =
                    pkg_upds.iter().map(|pkg_upd| pkg_upd.pkg_base).collect();
                let mut built_pkgs: Vec<Pkg> = vec![];

                for pkgbuild in PkgBuild::from_aur(Some(&aur_pkg_names), pkgbuild_dir)? {
                    match Pkg::build(
                        &pkgbuild,
                        no_chroot,
                        None,
                        gpg_key(),
                        local_dir(),
                        chroot_dir(),
                        &pkg_dir,
                    ) {
                        Err(err) => {
                            error!("{:?}", err);
                            continue;
                        }
                        Ok(pkgs) => built_pkgs.extend(pkgs),
                    }
                }

                // Add the successfully built packages to respository DB
                add_pkgs_to_db(&built_pkgs).with_context(|| err_msg.clone())?;

                if clean_chroot {
                    remove_chroot_dir().with_context(|| err_msg.clone())?;
                }
            });
        }
    });

    Ok(())
}

/// Uploads the files of the current repository from a local directory, if the
/// repository is remote. If the function is called for a local repository, it
/// does not do anything
fn upload() -> anyhow::Result<()> {
    // Nothing to do if self is a local repository
    if !is_remote() {
        return Ok(());
    }

    let remote_dir: &str = remote_dir().unwrap();
    let mut local_dir = local_dir().as_os_str().to_os_string();
    local_dir.push("/");

    msg!(
        "Uploading repository {} from {} ... (this may take a while)",
        name(),
        remote_dir
    );

    let err_msg = format!("Cannot upload repository {}", name());

    // Sync changes from the local cache directory to the remote directory
    let output = cmd!("rsync", "-a", "-z", "--delete", local_dir, remote_dir,)
        .stdout_null()
        .stderr_capture()
        .unchecked()
        .run()
        .with_context(|| err_msg.clone())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(anyhow!(from_utf8(&output.stderr).unwrap().to_string()).context(err_msg))
    }
}

/// Determines package names that are relevant for a processing step (such as
/// removing, updating or signing these packages). If `pkg_names` is None, the
/// names of all packages contained in the current repository are returned.
/// Otherwise, only the names are returned that are contained in `pkg_names`
/// and where the corresponding package is contained in the current repository
fn valid_pkg_names<S>(pkg_names: Option<&[S]>) -> anyhow::Result<Vec<&str>>
where
    S: AsRef<str> + Display,
{
    let err_msg = "Cannot validate package names";
    let mut valid_pkg_names: Vec<&str> = vec![];
    match pkg_names {
        Some(pkg_names) => {
            for pkg_name in pkg_names {
                if contains_pkg(pkg_name).with_context(|| err_msg)? {
                    valid_pkg_names.push(pkg_name.as_ref());
                    continue;
                }
                error!(
                    "Package {} is not contained in repository {}",
                    pkg_name,
                    name()
                );
            }
        }
        None => {
            for pkg_name in db_pkgs().with_context(|| err_msg)?.keys() {
                valid_pkg_names.push(pkg_name);
            }
        }
    }

    Ok(valid_pkg_names)
}
