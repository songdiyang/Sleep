use std::sync::mpsc;
use std::path::Path;

use crate::remote_fs::{RemoteFs, RemoteDirEntry, FsEvent, Result};
use openssh::{Session, SessionBuilder, KnownHosts, Stdio};

/// SSH 认证方式
#[derive(Clone, Debug)]
pub enum SshAuth {
    Password(String),
    Key { path: String, passphrase: Option<String> },
    Agent,
}

/// SSH 连接配置
#[derive(Clone, Debug)]
pub struct SshConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: SshAuth,
}

impl Default for SshConfig {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: 22,
            username: String::new(),
            auth: SshAuth::Agent,
        }
    }
}

/// SSH 远程文件系统实现
pub struct SshRemoteFs {
    config: SshConfig,
    session: Option<Session>,
}

impl SshRemoteFs {
    /// 创建新的 SSH 远程文件系统
    pub fn new(config: SshConfig) -> Self {
        Self {
            config,
            session: None,
        }
    }

    /// 建立 SSH 连接
    pub fn connect(&mut self) -> Result<()> {
        let mut builder = SessionBuilder::default();
        
        // 配置主机和端口
        builder.known_hosts_check(KnownHosts::Add);
        
        // 根据认证方式配置
        match &self.config.auth {
            SshAuth::Password(password) => {
                builder.password_auth(password.clone());
            }
            SshAuth::Key { path, passphrase } => {
                let key_path = Path::new(path);
                if key_path.exists() {
                    match passphrase {
                        Some(pass) => builder.key_pair(key_path, pass),
                        None => builder.key_pair(key_path, ""),
                    }
                }
            }
            SshAuth::Agent => {
                // 使用 SSH agent
            }
        }

        let addr = format!("{}:{}", self.config.host, self.config.port);
        let session = builder.connect(&addr, &self.config.username)
            .map_err(|e| format!("SSH 连接失败: {}", e))?;
        
        self.session = Some(session);
        Ok(())
    }

    /// 检查连接是否活跃
    pub fn is_connected(&self) -> bool {
        self.session.is_some()
    }

    /// 断开 SSH 连接
    pub fn disconnect(&mut self) {
        self.session = None;
    }
}

impl RemoteFs for SshRemoteFs {
    /// 读取远程文件内容
    fn read_file(&self, path: &str) -> Result<Vec<u8>> {
        let session = self.session.as_ref()
            .ok_or("SSH 未连接，请先调用 connect()")?;
        
        let output = session.command(&format!("cat {}", path))
            .stdout(Stdio::capture())
            .stderr(Stdio::capture())
            .output()
            .map_err(|e| format!("读取文件失败: {}", e))?;

        Ok(output.stdout)
    }

    /// 写入文件到远程
    fn write_file(&self, path: &str, content: &[u8]) -> Result<()> {
        let session = self.session.as_ref()
            .ok_or("SSH 未连接，请先调用 connect()")?;

        // 使用 base64 编码文件内容来安全传输
        let encoded = base64::encode(content);
        let cmd = format!("echo {} | base64 -d > {}", encoded, path);
        
        session.command(&cmd)
            .output()
            .map_err(|e| format!("写入文件失败: {}", e))?;

        Ok(())
    }

    /// 列出远程目录内容
    fn list_dir(&self, path: &str) -> Result<Vec<RemoteDirEntry>> {
        let session = self.session.as_ref()
            .ok_or("SSH 未连接，请先调用 connect()")?;

        let output = session.command(&format!("ls -la {}", path))
            .stdout(Stdio::capture())
            .stderr(Stdio::capture())
            .output()
            .map_err(|e| format!("列出目录失败: {}", e))?;

        let entries: Vec<RemoteDirEntry> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .skip(1) // 跳过第一行 "total ..."
            .filter_map(|line| parse_ls_line(line))
            .collect();

        Ok(entries)
    }

    /// 监听文件变更（SSH 场景下可能不适用）
    fn watch(&self, _path: &str) -> Result<mpsc::Receiver<FsEvent>> {
        let (_tx, rx) = mpsc::channel();
        Ok(rx)
    }

    /// 在远程执行命令
    fn exec(&self, command: &str) -> Result<(String, String)> {
        let session = self.session.as_ref()
            .ok_or("SSH 未连接，请先调用 connect()")?;
        
        let output = session.command(command)
            .stdout(Stdio::capture())
            .stderr(Stdio::capture())
            .output()
            .map_err(|e| format!("SSH 命令执行失败: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok((stdout, stderr))
    }
}

/// 解析 ls -l 输出的一行
fn parse_ls_line(line: &str) -> Option<RemoteDirEntry> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 8 {
        return None;
    }

    let is_dir = parts[0].starts_with('d');
    let size = parts[4].parse::<u64>().unwrap_or(0);
    let name = parts[8..].join(" "); // 处理文件名包含空格的情况

    Some(RemoteDirEntry {
        name,
        is_dir,
        size,
        modified: None,
    })
}