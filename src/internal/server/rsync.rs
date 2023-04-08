use crate::internal::server;
use anyhow::{anyhow, Context};
use arch_msgs::*;
use duct::cmd;
use std::{path::Path, str::from_utf8};
use url::Url;

pub struct Server {
    ssh_dir: String,
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

impl Server {
    pub fn new(url: Url) -> Self {
        Server {
            ssh_dir: ssh_path_from_url(&url),
        }
    }
}

impl server::Server for Server {
    fn is_remote(&self) -> bool {
        true
    }

    fn download_repo(&self, local_dir: &Path) -> anyhow::Result<()> {
        msg!(
            "Downloading repository from {} ... (this may take a while)",
            self.ssh_dir
        );

        let err_msg = "Cannot download repository";

        // Sync changes from remote directory to local cache directory
        let output = cmd!(
            "rsync",
            "-a",
            "-z",
            "--delete",
            format!("{}/", &self.ssh_dir),
            local_dir,
        )
        .stdout_null()
        .stderr_capture()
        .unchecked()
        .run()
        .with_context(|| err_msg)?;
        if output.status.success() {
            Ok(())
        } else {
            Err(anyhow!(from_utf8(&output.stderr).unwrap().to_string()).context(err_msg))
        }
    }

    fn upload_repo(&self, local_dir: &Path) -> anyhow::Result<()> {
        let mut local_dir = local_dir.as_os_str().to_os_string();
        local_dir.push("/");

        msg!(
            "Uploading repository from {} ... (this may take a while)",
            self.ssh_dir
        );

        let err_msg = "Cannot upload repository";

        // Sync changes from the local cache directory to the remote directory
        let output = cmd!("rsync", "-a", "-z", "--delete", local_dir, &self.ssh_dir,)
            .stdout_null()
            .stderr_capture()
            .unchecked()
            .run()
            .with_context(|| err_msg)?;
        if output.status.success() {
            Ok(())
        } else {
            Err(anyhow!(from_utf8(&output.stderr).unwrap().to_string()).context(err_msg))
        }
    }
}
