use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

/// Git 文件状态
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitFileStatus {
    Unmodified,     // 未修改
    Modified,       // 已修改
    Added,          // 已暂存
    Deleted,        // 已删除
    Renamed,        // 重命名
    Copied,         // 已复制
    Untracked,      // 未跟踪
    Ignored,        // 已忽略
    Conflict,       // 冲突
}

/// Git 仓库状态
#[derive(Clone, Debug, Default)]
pub struct GitRepository {
    pub is_repo: bool,
    pub branch: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub file_status: HashMap<String, GitFileStatus>,
    pub has_changes: bool,
}

impl GitRepository {
    pub fn new() -> Self {
        Self::default()
    }

    /// 检测指定路径是否为 Git 仓库
    pub fn detect(path: &Path) -> Self {
        let mut repo = Self::new();
        
        // 检查 .git 目录是否存在
        let git_dir = path.join(".git");
        if git_dir.exists() {
            repo.is_repo = true;
            repo.branch = Self::get_branch(path);
            repo.file_status = Self::get_status(path);
            repo.has_changes = repo.file_status.values().any(|s| *s != GitFileStatus::Unmodified && *s != GitFileStatus::Ignored);
        }
        
        repo
    }

    /// 获取当前分支名
    fn get_branch(path: &Path) -> Option<String> {
        let head_path = path.join(".git").join("HEAD");
        if let Ok(content) = std::fs::read_to_string(&head_path) {
            let content = content.trim();
            if content.starts_with("ref: refs/heads/") {
                return Some(content[16..].to_string());
            }
            // 分离 HEAD（detached HEAD）
            return Some(content[..7].to_string());
        }
        None
    }

    /// 获取文件状态（通过 git status --porcelain 解析）
    fn get_status(path: &Path) -> HashMap<String, GitFileStatus> {
        let mut status_map = HashMap::new();
        
        if let Ok(output) = Command::new("git")
            .args(&["status", "--porcelain", "-u"])
            .current_dir(path)
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    if line.len() >= 3 {
                        let status_code = &line[..2];
                        let file_path = &line[3..];
                        
                        let status = match status_code {
                            " M" | "M " | "MM" => GitFileStatus::Modified,
                            "A " | "AM" | "AD" => GitFileStatus::Added,
                            "D " | " D" | "DD" => GitFileStatus::Deleted,
                            "R " | "RM" | "RD" => GitFileStatus::Renamed,
                            "C " | "CM" | "CD" => GitFileStatus::Copied,
                            "??" => GitFileStatus::Untracked,
                            "!!" => GitFileStatus::Ignored,
                            "UU" | "AA" | "AU" | "UA" | "DU" | "UD" => GitFileStatus::Conflict,
                            _ => GitFileStatus::Unmodified,
                        };
                        
                        status_map.insert(file_path.to_string(), status);
                    }
                }
            }
        }
        
        status_map
    }

    /// 刷新状态
    pub fn refresh(&mut self, path: &Path) {
        *self = Self::detect(path);
    }

    /// 获取文件状态
    pub fn file_status(&self, file: &str) -> GitFileStatus {
        self.file_status.get(file).copied().unwrap_or(GitFileStatus::Unmodified)
    }

    /// 获取状态图标
    pub fn status_icon(status: GitFileStatus) -> &'static str {
        match status {
            GitFileStatus::Modified => "M",
            GitFileStatus::Added => "A",
            GitFileStatus::Deleted => "D",
            GitFileStatus::Untracked => "U",
            GitFileStatus::Conflict => "C",
            GitFileStatus::Renamed => "R",
            GitFileStatus::Copied => "C",
            GitFileStatus::Ignored => "I",
            GitFileStatus::Unmodified => "",
        }
    }

    /// 获取状态颜色（用于UI渲染）
    pub fn status_color(status: GitFileStatus) -> (f32, f32, f32) {
        match status {
            GitFileStatus::Modified => (0.9, 0.7, 0.2),  // 黄色
            GitFileStatus::Added => (0.2, 0.8, 0.3),     // 绿色
            GitFileStatus::Deleted => (0.9, 0.2, 0.2),   // 红色
            GitFileStatus::Untracked => (0.5, 0.5, 0.5), // 灰色
            GitFileStatus::Conflict => (0.9, 0.2, 0.9),  // 紫色
            GitFileStatus::Renamed => (0.2, 0.6, 0.9),   // 蓝色
            GitFileStatus::Copied => (0.2, 0.9, 0.9),    // 青色
            GitFileStatus::Ignored => (0.4, 0.4, 0.4),    // 深灰
            GitFileStatus::Unmodified => (0.0, 0.0, 0.0),  // 黑色（不显示）
        }
    }
}

/// Git 命令执行器
pub struct GitCommand;

impl GitCommand {
    /// 执行 git 命令，返回 (stdout, stderr, success)
    fn exec(path: &Path, args: &[&str]) -> (String, String, bool) {
        let output = match Command::new("git")
            .args(args)
            .current_dir(path)
            .output()
        {
            Ok(o) => o,
            Err(e) => return (String::new(), format!("执行 git 命令失败: {}", e), false),
        };

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let success = output.status.success();
        (stdout, stderr, success)
    }

    /// git add <file>
    pub fn add(path: &Path, file: &str) -> Result<String, String> {
        let (stdout, stderr, success) = Self::exec(path, &["add", file]);
        if success {
            Ok(stdout)
        } else {
            Err(stderr)
        }
    }

    /// git add .
    pub fn add_all(path: &Path) -> Result<String, String> {
        let (stdout, stderr, success) = Self::exec(path, &["add", "."]);
        if success {
            Ok(stdout)
        } else {
            Err(stderr)
        }
    }

    /// git reset HEAD <file>
    pub fn unstage(path: &Path, file: &str) -> Result<String, String> {
        let (stdout, stderr, success) = Self::exec(path, &["reset", "HEAD", file]);
        if success {
            Ok(stdout)
        } else {
            Err(stderr)
        }
    }

    /// git commit -m <message>
    pub fn commit(path: &Path, message: &str) -> Result<String, String> {
        let (stdout, stderr, success) = Self::exec(path, &["commit", "-m", message]);
        if success {
            Ok(stdout)
        } else {
            Err(stderr)
        }
    }

    /// git push
    pub fn push(path: &Path) -> Result<String, String> {
        let (stdout, stderr, success) = Self::exec(path, &["push"]);
        if success {
            Ok(stdout)
        } else {
            Err(stderr)
        }
    }

    /// git pull
    pub fn pull(path: &Path) -> Result<String, String> {
        let (stdout, stderr, success) = Self::exec(path, &["pull"]);
        if success {
            Ok(stdout)
        } else {
            Err(stderr)
        }
    }

    /// git fetch
    pub fn fetch(path: &Path) -> Result<String, String> {
        let (stdout, stderr, success) = Self::exec(path, &["fetch"]);
        if success {
            Ok(stdout)
        } else {
            Err(stderr)
        }
    }

    /// git checkout -b <branch>
    pub fn create_branch(path: &Path, branch: &str) -> Result<String, String> {
        let (stdout, stderr, success) = Self::exec(path, &["checkout", "-b", branch]);
        if success {
            Ok(stdout)
        } else {
            Err(stderr)
        }
    }

    /// git checkout <branch>
    pub fn switch_branch(path: &Path, branch: &str) -> Result<String, String> {
        let (stdout, stderr, success) = Self::exec(path, &["checkout", branch]);
        if success {
            Ok(stdout)
        } else {
            Err(stderr)
        }
    }

    /// git branch
    pub fn list_branches(path: &Path) -> Vec<String> {
        let (stdout, _, success) = Self::exec(path, &["branch", "--format=%(refname:short)"]);
        if !success {
            return Vec::new();
        }
        stdout.lines().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
    }

    /// git log --oneline -n <count>
    pub fn log(path: &Path, count: usize) -> Vec<String> {
        let count_str = count.to_string();
        let (stdout, _, success) = Self::exec(path, &["log", "--oneline", "-n", &count_str]);
        if !success {
            return Vec::new();
        }
        stdout.lines().map(|s| s.to_string()).collect()
    }

    /// git clone <url> <path>
    pub fn clone(url: &str, path: &Path) -> Result<String, String> {
        let (stdout, stderr, success) = Self::exec(Path::new("."), &["clone", url, path.to_str().unwrap_or(".")]);
        if success {
            Ok(stdout)
        } else {
            Err(stderr)
        }
    }
}

/// Git 集成管理器
pub struct GitIntegration {
    pub repo: GitRepository,
    pub enabled: bool,
    pub current_folder: Option<std::path::PathBuf>,
    /// 上次操作结果
    pub last_result: Option<Result<String, String>>,
}

impl GitIntegration {
    pub fn new() -> Self {
        Self {
            repo: GitRepository::new(),
            enabled: true,
            current_folder: None,
            last_result: None,
        }
    }

    /// 检测并初始化 Git 仓库
    pub fn detect(&mut self, path: &Path) {
        self.current_folder = Some(path.to_path_buf());
        self.repo = GitRepository::detect(path);
    }

    /// 刷新状态
    pub fn refresh(&mut self) {
        if let Some(path) = &self.current_folder {
            self.repo = GitRepository::detect(path);
        }
    }

    /// 是否启用了 Git
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// 是否在 Git 仓库中
    pub fn is_repo(&self) -> bool {
        self.repo.is_repo
    }

    /// 暂存文件
    pub fn stage_file(&mut self, file: &str) -> Result<String, String> {
        if let Some(path) = &self.current_folder {
            let result = GitCommand::add(path, file);
            self.refresh();
            self.last_result = Some(result.clone());
            result
        } else {
            Err("未打开文件夹".to_string())
        }
    }

    /// 暂存所有文件
    pub fn stage_all(&mut self) -> Result<String, String> {
        if let Some(path) = &self.current_folder {
            let result = GitCommand::add_all(path);
            self.refresh();
            self.last_result = Some(result.clone());
            result
        } else {
            Err("未打开文件夹".to_string())
        }
    }

    /// 取消暂存
    pub fn unstage_file(&mut self, file: &str) -> Result<String, String> {
        if let Some(path) = &self.current_folder {
            let result = GitCommand::unstage(path, file);
            self.refresh();
            self.last_result = Some(result.clone());
            result
        } else {
            Err("未打开文件夹".to_string())
        }
    }

    /// 提交更改
    pub fn commit(&mut self, message: &str) -> Result<String, String> {
        if let Some(path) = &self.current_folder {
            let result = GitCommand::commit(path, message);
            self.refresh();
            self.last_result = Some(result.clone());
            result
        } else {
            Err("未打开文件夹".to_string())
        }
    }

    /// 推送
    pub fn push(&mut self) -> Result<String, String> {
        if let Some(path) = &self.current_folder {
            let result = GitCommand::push(path);
            self.refresh();
            self.last_result = Some(result.clone());
            result
        } else {
            Err("未打开文件夹".to_string())
        }
    }

    /// 拉取
    pub fn pull(&mut self) -> Result<String, String> {
        if let Some(path) = &self.current_folder {
            let result = GitCommand::pull(path);
            self.refresh();
            self.last_result = Some(result.clone());
            result
        } else {
            Err("未打开文件夹".to_string())
        }
    }

    /// 获取分支列表
    pub fn branches(&self) -> Vec<String> {
        if let Some(path) = &self.current_folder {
            GitCommand::list_branches(path)
        } else {
            Vec::new()
        }
    }

    /// 切换分支
    pub fn switch_branch(&mut self, branch: &str) -> Result<String, String> {
        if let Some(path) = &self.current_folder {
            let result = GitCommand::switch_branch(path, branch);
            self.refresh();
            self.last_result = Some(result.clone());
            result
        } else {
            Err("未打开文件夹".to_string())
        }
    }

    /// 创建分支
    pub fn create_branch(&mut self, branch: &str) -> Result<String, String> {
        if let Some(path) = &self.current_folder {
            let result = GitCommand::create_branch(path, branch);
            self.refresh();
            self.last_result = Some(result.clone());
            result
        } else {
            Err("未打开文件夹".to_string())
        }
    }

    /// 克隆仓库
    pub fn clone_repo(url: &str, path: &Path) -> Result<String, String> {
        GitCommand::clone(url, path)
    }
}

impl Default for GitIntegration {
    fn default() -> Self {
        Self::new()
    }
}
