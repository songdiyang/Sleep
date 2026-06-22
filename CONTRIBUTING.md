# 贡献指南

## Git 工作流

本项目采用 **临时分支 -> dev -> main** 的三级工作流，禁止直接提交到 `dev` 和 `main` 分支。

```
临时分支 (feature/fix) -> dev -> main
```

### 分支说明

| 分支 | 用途 | 保护级别 |
|------|------|----------|
| `main` | 生产环境，只接受 dev 分支的合并 | 🔒 严格保护 |
| `dev` | 开发集成，只接受临时分支的合并 | 🔒 保护 |
| `用户名/功能描述` | 临时功能分支，从 dev 切出 | 无保护 |

### 工作流步骤

#### 1. 创建临时分支

```bash
# 从 dev 最新代码创建
git checkout dev
git pull origin dev
git checkout -b songdiyang/add-new-feature
```

**分支命名规范**：`<用户名>/<功能描述>`
- 示例：`songdiyang/fix-login-error`、`songdiyang/add-user-profile`

#### 2. 开发和提交

```bash
git add .
git commit -m "feat(scope): 描述"
git push -u origin songdiyang/add-new-feature
```

提交信息格式：
- `feat(scope): 新增功能`
- `fix(scope): 修复问题`
- `perf(scope): 性能优化`
- `refactor(scope): 代码重构`
- `docs(scope): 文档更新`

#### 3. 发起 PR 到 dev

- **目标分支**：`dev`
- **源分支**：你的临时分支
- **必须填写 PR 模板**：
  - 更新描述（必填）
  - 更新类型（勾选）
  - **更新了哪些特性**（必填，列出所有特性）
  - 测试验证（勾选）

#### 4. PR 合并到 dev

- 合并后，GitHub Actions 会自动记录特性到 `.github/features/`
- 无需手动操作

#### 5. 从 dev 发起 PR 到 main

- **目标分支**：`main`
- **源分支**：`dev`
- GitHub Actions 会自动汇总 dev 分支的所有特性记录到 PR 描述

### 禁止行为

❌ **以下行为会被 GitHub Actions 阻止：**

1. 临时分支直接合并到 `main`
2. 从 `main` 直接合并到 `dev`
3. PR 描述未填写"更新了哪些特性"
4. 直接 push 到 `dev` 或 `main` 分支

### 特性记录

当 PR 被合并到 `dev` 分支时，系统会自动：

1. 从 PR 描述中提取"更新了哪些特性"
2. 创建记录文件到 `.github/features/YYYYMMDD_PR{编号}.md`
3. 在 PR 下评论确认已记录

当 `dev` 合并到 `main` 时，系统会自动：

1. 收集 `.github/features/` 下的所有记录
2. 汇总到 PR 描述中
3. 合并后清空特性记录（可选）

## 本地验证

提交 PR 前，请确保：

```bash
# 编译检查
cargo check --workspace

# 运行测试
cargo test -p aether-core

# Release 构建验证
cargo build --release --bin aether-app
```
