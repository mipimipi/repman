use anyhow::{anyhow, Context};
use duct::cmd;
use once_cell::sync::OnceCell;
use std::{
    env,
    fmt::{self, Display, Formatter},
    fs,
    path::{Path, PathBuf},
    process,
    str::from_utf8,
};

/// Supported architectures
#[allow(non_camel_case_types)]
#[derive(Debug)]
pub enum Arch {
    any,
    aarch64,
    armv7h,
    x86_64,
    Unknown,
}
impl Default for Arch {
    fn default() -> Self {
        arch().unwrap()
    }
}

impl Display for Arch {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Arch::any => write!(f, "any"),
            Arch::aarch64 => write!(f, "aarch64"),
            Arch::armv7h => write!(f, "armv7h"),
            Arch::x86_64 => write!(f, "x86_64"),
            Arch::Unknown => write!(f, "unknown"),
        }
    }
}

impl<S> From<S> for Arch
where
    S: AsRef<str>,
{
    fn from(arch: S) -> Self {
        match arch.as_ref() {
            "any" => Arch::any,
            "aarch64" => Arch::aarch64,
            "arm" => Arch::armv7h,
            "x86_64" => Arch::x86_64,
            &_ => Arch::Unknown,
        }
    }
}

/// Retrieves architecture of the system repman is running on
pub fn arch() -> anyhow::Result<Arch> {
    match Arch::from(env::consts::ARCH) {
        Arch::Unknown => Err(anyhow!(format!(
            "Architecture of this system ({}) is not supported",
            env::consts::ARCH
        ))),
        _ => Ok(Arch::from(env::consts::ARCH)),
    }
}

/// File suffixes
pub const SIG_SUFFIX: &str = ".sig";

/// File and directory names
const CACHE_SUB_PATH: &str = ".cache";
const CFG_DEFAULT_SUB_PATH: &str = ".config";
const LOCKS_SUB_PATH: &str = "locks";
const TMP_SUB_PATH: &str = "tmp";
pub const REPMAN_SUB_PATH: &str = "repman";

/// Path of cache directory. Often that's "~/.cache". The retrieval of the
/// cache directory is only done once. The result is buffered in a static
/// variable.
pub fn cache_dir() -> anyhow::Result<&'static Path> {
    static PATH: OnceCell<PathBuf> = OnceCell::new();
    Ok(PATH
        .get_or_try_init(|| {
            // Assemble path of cache directory. Sequence:
            //   (1) XDG cache dir (if that's available)
            //   (2) XDG home dir (if that's available) joined with default
            //       (relative) cache path
            // Both joined with the repman sub directory.
            Ok(if let Some(cache_dir) = dirs::cache_dir() {
                cache_dir.join(REPMAN_SUB_PATH)
            } else if let Some(home_dir) = dirs::home_dir() {
                home_dir.join(CACHE_SUB_PATH).join(REPMAN_SUB_PATH)
            } else {
                return Err(anyhow!("Cannot determine path of cache directory"));
            })
        })?
        .as_path())
}

/// Path of config directory. Often that's "~/.config". The retrieval of the
/// config directory is only done once. The result is buffered in a static
/// variable.
pub fn config_dir() -> anyhow::Result<&'static Path> {
    static PATH: OnceCell<PathBuf> = OnceCell::new();
    Ok(PATH
        .get_or_try_init(|| {
            // Assemble path of configuration directory. Sequence:
            //   (1) XDG config dir (if that's available)
            //   (2) XDG home dir (if that's available) joined with default
            //       (relative) configuration path
            let path = if let Some(cfg_dir) = dirs::config_dir() {
                cfg_dir.join(REPMAN_SUB_PATH)
            } else if let Some(home_dir) = dirs::home_dir() {
                home_dir.join(CFG_DEFAULT_SUB_PATH).join(REPMAN_SUB_PATH)
            } else {
                return Err(anyhow!("Cannot determine path of configuration file"));
            };
            Ok(path)
        })?
        .as_path())
}

/// Create directory `dir` if it does not exist
pub fn ensure_dir<P>(dir: P) -> anyhow::Result<PathBuf>
where
    P: AsRef<Path>,
{
    let err_msg = format!("Cannot create directory '{}' ", dir.as_ref().display());

    if dir.as_ref().exists() && !dir.as_ref().is_dir() {
        return Err(anyhow!(
            "'{}' exists already but is no directory",
            dir.as_ref().display()
        )
        .context(err_msg));
    }

    fs::create_dir_all(dir.as_ref()).with_context(|| err_msg)?;

    Ok(dir.as_ref().to_path_buf())
}

/// Creates a temporary directory for the current process
pub fn ensure_tmp_dir() -> anyhow::Result<PathBuf> {
    let err_msg = "Cannot ensure temporary directory";
    ensure_dir::<PathBuf>(tmp_dir().with_context(|| err_msg)?).with_context(|| err_msg)
}

/// Returns path of the directory where lock files are stored. Normally, thats:
/// `~/.cache/repman/locks`
pub fn locks_dir() -> anyhow::Result<PathBuf> {
    Ok(cache_dir()
        .with_context(|| "Cannot determine locks directory")?
        .join(LOCKS_SUB_PATH))
}

/// Checks is Arch Linux package of name `pkg_name` is installed
pub fn is_pkg_installed<S>(pkg_name: S) -> anyhow::Result<bool>
where
    S: AsRef<str> + Display,
{
    Ok(cmd!("pacman", "-Q", pkg_name.as_ref())
        .stdout_null()
        .stderr_capture()
        .unchecked()
        .run()
        .with_context(|| format!("Cannot check if package '{}' is installed", pkg_name))?
        .status
        .success())
}

/// Retrieve the process ID from the file `file`
pub fn pid_from_file<P>(file: P) -> anyhow::Result<u32>
where
    P: AsRef<Path>,
{
    let err_msg = format!(
        "Cannot retrieve PID from file '{}'",
        file.as_ref().display()
    );
    fs::read_to_string(file)
        .with_context(|| err_msg.clone())?
        .parse::<u32>()
        .with_context(|| err_msg)
}

/// Signs file `file` with `gpg` using key `gpg_key`
pub fn sign_file<P, S>(file: P, gpg_key: S) -> anyhow::Result<()>
where
    P: AsRef<Path>,
    S: AsRef<str>,
{
    let err_msg = format!("Cannot sign file '{}'", file.as_ref().to_str().unwrap());

    if gpg_key.as_ref().is_empty() {
        return Err(anyhow!("GPG key is not set").context(err_msg));
    }

    let output = cmd!(
        "gpg",
        "--yes",
        "-u",
        gpg_key.as_ref(),
        "--output",
        file.as_ref().to_str().unwrap().to_string() + SIG_SUFFIX,
        "--detach-sign",
        "--pinentry-mode=loopback",
        file.as_ref().to_str().unwrap(),
    )
    .stdout_null()
    .stderr_capture()
    .unchecked()
    .run()
    .with_context(|| err_msg.clone())?;

    if output.status.success() {
        Ok(())
    } else {
        Err(anyhow!(format!("gpg: {}", from_utf8(&output.stderr).unwrap())).context(err_msg))
    }
}

/// Assemble the path for the temporary directory for the current process.
/// Normally, that is `~/.cache/repman/tmp/<PID>`
pub fn tmp_dir() -> anyhow::Result<PathBuf> {
    Ok(cache_dir()
        .with_context(|| "Cannot assemble path of temporary directory")?
        .join(TMP_SUB_PATH)
        .join(format!("{}", process::id())))
}
