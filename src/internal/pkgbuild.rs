use crate::internal::aur::AurData;
use anyhow::{anyhow, Context};
use arch_msgs::*;
use duct::cmd;
use std::{
    cmp::Eq,
    ffi::OsStr,
    fmt::Display,
    hash::Hash,
    io::{prelude::*, BufReader},
    path::{Path, PathBuf},
};

const PKGBUILD_FILE_NAME: &str = "PKGBUILD";

/// PKGBUILD file
#[derive(Default)]
pub struct PkgBuild(PathBuf);

impl AsRef<Path> for PkgBuild {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

/// Tries to create PKGBUILD file instance from PathBuf
impl TryFrom<PathBuf> for PkgBuild {
    type Error = anyhow::Error;

    fn try_from(file: PathBuf) -> Result<Self, Self::Error> {
        // Check if file exists and if it is a PKGBUILD file
        if !file.exists() {
            return Err(anyhow!(
                "PKGBUILD file (candidate) '{}' does not exist",
                file.display()
            ));
        }
        if file
            .file_name()
            .unwrap_or_else(|| panic!("Cannot retrieve file name for PKGBUILD file (candidate)"))
            .to_str()
            .unwrap_or_else(|| panic!("PKGBUILD file name (candidate) is not a proper string"))
            != PKGBUILD_FILE_NAME
        {
            return Err(anyhow!("'{}' is not a PKGBUILD file", file.display()));
        }

        // Create new PkgBuild from the normalized version of file to avoid
        // something like "some-path/./PKGBUILD"
        Ok(PkgBuild(file.canonicalize()?))
    }
}

impl PkgBuild {
    /// Directory of PKGBUILD file
    fn dir(&self) -> &Path {
        self.as_ref()
            .parent()
            .unwrap_or_else(|| panic!("Cannot determine parent directory of PKGBUILD file"))
    }

    /// Creates PKGBUILD file instances from package repositories which are
    /// cloned from AUR. If `pkg_names` is Some(...) only packages are considered
    /// whose names are contained in `Some(pkg_names)`. Otherwise, all package
    /// repositories are considered where package information has been retrieved
    /// from AUR before
    pub fn from_aur<P, S>(
        aur_data: &AurData,
        pkg_names: Option<&[S]>,
        pkgbuild_dir: P,
    ) -> anyhow::Result<Vec<PkgBuild>>
    where
        P: AsRef<Path>,
        S: AsRef<str> + Display + Eq + Hash,
    {
        let mut pkgbuilds: Vec<PkgBuild> = vec![];
        for pkg_repo_dir in aur_data.clone_pkg_repos(pkg_names, pkgbuild_dir) {
            pkgbuilds.push(PkgBuild::try_from(pkg_repo_dir.join(PKGBUILD_FILE_NAME))?);
        }

        Ok(pkgbuilds)
    }

    /// Create PKGBUILD file instances from directory paths
    pub fn from_dirs<P>(dirs: &[P]) -> anyhow::Result<Vec<PkgBuild>>
    where
        P: AsRef<Path>,
    {
        let mut pkgbuilds: Vec<PkgBuild> = vec![];

        for dir in dirs {
            // dir must exist, be a directory and contain a PKGBUILD file
            if !dir.as_ref().exists() {
                error!("'{}' does not exist", dir.as_ref().display());
                continue;
            }
            if !dir.as_ref().is_dir() {
                error!("'{}' is not a directory", dir.as_ref().display());
                continue;
            }

            pkgbuilds.push(PkgBuild::try_from(dir.as_ref().join(PKGBUILD_FILE_NAME))?);
        }

        Ok(pkgbuilds)
    }

    /// Build packages from PKGBUILD file with makechrootpkg
    pub fn build_with_makechrootpkg<P>(
        &self,
        ignore_arch: bool,
        repo_dir: P,
        chroot_dir: P,
        pkg_dir: P,
    ) -> anyhow::Result<()>
    where
        P: AsRef<Path>,
    {
        let err_msg = format!(
            "Cannot build from '{}' with makechrootpkg",
            self.as_ref().display()
        );

        // Assemble arguments for makechrootpkg
        let mut args: Vec<&OsStr> = vec![
            OsStr::new("-r"),
            chroot_dir.as_ref().as_os_str(),
            OsStr::new("-D"),
            repo_dir.as_ref().as_os_str(),
            OsStr::new("-u"),
            OsStr::new("--"),
            OsStr::new("-c"),
            OsStr::new("--noconfirm"),
            OsStr::new("--needed"),
            OsStr::new("--syncdeps"),
        ];
        if ignore_arch {
            args.extend([OsStr::new("--ignorearch")]);
        }

        let reader = cmd("makechrootpkg", &args)
            .dir(self.dir())
            .env("PKGDEST", pkg_dir.as_ref())
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

        Ok(())
    }

    /// Build packages from PKGBUILD file with makepkg
    pub fn build_with_makepkg<P>(&self, ignore_arch: bool, pkg_dir: P) -> anyhow::Result<()>
    where
        P: AsRef<Path>,
    {
        let err_msg = format!(
            "Cannot build from '{}' with makepkg",
            self.as_ref().display()
        );

        // Assemble arguments for makepkg
        let mut args: Vec<&OsStr> = vec![
            OsStr::new("-u"),
            OsStr::new("SHELLOPTS"),
            OsStr::new("makepkg"),
            OsStr::new("-c"),
            OsStr::new("--noconfirm"),
            OsStr::new("--needed"),
            OsStr::new("--syncdeps"),
        ];
        if ignore_arch {
            args.extend([OsStr::new("--ignorearch")]);
        }

        let reader = cmd("env", &args)
            .dir(self.dir())
            .env("PKGDEST", pkg_dir.as_ref())
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

        Ok(())
    }

    /// Returnes list of package files that would be build with a PKGBUILD file
    pub fn pkg_files<P>(&self, pkg_dir: P) -> anyhow::Result<Vec<PathBuf>>
    where
        P: AsRef<Path>,
    {
        let err_msg = format!(
            "Cannot determine package list of PKGBUILD file '{}'",
            self.as_ref().display()
        );

        let output = cmd!("makepkg", "--packagelist",)
            .dir(self.dir())
            .env("PKGDEST", pkg_dir.as_ref().to_str().unwrap())
            .stderr_capture()
            .unchecked()
            .read()
            .with_context(|| err_msg.clone())?;
        let mut paths: Vec<PathBuf> = vec![];
        for line in output.lines() {
            let mut path = PathBuf::new();
            path.push(line);
            paths.push(path);
        }

        Ok(paths)
    }
}
