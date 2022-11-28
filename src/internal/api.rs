use crate::internal::{cfg, repo};
use anyhow::Context;
use arch_msgs::*;
use dialoguer::Confirm;
use std::{cmp::Eq, fmt::Display, hash::Hash, path::PathBuf};

pub fn add<S, T>(
    repo_name: S,
    aur_pkg_names: &[T],
    pkgbuild_dirs: &[PathBuf],
    no_chroot: bool,
    clean_chroot: bool,
    sign: bool,
) -> anyhow::Result<()>
where
    S: AsRef<str> + Display + Eq + Hash,
    T: AsRef<str> + Display + Eq + Hash,
{
    repo::try_init(repo_name)?;
    repo::add(aur_pkg_names, pkgbuild_dirs, no_chroot, clean_chroot, sign)
}

/// Clean up a repository
pub fn clean_up(repo_name: &str) -> anyhow::Result<()> {
    repo::try_init(repo_name)
        .with_context(|| format!("Cannot clear data of repository {}", repo_name))?;
    repo::clean_up()
}

/// Removes the chroot directory and/or (provided the repository is remote) the
/// cache directory of the repository represented by repo_name
pub fn clear<S>(repo_name: &S, clear_cache: bool, clear_chroot: bool) -> anyhow::Result<()>
where
    S: AsRef<str> + Display + Eq + Hash,
{
    repo::try_init(repo_name)
        .with_context(|| format!("Cannot clear data of repository {}", repo_name))?;

    if clear_cache {
        if !repo::is_remote() {
            warning!(
                "Repository {} is local. Thus, removing its cache directory does not makes sense",
                repo_name
            );
        } else {
            repo::remove_cache_dir().with_context(|| {
                format!("Cannot remove cache directory of repository {}", repo_name)
            })?;
            msg!("Cache directory of repository {} removed", repo_name);
        }
    }

    if clear_chroot {
        repo::remove_chroot_dir().with_context(|| {
            format!("Cannot remove chroot directory of repository {}", repo_name)
        })?;
        msg!("Chroot directory of repository {} removed", repo_name);
    }

    Ok(())
}

/// Lists the packages of a repository
pub fn ls<S>(repo_name: &S) -> anyhow::Result<()>
where
    S: AsRef<str> + Display + Eq + Hash,
{
    let err_msg = format!("Cannot list content of repository {}", repo_name);

    repo::try_init(repo_name).with_context(|| err_msg.clone())?;

    repo::list().with_context(|| err_msg)
}

/// Lists the names of all configured repositories
pub fn ls_repos() -> anyhow::Result<()> {
    for repo_name in cfg::repos()?.keys() {
        println!("{}", repo_name)
    }
    Ok(())
}

/// Creates a chroot container for a repository
pub fn mkchroot<S>(repo_name: &S) -> anyhow::Result<()>
where
    S: AsRef<str> + Display + Eq + Hash,
{
    let err_msg = format!("Cannot make chroot container for repository {}", repo_name);

    repo::try_init(repo_name).with_context(|| err_msg.clone())?;

    if repo::chroot_exists() {
        if Confirm::new()
            .with_prompt(format!(
                "A chroot for repository {} exists already. It is now being deleted. OK?",
                repo_name
            ))
            .default(true)
            .interact()
            .with_context(|| err_msg.clone())?
        {
            repo::remove_chroot_dir().with_context(|| err_msg.clone())?
        } else {
            return Ok(());
        }
    }

    repo::make_chroot().with_context(|| err_msg)?;

    Ok(())
}

/// Removes packages from a repository
pub fn rm<S, T>(repo_name: S, no_confirm: bool, pkg_names: &[T]) -> anyhow::Result<()>
where
    S: AsRef<str>,
    T: AsRef<str> + Display,
{
    if pkg_names.is_empty() {
        Ok(())
    } else {
        let err_msg = format!(
            "Cannot remove packages from repository {}",
            repo_name.as_ref()
        );
        repo::try_init(repo_name.as_ref()).with_context(|| err_msg.clone())?;
        repo::remove(pkg_names, no_confirm).with_context(|| err_msg)
    }
}

/// Signs packages of a repository
pub fn sign<S, T>(repo_name: S, pkg_names: Option<&[T]>) -> anyhow::Result<()>
where
    S: AsRef<str> + Display + Eq + Hash,
    T: AsRef<str> + Display,
{
    let err_msg = format!("Cannot sign packages of repository {}", repo_name);
    repo::try_init(repo_name).with_context(|| err_msg.clone())?;
    repo::sign(pkg_names).with_context(|| err_msg)
}

/// Update packages of a repository
pub fn update<S, T>(
    repo_name: S,
    no_chroot: bool,
    clean_chroot: bool,
    no_confirm: bool,
    pkg_names: Option<&[T]>,
) -> anyhow::Result<()>
where
    S: AsRef<str> + Display + Eq + Hash,
    T: AsRef<str> + Display + Eq + Hash,
{
    repo::try_init(repo_name)?;
    repo::update(pkg_names, no_chroot, clean_chroot, no_confirm)
}
