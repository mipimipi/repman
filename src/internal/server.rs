use anyhow::{anyhow, Context};
use arch_msgs::*;
use duct::cmd;
use std::{
    borrow::Cow,
    ffi::{OsStr, OsString},
    os::unix::ffi::OsStrExt,
    path::Path,
    str::from_utf8,
};
use url::Url;

pub trait Server {
    fn is_remote(&self) -> bool {
        false
    }

    fn download_repo(&self, _local_dir: &Path) -> anyhow::Result<()> {
        Ok(())
    }
    fn upload_repo(&self, _local_dir: &Path) -> anyhow::Result<()> {
        Ok(())
    }
}

/// Constants for currently supported URL schemes
const SCHEME_FILE: &str = "file";
const SCHEME_RSYNC: &str = "rsync";
const SCHEME_S3: &str = "s3";

/// Takes an URL and creates - based on its scheme - an instance of a
/// corresponding type that implements the Server trait
pub fn new(url: &Url) -> anyhow::Result<Box<dyn Server>> {
    let server: Box<dyn Server> = match url.scheme() {
        SCHEME_FILE => Box::new(File::new()),
        SCHEME_RSYNC => Box::new(Rsync::new(url.clone())),
        SCHEME_S3 => Box::new(S3::new(url.clone())),
        _ => {
            return Err(anyhow!("Server URL '{}' has unsupported scheme", &url));
        }
    };

    Ok(server)
}

/// Generic code for downloading a repository from a remote location
macro_rules! download_repo {
    ($remote_dir:expr, $cmd:expr) => {
        msg!(
            "Downloading repository from {} ... (this may take a while)",
            $remote_dir
        );

        // Sync changes from remote directory to local cache directory
        let output = $cmd
            .stdout_null()
            .stderr_capture()
            .unchecked()
            .run()
            .with_context(|| "Cannot download repository")?;

        return if output.status.success() {
            Ok(())
        } else {
            Err(anyhow!(from_utf8(&output.stderr).unwrap().to_string())
                .context("Cannot download repository"))
        };
    };
}

/// Generic code for uploading a repository to a remote location
macro_rules! upload_repo {
    ($remote_dir:expr, $cmd:expr) => {
        msg!(
            "Uploading repository to {} ... (this may take a while)",
            $remote_dir
        );

        // Sync changes from the local cache directory to the remote directory
        let output = $cmd
            .stdout_null()
            .stderr_capture()
            .unchecked()
            .run()
            .with_context(|| "Cannot upload repository")?;

        return if output.status.success() {
            Ok(())
        } else {
            Err(anyhow!(from_utf8(&output.stderr).unwrap().to_string())
                .context("Cannot upload repository"))
        };
    };
}

/// Implementation for local file system
struct File {}
impl File {
    pub fn new() -> File {
        File {}
    }
}
impl Server for File {}

/// Implementation for rsync/SSH server
struct Rsync {
    ssh_dir: String,
}
impl Rsync {
    pub fn new(url: Url) -> Self {
        Rsync {
            ssh_dir: ssh_path_from_url(&url),
        }
    }
}
impl Server for Rsync {
    fn is_remote(&self) -> bool {
        true
    }

    fn download_repo(&self, local_dir: &Path) -> anyhow::Result<()> {
        download_repo!(
            self.ssh_dir,
            cmd!(
                "rsync",
                "-a",
                "-z",
                "--delete",
                format!("{}/", &self.ssh_dir),
                local_dir,
            )
        );
    }

    fn upload_repo(&self, local_dir: &Path) -> anyhow::Result<()> {
        upload_repo!(
            self.ssh_dir,
            cmd!(
                "rsync",
                "-a",
                "-z",
                "--delete",
                ensure_ends_with_slash(local_dir.as_os_str()),
                &self.ssh_dir,
            )
        );
    }
}

/// Implementation for AWS S3
struct S3 {
    url: Url,
}
impl S3 {
    pub fn new(url: Url) -> Self {
        S3 { url }
    }
}
impl Server for S3 {
    fn is_remote(&self) -> bool {
        true
    }

    fn download_repo(&self, local_dir: &Path) -> anyhow::Result<()> {
        download_repo!(
            self.url,
            cmd!(
                "s3cmd",
                "sync",
                "--delete-removed",
                ensure_ends_with_slash(OsStr::new(&self.url.as_str())),
                ensure_ends_with_slash(local_dir.as_os_str()),
            )
        );
    }

    fn upload_repo(&self, local_dir: &Path) -> anyhow::Result<()> {
        upload_repo!(
            self.url,
            cmd!(
                "s3cmd",
                "sync",
                "--follow-symlinks",
                "--delete-removed",
                "--acl-public",
                ensure_ends_with_slash(local_dir.as_os_str()),
                ensure_ends_with_slash(OsStr::new(&self.url.as_str())),
            )
        );
    }
}

/// Appends a slash at an OS string if it does not end already with one
fn ensure_ends_with_slash(s: &'_ OsStr) -> Cow<'_, OsStr> {
    if s.is_empty() {
        let mut t = OsString::new();
        t.push("/");
        Cow::Owned(t)
    } else if s.as_bytes().last() == Some(&b'/') {
        Cow::Borrowed(s)
    } else {
        let mut t = s.to_os_string();
        t.push("/");
        Cow::Owned(t)
    }
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
