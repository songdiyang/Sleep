pub mod remote_fs;
pub mod ssh;
pub mod git;
pub mod container;
pub mod workspace;

#[cfg(test)]
mod tests;

pub use remote_fs::{RemoteFs, RemoteDirEntry, FsEvent, Result, GitRemoteInfo, GitSshRepo};
pub use ssh::{SshRemoteFs, SshConfig, SshAuth};
pub use git::{
    GitRepository, GitRepoConfig, GitRepoType,
    GitStatus, GitCommit, GitError,
    setup_ssh_credentials
};
pub use container::{ContainerBackend, ContainerConfig, ContainerRemoteFs};
pub use workspace::RemoteWorkspace;
