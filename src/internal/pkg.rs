use crate::internal::{common::*, pkgbuild::PkgBuild};
use anyhow::{anyhow, Context};
use arch_msgs::*;
use glob::glob;
use lazy_static::lazy_static;
use regex::Regex;
use std::{
    fmt::Display,
    fs,
    path::{Path, PathBuf},
};

// Regular expression to check if a file could be a package file wrt. its path
// and to extract:
//   (1) Path of package directory
//   (2) Package name
//   (3) Package version with release number
//   (4) Release number
//   (5) Architecture
//   (6) Suffix of package file
// from package file path
lazy_static! {
    static ref RE_PKG_FILE: Regex =
        Regex::new(r#"^(.*/)(.+)-([^-]+)-([^-]+)-([^\.]+)(\.pkg\.tar\.[^\.]+)$"#).unwrap();
}

/// Package file
#[derive(Debug)]
pub struct Pkg(PathBuf);

impl AsRef<Path> for Pkg {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl TryFrom<PathBuf> for Pkg {
    type Error = anyhow::Error;

    /// Creates package file instance from a PathBuf
    fn try_from(file: PathBuf) -> Result<Self, Self::Error> {
        let err_msg = format!("Cannot create package from '{}'", file.display());

        if !file.exists() {
            return Err(anyhow!("Package file '{}' does not exist", file.display()));
        }

        if !RE_PKG_FILE.is_match(file.to_str().unwrap()) {
            return Err(
                anyhow!("'{}' is not a valid package file", file.display()).context(err_msg)
            );
        }

        Ok(Pkg(file))
    }
}

impl Pkg {
    /// Builds packages from a PKGBUILD file. From one PKGBUILD file, multiple
    /// packages can be built (in case of [split packages](https://man.archlinux.org/man/PKGBUILD.5#PACKAGE_SPLITTING))
    /// Packages are either built via makechrootpkg or makepkg, depending on
    /// `no_chroot`.
    /// After being built, package files are first stord in `pkg_dir`. Finally,
    /// they are moved to the repository directory `repo_dir`.
    /// If `sign` is `Some(...)`, package files are signed (`Some(true)`) or not
    /// (`Some(false)`). If `sign` is `None`, package files are only signed if
    /// there is a package file of an package version in `repo_dir` that is
    /// signed
    /// Function returns a vector of Pkg instance correswpo0nding to the packages
    /// that were built
    pub fn build<P, S>(
        pkgbuild: &PkgBuild,
        no_chroot: bool,
        sign: Option<bool>,
        gpg_key: Option<S>,
        repo_dir: P,
        chroot_dir: P,
        pkg_dir: P,
    ) -> anyhow::Result<Vec<Pkg>>
    where
        P: AsRef<Path> + Copy,
        S: AsRef<str>,
    {
        let err_msg = format!(
            "Cannot build packages from {}'",
            pkgbuild.as_ref().display()
        );

        if sign.is_some() && sign.unwrap() && gpg_key.is_none() {
            return Err(anyhow!(
                "Cannot built packages since they shall be signed but GPG_KEY is not set"
            ));
        }

        // Get list of package files that would be built from PKGBUILD file
        let pkg_files = pkgbuild.pkg_files(pkg_dir)?;
        if pkg_files.is_empty() {
            return Err(anyhow!("PKGBUILD does not define any package").context(err_msg));
        }

        msg!("Building package(s) from '{}'", pkgbuild.as_ref().display());

        // Build packages either with makepkg or makechrootpkg. Resulting package
        // files are stored in `pkg_dir`
        if no_chroot {
            pkgbuild
                .build_with_makepkg(pkg_dir)
                .with_context(|| err_msg.clone())?
        } else {
            pkgbuild
                .build_with_makechrootpkg(repo_dir, chroot_dir, pkg_dir)
                .with_context(|| err_msg.clone())?
        };

        // Process packages: Collect built packages, remove old package files,
        // copy new files to repository directory, and sign them
        let mut pkgs: Vec<Pkg> = vec![];
        for pkg_file in pkg_files {
            // Based on the package file determined before via
            // makepkg --packagelist, it is checked if all package files were
            // built
            // NOTE: Since the package version can be modified in PKGBUILD with
            // the pkgver() function, the version part of the built files might
            // be different from the file name as it was determined by makepkg
            // --packagelist. Thus, the new file name is retrieved in a rather
            // complex way via glob with a wildcard replacing the version:
            // .../NAME-*-PKGREL-ARCH.pkg.tar.zst
            let mut pkg =
                Pkg::from_file_ignore_version(&pkg_file).with_context(|| err_msg.clone())?;

            // Package file must either be signed if the sign parameter of this
            // function is Some(true), which might be the case if new packages
            // are added to the repository, or if there is an old package version
            // that was signed (which might be the case if packages are updated).
            // Since files of old package versions might have to be examined,
            // this block must be executed before old files are deleted
            let to_be_signed = match sign {
                Some(to_be_signed) => to_be_signed,
                None => {
                    // Check if a signed package file of an older package version
                    // exists in the repository directory
                    file_exists_for_pattern(
                        (pattern_ignore_version(
                            &pkg_file,
                            Some(&repo_dir.as_ref().to_path_buf()),
                        )?
                        .clone()
                            + SIG_SUFFIX)
                            .as_str(),
                    )
                }
            };

            // Remove old package files from repository directory
            // NOTE: This call must happen before the new package file is
            // moved to the repository directory, since otherwise the new
            // file would be removed as well
            pkg.remove_from_dir(repo_dir)
                .with_context(|| err_msg.clone())?;

            // Move new package file to repository directory
            pkg.move_to_dir(repo_dir).with_context(|| err_msg.clone())?;

            // Sign package file if required
            if to_be_signed {
                if gpg_key.as_ref().is_none() {
                    return Err(anyhow!("GPG_KEY is not set").context(err_msg));
                }
                pkg.sign(gpg_key.as_ref().unwrap())
                    .with_context(|| err_msg.clone())?;
            }

            pkgs.push(pkg);
        }

        Ok(pkgs)
    }

    /// Creates a Pkg instance from meta data such as package name and version
    /// The different genertic type `S` and `T` are used to supprot different
    /// string type in one call
    pub fn from_meta_data<P, S, T>(
        name: S,
        version: S,
        arch: S,
        local_dir: P,
        pkg_ext: T,
    ) -> anyhow::Result<Pkg>
    where
        P: AsRef<Path>,
        S: AsRef<str> + Display,
        T: AsRef<str> + Display,
    {
        Pkg::try_from(PathBuf::from(format!(
            "{}/{}-{}-{}{}",
            local_dir.as_ref().display(),
            name,
            version,
            arch,
            pkg_ext
        )))
    }

    /// Creates a package from a package file stored in the directiory
    /// `file.parent()` and having the same package name, architecture and file
    /// extension as `file`. `file` must be a package file.
    fn from_file_ignore_version<P>(file: P) -> anyhow::Result<Pkg>
    where
        P: AsRef<Path>,
    {
        let err_msg = format!(
            "Cannot create package from '{}' ignoring version",
            file.as_ref().display()
        );

        let pattern = pattern_ignore_version(file, None).with_context(|| err_msg.clone())?;

        Pkg::try_from(file_from_pattern(pattern.as_str()).with_context(|| err_msg.clone())?)
            .with_context(|| err_msg)
    }

    /// Returns `true` if package file is signed, `false` otherwise
    pub fn is_signed(&self) -> bool {
        let sig_file_name = self
            .as_ref()
            .to_str()
            .unwrap_or_else(|| {
                panic!("Path of package file cannot be converted to a proper string")
            })
            .to_string()
            + SIG_SUFFIX;
        Path::new(&sig_file_name).exists()
    }

    /// Moves package file to `dir`
    fn move_to_dir<P>(&mut self, dir: P) -> anyhow::Result<()>
    where
        P: AsRef<Path>,
    {
        let err_msg = format!(
            "Cannot move package file of '{}' to '{}'",
            self.name(),
            dir.as_ref().display()
        );

        // Make sure dir exists and is a directory
        if !dir.as_ref().exists() {
            return Err(
                anyhow!("Directory '{}' does not exist", dir.as_ref().display()).context(err_msg),
            );
        }
        if !dir.as_ref().is_dir() {
            return Err(anyhow!("'{}' is not a directory", dir.as_ref().display()))
                .context(err_msg);
        }

        let new_path = dir.as_ref().join(
            self.as_ref()
                .file_name()
                .unwrap_or_else(|| panic!("Cannot extract file name from path of package file")),
        );

        fs::rename(self.as_ref(), &new_path).with_context(|| err_msg)?;
        self.0 = new_path;

        Ok(())
    }

    /// Returns name of package that is stored in package file
    pub fn name(&self) -> String {
        let captures = RE_PKG_FILE
            .captures(self.as_ref().to_str()
		      .unwrap_or_else(|| panic!("Cannot extract package name from file since file path cannot be converted into a string")))
            .unwrap_or_else(|| panic!("Cannot extract package name from file since file is not a valid package file"));
        captures
            .get(2)
            .unwrap_or_else(|| panic!("Cannot extract package name from file"))
            .as_str()
            .to_string()
    }

    /// Removes all files belonging to package stored in package file from `dir`.
    /// This comprises the package file itself and a potentially exsiting
    /// signature file
    pub fn remove_from_dir<P>(&self, dir: P) -> anyhow::Result<()>
    where
        P: AsRef<Path>,
    {
        let err_msg = format!(
            "Cannot remove package files of {} from '{}'",
            self.name(),
            dir.as_ref().display()
        );

        // Make sure dir exists and is a directory
        if !dir.as_ref().exists() {
            return Err(
                anyhow!("Directory '{}' does not exist", dir.as_ref().display()).context(err_msg),
            );
        }
        if !dir.as_ref().is_dir() {
            return Err(anyhow!("'{}' is not a directory", dir.as_ref().display()))
                .context(err_msg);
        }

        for path in (glob(
            format!(
                "{}*",
                pattern_ignore_version(self.as_ref(), Some(dir.as_ref()))
                    .with_context(|| err_msg.clone())?
                    .as_str()
            )
            .as_str(),
        )
        .with_context(|| err_msg.clone())?)
        .flatten()
        {
            if path.is_file() {
                fs::remove_file(path).with_context(|| err_msg.clone())?;
            }
        }

        Ok(())
    }

    /// Signs package file
    pub fn sign<S>(&self, gpg_key: S) -> anyhow::Result<()>
    where
        S: AsRef<str>,
    {
        if self.is_signed() {
            return Ok(());
        }

        sign_file(self.as_ref(), gpg_key)
    }
}

/// Checks if a file exists that matches `pattern`
fn file_exists_for_pattern(pattern: &str) -> bool {
    glob(pattern)
        .unwrap_or_else(|_| panic!("Cannot check if file for pattern '{}' exists", pattern))
        .next()
        .is_some()
}

/// Returns the first file path as PathBuf that matches `pattern`
fn file_from_pattern(pattern: &str) -> anyhow::Result<PathBuf> {
    match glob(pattern)
        .unwrap_or_else(|_| panic!("Cannot retrieve file for pattern '{}'", pattern))
        .next()
    {
        Some(result) => {
            let path = result.unwrap_or_else(|_| {
                panic!(
                    "Some weird problem with path found for pattern '{}'",
                    pattern
                )
            });
            if !path.is_file() {
                Err(anyhow!(
                    "Found something matching pattern '{}' which is no file",
                    pattern
                ))
            } else {
                Ok(path)
            }
        }
        None => Err(anyhow!(
            "Could not find a anything matching pattern '{}'",
            pattern
        )),
    }
}

/// Creates a pattern from the file path of `file` where the version part is
/// replaced by the wildcard `*`. `file` must be a package file.
fn pattern_ignore_version<P>(file: P, dir: Option<P>) -> anyhow::Result<String>
where
    P: AsRef<Path>,
{
    if !RE_PKG_FILE.is_match(file.as_ref().to_str().unwrap()) {
        return Err(
            anyhow!("'{}' is not a valid package file", file.as_ref().display()).context(format!(
                "Cannot create package from '{}' ignoring version",
                file.as_ref().display()
            )),
        );
    }

    let captures = RE_PKG_FILE
        .captures(file.as_ref().to_str().unwrap())
        .unwrap();
    let dir_str: String = match dir {
        Some(dir) => dir.as_ref().to_str().unwrap().to_string() + "/",
        None => captures.get(1).unwrap().as_str().to_string(),
    };

    Ok(format!(
        "{}{}-*-{}-{}{}",
        dir_str,
        captures.get(2).unwrap().as_str(),
        captures.get(4).unwrap().as_str(),
        captures.get(5).unwrap().as_str(),
        captures.get(6).unwrap().as_str()
    ))
}
