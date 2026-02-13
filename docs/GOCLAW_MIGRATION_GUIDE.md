# goclaw → nanors 功能移植指南

> 本文档详细说明如何将 goclaw 的核心功能移植到 nanors (Rust)

---

## 迁移状态

| 功能 | 状态 | 备注 |
|------|------|------|
| Skills 系统 | 未操作 | 动态技能插件架构 |
| web_fetch 工具 | 未操作 | HTTP 网页抓取工具 |
| web_search 工具 | 未操作 | 网络搜索工具 |
| Cron 调度器 | 未操作 | 定时任务调度器 |

**状态说明**：
- `未操作` - 尚未开始移植
- `操作中` - 正在进行代码移植
- `待测试` - 代码已完成，等待测试验证
- `测试完成` - 功能已验证通过

---

## 目录

1. [Skills 系统](#1-skills-系统)
2. [web_fetch 工具](#2-web_fetch-工具)
3. [web_search 工具](#3-web_search-工具)
4. [Cron 调度器](#4-cron-调度器)
5. [配置集成](#5-配置集成)

---

## 1. Skills 系统

### 1.1 goclaw 实现概述

Skills 系统是 goclaw 的核心插件架构，允许动态注入技能到 AI Agent 的系统提示词中。

**核心特性：**
- Markdown 文件格式 (`SKILL.md`)
- YAML frontmatter 元数据
- 环境检测（二进制、环境变量、配置文件）
- 优先级系统（workspace > managed > bundled）
- 热重载支持（文件系统监听）
- OpenClaw/AgentSkills 兼容

**目录结构：**
```
skills/
├── api.go              # API 网关
├── discovery.go        # 技能发现和加载
├── eligibility.go       # 环境检测和过滤
├── frontmatter.go      # YAML 解析
└── snapshot.go         # 提示词注入
```

### 1.2 nanors 架构设计

#### 新增 Crate: `nanors_skills`

```
nanors_skills/
├── Cargo.toml
├── src/
│   ├── lib.rs              # 公共接口
│   ├── skill.rs            # Skill 数据结构
│   ├── discovery.rs        # 技能发现
│   ├── frontmatter.rs      # YAML 解析
│   ├── eligibility.rs      # 环境检测
│   ├── snapshot.rs         # 提示词生成
│   └── config.rs           # 配置结构
└── tests/
```

#### 依赖项 (Cargo.toml)

```toml
[package]
name = "nanors_skills"
version = "0.1.0"
edition = "2024"

[dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
serde_yaml = "0.9"
toml = "0.8"
tokio = { version = "1.0", features = ["fs", "time"] }
tracing = "0.1"
anyhow = "1.0"
notify = "7.0"           # 文件系统监听
glob = "0.3"             # 文件匹配
regex = "1.0"            # 模式过滤
once_cell = "1.0"         # 全局状态

[dev-dependencies]
tempfile = "3.0"
```

### 1.3 核心数据结构

#### `src/skill.rs`

```rust
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::collections::HashMap;

/// Skill 技能条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    /// 技能名称
    pub name: String,
    /// 技能描述
    pub description: String,
    /// 技能文件路径
    pub file_path: PathBuf,
    /// 技能基础目录
    pub base_dir: PathBuf,
    /// 技能来源
    pub source: SkillSource,
    /// Frontmatter 元数据
    pub frontmatter: ParsedFrontmatter,
    /// OpenClaw 元数据
    pub metadata: Option<OpenClawMetadata>,
    /// 技能内容（不含 frontmatter）
    pub content: String,
    /// 缺失依赖
    pub missing_deps: Option<MissingDependencies>,
}

/// 技能来源（优先级从高到低）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SkillSource {
    /// 工作区技能（优先级 300）
    Workspace,
    /// 托管技能（优先级 200）
    Managed,
    /// 内置技能（优先级 100）
    Bundled,
    /// 自定义路径（优先级 50）
    Custom,
}

impl SkillSource {
    pub fn priority(&self) -> u32 {
        match self {
            Self::Workspace => 300,
            Self::Managed => 200,
            Self::Bundled => 100,
            Self::Custom => 50,
        }
    }
}

/// 解析后的 Frontmatter
pub type ParsedFrontmatter = HashMap<String, serde_json::Value>;

/// OpenClaw 技能元数据
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpenClawMetadata {
    /// 总是启用此技能
    #[serde(default)]
    pub always: bool,

    /// 技能唯一标识
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_key: Option<String>,

    /// 主要环境变量
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_env: Option<String>,

    /// 表情符号
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji: Option<String>,

    /// 主页 URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,

    /// 支持的操作系统
    #[serde(default)]
    pub os: Vec<String>,

    /// 依赖要求
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires: Option<SkillRequires>,

    /// 安装规格
    #[serde(default)]
    pub install: Vec<SkillInstallSpec>,
}

/// 依赖要求
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillRequires {
    /// 必需的二进制文件
    #[serde(default)]
    pub bins: Vec<String>,

    /// 任一即可（anyBins 中至少一个可用）
    #[serde(default)]
    pub any_bins: Vec<String>,

    /// 必需的环境变量
    #[serde(default)]
    pub env: Vec<String>,

    /// 必需的配置文件
    #[serde(default)]
    pub config: Vec<String>,
}

/// 安装规格
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInstallSpec {
    /// 安装 ID
    pub id: String,

    /// 安装类型
    pub kind: InstallKind,

    /// 安装标签
    pub label: String,

    /// 安装后的二进制文件
    #[serde(default)]
    pub bins: Vec<String>,

    /// 支持的操作系统
    #[serde(default)]
    pub os: Vec<String>,

    /// 包管理器相关
    #[serde(skip_serializing_if = "Option::is_none")]
    pub formula: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// 安装类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstallKind {
    Brew,
    Node,
    Go,
    Uv,
    Download,
}

/// 缺失依赖
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MissingDependencies {
    #[serde(default)]
    pub bins: Vec<String>,
    #[serde(default)]
    pub any_bins: Vec<String>,
    #[serde(default)]
    pub env: Vec<String>,
    #[serde(default)]
    pub config: Vec<String>,
    #[serde(default)]
    pub os: Vec<String>,
}
```

### 1.4 Frontmatter 解析

#### `src/frontmatter.rs`

```rust
use anyhow::{Context, Result};
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;

lazy_static::lazy_static! {
    static ref FRONTMATTER_RE: Regex =
        Regex::new(r"^---\n(.*?)\n---").unwrap();
}

/// 解析 Frontmatter
pub fn parse_frontmatter(content: &str) -> Result<(ParsedFrontmatter, String)> {
    let normalized = normalize_newlines(content);

    if !normalized.starts_with("---") {
        return Ok((HashMap::new(), normalized));
    }

    let captures = FRONTMATTER_RE.captures(&normalized)
        .context("Failed to find frontmatter delimiters")?;

    let block = &captures[1];
    let body = normalized[captures[0].len()..].to_string();

    // 尝试 YAML 解析
    let yaml_result = parse_yaml_frontmatter(block);
    let line_result = parse_line_frontmatter(block);

    let mut merged = yaml_result.unwrap_or_default();
    // 行解析优先于 JSON 值
    for (k, v) in line_result {
        if is_json_value(&v) {
            merged.insert(k, v);
        }
    }

    Ok((merged, body))
}

fn parse_yaml_frontmatter(block: &str) -> Option<ParsedFrontmatter> {
    serde_yaml::from_str(block).ok()
}

fn parse_line_frontmatter(block: &str) -> ParsedFrontmatter {
    let mut result = HashMap::new();
    for line in block.lines() {
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_string();
            let value = value.trim().to_string();
            result.insert(key, Value::String(value));
        }
    }
    result
}

fn is_json_value(value: &Value) -> bool {
    match value {
        Value::String(s) => {
            s.starts_with('{') || s.starts_with('[')
        }
        _ => false,
    }
}

fn normalize_newlines(s: &str) -> String {
    s.replace("\r\n", "\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_frontmatter_yaml() {
        let content = r#"---
name: test-skill
description: A test skill
version: 1.0.0
---
Content here"#;
        let (fm, body) = parse_frontmatter(content).unwrap();
        assert_eq!(fm.get("name"), Some(&Value::String("test-skill".into())));
        assert_eq!(body, "Content here");
    }
}
```

### 1.5 技能发现和加载

#### `src/discovery.rs`

```rust
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use tokio::fs;
use futures::stream::{self, StreamExt};

/// 技能加载配置
#[derive(Debug, Clone)]
pub struct SkillLoadConfig {
    /// 工作区目录
    pub workspace_dir: PathBuf,
    /// 托管技能目录 (~/.nanors/skills)
    pub managed_dir: PathBuf,
    /// 内置技能目录
    pub bundled_dir: PathBuf,
    /// 额外的技能目录
    pub extra_dirs: Vec<PathBuf>,
}

impl Default for SkillLoadConfig {
    fn default() -> Self {
        Self {
            workspace_dir: PathBuf::from("./skills"),
            managed_dir: dirs::home_dir()
                .unwrap()
                .join(".nanors/skills"),
            bundled_dir: PathBuf::from("/usr/lib/nanors/skills"),
            extra_dirs: Vec::new(),
        }
    }
}

/// 技能加载结果
#[derive(Debug)]
pub struct SkillLoadResult {
    /// 所有加载的技能（按名称去重）
    pub skills: HashMap<String, Skill>,
    /// 加载统计
    pub stats: LoadStats,
}

#[derive(Debug, Default)]
pub struct LoadStats {
    pub total_found: usize,
    pub loaded: usize,
    pub skipped: usize,
    pub errors: usize,
}

/// 加载所有技能
pub async fn load_skills(config: &SkillLoadConfig) -> Result<SkillLoadResult> {
    let mut all_skills: Vec<(String, Skill, u32)> = Vec::new();

    // 按优先级顺序加载（低优先级先加载）
    let sources = vec![
        (SkillSource::Bundled, &config.bundled_dir),
        (SkillSource::Managed, &config.managed_dir),
        (SkillSource::Workspace, &config.workspace_dir),
    ];

    // 额外目录优先级最低
    for (idx, dir) in config.extra_dirs.iter().enumerate() {
        sources.push((SkillSource::Custom, dir));
    }

    for (source, dir) in sources {
        match load_skills_from_dir(dir, source).await {
            Ok(mut skills) => {
                for skill in skills.drain(..) {
                    let priority = source.priority();
                    all_skills.push((skill.name.clone(), skill, priority));
                }
            }
            Err(e) => {
                tracing::warn!("Failed to load skills from {:?}: {}", dir, e);
            }
        }
    }

    // 按优先级合并（高优先级覆盖低优先级）
    all_skills.sort_by_key(|(_, _, p)| *p);

    let mut merged: HashMap<String, Skill> = HashMap::new();
    for (name, skill, _) in all_skills {
        merged.insert(name, skill);
    }

    let stats = LoadStats {
        total_found: all_skills.len(),
        loaded: merged.len(),
        ..Default::default()
    };

    Ok(SkillLoadResult { skills: merged, stats })
}

/// 从目录加载技能
async fn load_skills_from_dir(
    dir: &Path,
    source: SkillSource,
) -> Result<Vec<Skill>> {
    let mut skills = Vec::new();

    if !dir.exists() {
        return Ok(skills);
    }

    let mut entries = fs::read_dir(dir).await?;
    while let Some(entry) = entries.next().await {
        let entry = entry?;
        let path = entry.path();

        // 跳过隐藏目录
        if path.file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.starts_with('.'))
            .unwrap_or(false)
        {
            continue;
        }

        // 递归处理子目录
        if path.is_dir() {
            let sub_skills = load_skills_from_dir(&path, source).await?;
            skills.extend(sub_skills);
            continue;
        }

        // 只处理 .md 文件
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }

        // 加载技能文件
        match load_skill_from_file(&path, source).await {
            Ok(skill) => skills.push(skill),
            Err(e) => {
                tracing::warn!("Failed to load skill {:?}: {}", path, e);
            }
        }
    }

    Ok(skills)
}

/// 从文件加载技能
async fn load_skill_from_file(
    path: &Path,
    source: SkillSource,
) -> Result<Skill> {
    let content = fs::read_to_string(path).await?;
    let (frontmatter, body) = parse_frontmatter(&content)
        .context("Failed to parse frontmatter")?;

    let name = frontmatter
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'name' in frontmatter"))?
        .to_string();

    let description = frontmatter
        .get("description")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'description' in frontmatter"))?
        .to_string();

    // 解析 OpenClaw 元数据
    let metadata = frontmatter
        .get("metadata")
        .and_then(|v| serde_json::from_value(v.clone()).ok());

    Ok(Skill {
        name,
        description,
        file_path: path.to_path_buf(),
        base_dir: path.parent()
            .unwrap_or_else(|| Path::new("/"))
            .to_path_buf(),
        source,
        frontmatter,
        metadata,
        content: body,
        missing_deps: None,
    })
}
```

### 1.6 环境检测

#### `src/eligibility.rs`

```rust
use anyhow::Result;
use std::env;
use std::path::Path;

/// 环境检测器
pub struct EligibilityChecker;

impl EligibilityChecker {
    /// 检查技能是否应该在当前环境加载
    pub fn should_include(skill: &Skill) -> bool {
        // 1. 检查 always 标志
        if skill.metadata
            .as_ref()
            .map(|m| m.always)
            .unwrap_or(false)
        {
            return true;
        }

        // 2. 检查操作系统兼容性
        if !Self::check_os_compatibility(skill) {
            return false;
        }

        // 3. 检查二进制文件
        if !Self::check_binaries(skill) {
            return false;
        }

        // 4. 检查环境变量
        if !Self::check_env_vars(skill) {
            return false;
        }

        true
    }

    fn check_os_compatibility(skill: &Skill) -> bool {
        let Some(metadata) = &skill.metadata else {
            return true; // 没有限制
        };

        if metadata.os.is_empty() {
            return true; // 没有限制
        }

        let current_os = env::consts::OS;
        metadata.os.iter().any(|os| {
            os.as_str() == current_os
                || (os == "darwin" && current_os == "macos")
                || (os == "macos" && current_os == "darwin")
        })
    }

    fn check_binaries(skill: &Skill) -> bool {
        let Some(metadata) = &skill.metadata else {
            return true;
        };
        let Some(requires) = &metadata.requires else {
            return true;
        };

        // 检查必需的二进制
        for bin in &requires.bins {
            if !Self::binary_exists(bin) {
                return false;
            }
        }

        // 检查 anyBins（至少一个）
        if !requires.any_bins.is_empty() {
            let has_any = requires.any_bins.iter()
                .any(|bin| Self::binary_exists(bin));
            if !has_any {
                return false;
            }
        }

        true
    }

    fn check_env_vars(skill: &Skill) -> bool {
        let Some(metadata) = &skill.metadata else {
            return true;
        };
        let Some(requires) = &metadata.requires else {
            return true;
        };

        requires.env.iter().all(|var| {
            env::var(var).is_ok()
        })
    }

    fn binary_exists(name: &str) -> bool {
        // 使用 which 命令检测
        #[cfg(unix)]
        {
            use std::process::Command;
            Command::new("which")
                .arg(name)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        }

        #[cfg(windows)]
        {
            use std::process::Command;
            Command::new("where")
                .arg(name)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        }
    }

    /// 获取缺失的依赖
    pub fn get_missing_deps(skill: &Skill) -> MissingDependencies {
        let mut missing = MissingDependencies::default();

        let Some(metadata) = &skill.metadata else {
            return missing;
        };
        let Some(requires) = &metadata.requires else {
            return missing;
        };

        // 检查缺失的二进制
        missing.bins = requires.bins.iter()
            .filter(|bin| !Self::binary_exists(bin))
            .cloned()
            .collect();

        // 检查 anyBins
        if !requires.any_bins.is_empty() {
            let has_any = requires.any_bins.iter()
                .any(|bin| Self::binary_exists(bin));
            if !has_any {
                missing.any_bins = requires.any_bins.clone();
            }
        }

        // 检查缺失的环境变量
        missing.env = requires.env.iter()
            .filter(|var| env::var(var).is_err())
            .cloned()
            .collect();

        // 检查缺失的配置文件
        missing.config = requires.config.iter()
            .filter(|path| !Path::new(path).exists())
            .cloned()
            .collect();

        missing
    }
}
```

### 1.7 提示词生成

#### `src/snapshot.rs`

```rust
use anyhow::Result;
use std::collections::HashMap;

/// 技能快照（用于注入系统提示词）
#[derive(Debug, Clone)]
pub struct SkillSnapshot {
    /// 生成的提示词
    pub prompt: String,
    /// 版本号（用于缓存失效）
    pub version: i64,
    /// 包含的技能数量
    pub skill_count: usize,
}

/// 构建技能快照
pub fn build_skill_snapshot(skills: &HashMap<String, Skill>) -> Result<SkillSnapshot> {
    if skills.is_empty() {
        return Ok(SkillSnapshot {
            prompt: String::new(),
            version: 0,
            skill_count: 0,
        });
    }

    let mut lines = Vec::new();
    lines.push(String::new());
    lines.push("The following skills provide specialized instructions for specific tasks.".to_string());
    lines.push("Use the read_file tool to load a skill's file when task matches its description.".to_string());
    lines.push(String::new());
    lines.push("<available_skills>".to_string());

    // 按名称排序
    let mut skill_names: Vec<&String> = skills.keys().collect();
    skill_names.sort();

    for name in skill_names {
        let skill = &skills[name];
        lines.push("  <skill>".to_string());
        lines.push(format!("    <name>{}</name>", escape_xml(&skill.name)));
        lines.push(format!("    <description>{}</description>",
            escape_xml(&skill.description)));
        lines.push(format!("    <location>{}</location>",
            escape_xml(&skill.file_path.to_string_lossy())));
        lines.push("  </skill>".to_string());
    }

    lines.push("</available_skills>".to_string());

    Ok(SkillSnapshot {
        prompt: lines.join("\n"),
        version: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis() as i64,
        skill_count: skills.len(),
    })
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// 版本号管理器
pub struct VersionManager {
    global_version: std::sync::atomic::AtomicI64,
    workspace_versions: parking_lot::Mutex<HashMap<String, i64>>,
}

impl VersionManager {
    pub fn new() -> Self {
        Self {
            global_version: std::sync::atomic::AtomicI64::new(0),
            workspace_versions: parking_lot::Mutex::new(HashMap::new()),
        }
    }

    pub fn bump_global(&self) -> i64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        self.global_version.fetch_max(now, std::sync::atomic::Ordering::SeqCst);
        self.global_version.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn bump_workspace(&self, workspace: &str) -> i64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let mut versions = self.workspace_versions.lock();
        let current = versions.get(workspace).copied().unwrap_or(0);
        let new = now.max(current + 1);
        versions.insert(workspace.to_string(), new);
        new
    }
}

impl Default for VersionManager {
    fn default() -> Self {
        Self::new()
    }
}
```

### 1.8 公共接口

#### `src/lib.rs`

```rust
mod config;
mod discovery;
mod eligibility;
mod frontmatter;
mod skill;
mod snapshot;

pub use config::{SkillLoadConfig, SkillsConfig};
pub use discovery::{load_skills, SkillLoadResult};
pub use eligibility::{EligibilityChecker, MissingDependencies};
pub use frontmatter::parse_frontmatter;
pub use skill::{
    InstallKind, OpenClawMetadata, ParsedFrontmatter, Skill, SkillInstallSpec,
    SkillRequires, SkillSource,
};
pub use snapshot::{build_skill_snapshot, SkillSnapshot, VersionManager};

/// nanors_skills 技能系统
///
/// # 示例
///
/// ```no_run
/// use nanors_skills::{load_skills, SkillLoadConfig, EligibilityChecker};
///
/// # #[tokio::main]
/// # async fn main() -> anyhow::Result<()> {
/// let config = SkillLoadConfig::default();
/// let result = load_skills(&config).await?;
///
/// // 过滤可用的技能
/// let available: Vec<_> = result.skills.into_iter()
///     .filter(|(_, skill)| EligibilityChecker::should_include(skill))
///     .collect();
///
/// println!("Loaded {} skills", available.len());
/// # Ok(())
/// # }
/// ```
```

### 1.9 集成到 nanors_core

#### 修改 `nanors_core/src/lib.rs`

```rust
// 添加依赖
pub use nanors_skills;

// 在 AgentLoop 中集成技能
use nanors_skills::{SkillSnapshot, SkillLoadConfig, load_skills};

pub struct AgentLoop {
    // ... 现有字段

    /// 技能快照
    skill_snapshot: Arc<RwLock<SkillSnapshot>>,
    /// 技能版本（用于检测更新）
    skill_version: Arc<AtomicI64>,
}

impl AgentLoop {
    pub async fn new(...) -> anyhow::Result<Self> {
        // ... 现有初始化

        // 加载技能
        let skill_config = SkillLoadConfig::default();
        let skill_result = load_skills(&skill_config).await?;
        let skills = EligibilityChecker::filter_eligible(skill_result.skills);
        let skill_snapshot = build_skill_snapshot(&skills)?;

        Ok(Self {
            // ...
            skill_snapshot: Arc::new(RwLock::new(skill_snapshot)),
            skill_version: Arc::new(AtomicI64::new(0)),
        })
    }

    /// 构建系统提示词（包含技能）
    async fn build_system_prompt(&self) -> String {
        let base_prompt = self.config.system_prompt.clone()
            .unwrap_or_else(|| DEFAULT_SYSTEM_PROMPT.to_string());

        let snapshot = self.skill_snapshot.read().await;
        if snapshot.skill_count == 0 {
            return base_prompt;
        }

        format!("{}\n\n{}", base_prompt, snapshot.prompt)
    }
}
```

---

## 2. web_fetch 工具

### 2.1 goclaw 实现概述

web_fetch 工具提供 HTTP 网页抓取和 HTML 到 Markdown 转换功能。

**核心功能：**
- HTTP GET 请求
- URL 验证
- 自定义 headers (User-Agent, Accept)
- 超时控制
- HTML 简化转 Markdown
- 错误处理

### 2.2 nanors 实现设计

#### 新增文件: `nanors_tools/src/web_fetch.rs`

```rust
use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use super::{schema_object, Tool, ToolDefinition, ToolResult};

/// Web fetch 工具配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebFetchConfig {
    /// 请求超时（秒）
    #[serde(default = "WebFetchConfig::default_timeout")]
    pub timeout: u64,

    /// User-Agent
    #[serde(default = "WebFetchConfig::default_user_agent")]
    pub user_agent: String,

    /// 最大响应大小（字节）
    #[serde(default = "WebFetchConfig::default_max_size")]
    pub max_size: usize,
}

impl WebFetchConfig {
    fn default_timeout() -> u64 { 10 }
    fn default_user_agent() -> String {
        "Mozilla/5.0 (compatible; nanors/1.0)".to_string()
    }
    fn default_max_size() -> usize { 1_000_000 } // 1MB
}

impl Default for WebFetchConfig {
    fn default() -> Self {
        Self {
            timeout: Self::default_timeout(),
            user_agent: Self::default_user_agent(),
            max_size: Self::default_max_size(),
        }
    }
}

/// Web fetch 工具
pub struct WebFetchTool {
    client: Client,
    config: WebFetchConfig,
}

impl WebFetchTool {
    pub fn new(config: WebFetchConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self { client, config })
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name().to_string(),
            description: "Fetch a web page and convert to simplified text. \
                Supports http and https URLs.".to_string(),
            input_schema: schema_object(
                json!({
                    "url": {
                        "type": "string",
                        "description": "The URL to fetch (http or https only)"
                    }
                }),
                &["url"],
            ),
        }
    }

    async fn execute(&self, input: serde_json::Value) -> ToolResult {
        // 提取参数
        let url = match input.get("url").and_then(|v| v.as_str()) {
            Some(u) => u,
            None => return ToolResult::error("Missing required parameter: url"),
        };

        // 验证 URL
        let parsed = match url::Url::parse(url) {
            Ok(u) => u,
            Err(e) => {
                return ToolResult::error(format!("Invalid URL: {}", e))
                    .with_error_type("invalid_url");
            }
        };

        // 只支持 HTTP/HTTPS
        if !matches!(parsed.scheme(), "http" | "https") {
            return ToolResult::error(
                "Only http and https URLs are supported"
            ).with_error_type("unsupported_scheme");
        }

        // 发送请求
        let response = match self.client
            .get(url)
            .header("User-Agent", &self.config.user_agent)
            .header("Accept", "text/html, text/markdown, text/plain")
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return ToolResult::error(format!("HTTP request failed: {}", e))
                    .with_error_type("http_error");
            }
        };

        let status = response.status();
        let headers = response.headers().clone();

        // 获取内容类型
        let content_type = headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream");

        // 限制读取大小
        let bytes = match response.bytes().await {
            Ok(b) => b,
            Err(e) => {
                return ToolResult::error(format!("Failed to read response: {}", e))
                    .with_error_type("read_error");
            }
        };

        if bytes.len() > self.config.max_size {
            return ToolResult::error(format!(
                "Response too large: {} bytes (max: {})",
                bytes.len(),
                self.config.max_size
            )).with_error_type("size_exceeded");
        }

        // 转换内容
        let content = if content_type.contains("html") {
            self.html_to_text(&bytes)
        } else {
            String::from_utf8_lossy(&bytes).to_string()
        };

        // 截断过长内容
        let content = if content.len() > 10_000 {
            format!("{}\n\n... (truncated at 10000 chars)", &content[..10_000])
        } else {
            content
        };

        ToolResult::success(content)
            .with_status_code(status.as_u16() as i32)
    }
}

impl WebFetchTool {
    /// HTML 转纯文本（简化版）
    fn html_to_text(&self, bytes: &[u8]) -> String {
        let html = String::from_utf8_lossy(bytes);

        // 移除 script 和 style 标签
        let html = self.remove_tag(&html, "script");
        let html = self.remove_tag(&html, "style");

        // 简单的文本提取
        let text = html
            .replace("<br>", "\n")
            .replace("<br/>", "\n")
            .replace("<br />", "\n")
            .replace("</p>", "\n\n")
            .replace("</div>", "\n")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");

        text.chars()
            .collect::<Vec<_>>()
            .chunks(100)
            .map(|chunk| chunk.iter().collect())
            .collect::<Vec<String>>()
            .join("\n")
    }

    fn remove_tag(&self, html: &str, tag: &str) -> String {
        let start = format!("<{}>", tag);
        let end_start = format!("</{}>", tag);
        let end_self = format!("<{} ", tag);

        let mut result = html.to_string();
        let mut pos = 0;

        while pos < result.len() {
            if let Some(idx) = result[pos..].find(&start) {
                result.replace_range(pos..pos + idx + start.len(), " ");
                pos += idx + start.len();

                // 找到闭合标签
                if let Some(end_idx) = result[pos..].find(&end_start) {
                    result.replace_range(pos..pos + end_idx + end_start.len(), " ");
                } else if let Some(self_idx) = result[pos..].find(&end_self) {
                    if let Some(closer) = result[pos + self_idx..].find('>') {
                        result.replace_range(
                            pos..pos + self_idx + closer + 1,
                            " "
                        );
                    }
                }
            } else {
                break;
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_web_fetch_success() {
        let tool = WebFetchTool::new(WebFetchConfig::default()).unwrap();
        let def = tool.definition();
        assert_eq!(def.name, "web_fetch");
    }

    #[test]
    fn test_html_to_text() {
        let tool = WebFetchTool::new(WebFetchConfig::default()).unwrap();
        let html = r#"<html><body><h1>Title</h1><p>Hello <script>var x = 1;</script>world</p></body></html>"#;
        let text = tool.html_to_text(html.as_bytes());
        assert!(!text.contains("script"));
        assert!(!text.contains("var x"));
        assert!(text.contains("Title") || text.contains("Hello") || text.contains("world"));
    }
}
```

### 2.3 修改 `nanors_tools/src/lib.rs`

```rust
pub mod apply_patch;
pub mod bash;
pub mod command_runner;
pub mod glob;
pub mod grep;
pub mod path_guard;
pub mod read_file;
pub mod web_fetch;   // 新增

pub use web_fetch::WebFetchTool;  // 新增

// 在 ToolRegistry::with_default_tools 中添加
pub fn with_default_tools(working_dir: &str) -> Self {
    let mut registry = Self::new();
    registry.add_tool(Box::new(BashTool::new(working_dir)));
    registry.add_tool(Box::new(ReadFileTool::new(working_dir)));
    registry.add_tool(Box::new(ApplyPatchTool::new(working_dir)));
    registry.add_tool(Box::new(GlobTool::new(working_dir)));
    registry.add_tool(Box::new(GrepTool::new(working_dir)));
    registry.add_tool(Box::new(WebFetchTool::new(WebFetchConfig::default()).unwrap()));  // 新增
    registry
}
```

### 2.4 配置集成

#### 修改 `nanors_config/src/schema.rs`

```rust
#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct Config {
    #[serde(default)]
    pub agents: AgentsConfig,
    #[serde(default)]
    pub providers: ProvidersConfig,
    #[serde(default)]
    pub database: DatabaseConfig,
    #[serde(default)]
    pub memory: MemoryConfig,
    #[serde(default)]
    pub telegram: TelegramConfig,
    #[serde(default)]
    pub tools: ToolsConfig,  // 新增
}

// 新增配置
#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct ToolsConfig {
    #[serde(default)]
    pub web_fetch: WebFetchConfig,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct WebFetchConfig {
    #[serde(default = "WebFetchConfig::default_timeout")]
    pub timeout: u64,
    #[serde(default = "WebFetchConfig::default_max_size")]
    pub max_size: usize,
}

impl WebFetchConfig {
    fn default_timeout() -> u64 { 10 }
    fn default_max_size() -> usize { 1_000_000 }
}
```

---

## 3. web_search 工具

### 3.1 goclaw 实现概述

web_search 工具提供网络搜索功能，支持多层回退策略。

**搜索策略（优先级从高到低）：**
1. Tavily API（专业搜索 API）
2. Serper API（Google 搜索 API）
3. Crawl4AI（Python 脚本回退）
4. Chrome CDP（浏览器自动化）

### 3.2 nanors 实现设计

#### 新增文件: `nanors_tools/src/web_search.rs`

```rust
use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use super::{schema_object, Tool, ToolDefinition, ToolResult};

/// Web search 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchConfig {
    /// 搜索 API (tavily, serper, mock)
    #[serde(default = "WebSearchConfig::default_api")]
    pub api: String,

    /// API Key
    #[serde(default)]
    pub api_key: String,

    /// 最大结果数
    #[serde(default = "WebSearchConfig::default_max_results")]
    pub max_results: usize,

    /// 超时（秒）
    #[serde(default = "WebSearchConfig::default_timeout")]
    pub timeout: u64,
}

impl WebSearchConfig {
    fn default_api() -> String { "tavily".to_string() }
    fn default_max_results() -> usize { 5 }
    fn default_timeout() -> u64 { 10 }
}

impl Default for WebSearchConfig {
    fn default() -> Self {
        Self {
            api: Self::default_api(),
            api_key: String::new(),
            max_results: Self::default_max_results(),
            timeout: Self::default_timeout(),
        }
    }
}

/// Web search 工具
pub struct WebSearchTool {
    client: Client,
    config: WebSearchConfig,
}

impl WebSearchTool {
    pub fn new(config: WebSearchConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self { client, config })
    }
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name().to_string(),
            description: "Search the web for information. \
                Returns relevant results with titles, URLs, and descriptions.".to_string(),
            input_schema: schema_object(
                json!({
                    "query": {
                        "type": "string",
                        "description": "The search query"
                    }
                }),
                &["query"],
            ),
        }
    }

    async fn execute(&self, input: serde_json::Value) -> ToolResult {
        let query = match input.get("query").and_then(|v| v.as_str()) {
            Some(q) => q,
            None => return ToolResult::error("Missing required parameter: query"),
        };

        // 无 API Key 时的友好提示
        if self.config.api_key.is_empty() {
            return ToolResult::success(format!(
                "Search results for: {}\n\n[Note: Search API key not configured. \
                Add api_key to tools.web_search.api_key in config to enable real search results.]",
                query
            ));
        }

        // 根据配置的 API 执行搜索
        let result = match self.config.api.as_str() {
            "tavily" => self.search_tavily(query).await,
            "serper" => self.search_serper(query).await,
            _ => self.search_mock(query).await,
        };

        match result {
            Ok(r) => ToolResult::success(r),
            Err(e) => ToolResult::error(format!("Search failed: {}", e))
                .with_error_type("search_error"),
        }
    }
}

impl WebSearchTool {
    /// Tavily API 搜索
    async fn search_tavily(&self, query: &str) -> Result<String> {
        let request_body = serde_json::json!({
            "query": query,
            "search_depth": "basic",
            "max_results": self.config.max_results,
            "include_images": false,
        });

        let response = self.client
            .post("https://api.tavily.com/search")
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .json(&request_body)
            .send()
            .await
            .context("Tavily API request failed")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Tavily API returned {}: {}", status, body);
        }

        #[derive(Deserialize)]
        struct TavilyResult {
            results: Vec<TavilyItem>,
        }

        #[derive(Deserialize)]
        struct TavilyItem {
            title: String,
            url: String,
            content: String,
        }

        let result: TavilyResult = response.json().await
            .context("Failed to parse Tavily response")?;

        if result.results.is_empty() {
            return Ok(format!("No results found for query: {}", query));
        }

        let mut output = Vec::new();
        output.push(format!("Search results for: {}", query));
        output.push(String::new());

        for item in result.results {
            output.push(format!("Title: {}", item.title));
            output.push(format!("URL: {}", item.url));
            output.push(format!("Content: {}", item.content));
            output.push(String::new());
        }

        Ok(output.join("\n"))
    }

    /// Serper API 搜索
    async fn search_serper(&self, query: &str) -> Result<String> {
        let request_body = serde_json::json!({
            "q": query,
        });

        let response = self.client
            .post("https://google.serper.dev/search")
            .header("X-API-KEY", &self.config.api_key)
            .json(&request_body)
            .send()
            .await
            .context("Serper API request failed")?;

        if !response.status().is_success() {
            let status = response.status();
            anyhow::bail!("Serper API returned {}", status);
        }

        #[derive(Deserialize)]
        struct SerperResult {
            #[serde(default)]
            organic: Vec<SerperItem>,
        }

        #[derive(Deserialize)]
        struct SerperItem {
            title: String,
            link: String,
            snippet: String,
        }

        let result: SerperResult = response.json().await
            .context("Failed to parse Serper response")?;

        if result.organic.is_empty() {
            return Ok(format!("No results found for query: {}", query));
        }

        let mut output = Vec::new();
        output.push(format!("Search results for: {}", query));
        output.push(String::new());

        for item in result.organic {
            output.push(format!("Title: {}", item.title));
            output.push(format!("URL: {}", item.link));
            output.push(format!("Description: {}", item.snippet));
            output.push(String::new());
        }

        Ok(output.join("\n"))
    }

    /// Mock 搜索（用于测试或无 API Key）
    async fn search_mock(&self, query: &str) -> Result<String> {
        Ok(format!(
            "Search results for: {}\n\n\
            [Mock Results - Configure API key for real results]\n\n\
            1. Example Result 1\n   https://example.com/1\n   Description for result 1\n\n\
            2. Example Result 2\n   https://example.com/2\n   Description for result 2",
            query
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_web_search_mock() {
        let config = WebSearchConfig {
            api: "mock".to_string(),
            ..Default::default()
        };
        let tool = WebSearchTool::new(config).unwrap();
        let result = tool.execute(json!({"query": "test rust"})).await;
        assert!(!result.is_error);
        assert!(result.content.contains("test rust"));
    }
}
```

### 3.3 修改 `nanors_tools/src/lib.rs`

```rust
pub mod web_search;  // 新增
pub use web_search::WebSearchTool;  // 新增

// 在 ToolRegistry::with_default_tools 中添加
pub fn with_default_tools(working_dir: &str) -> Self {
    let mut registry = Self::new();
    // ... 其他工具
    registry.add_tool(Box::new(WebSearchTool::new(WebSearchConfig::default()).unwrap()));  // 新增
    registry
}
```

### 3.4 配置集成

#### 修改 `nanors_config/src/schema.rs`

```rust
#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct ToolsConfig {
    #[serde(default)]
    pub web_fetch: WebFetchConfig,
    #[serde(default)]
    pub web_search: WebSearchConfig,  // 新增
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct WebSearchConfig {
    #[serde(default = "WebSearchConfig::default_api")]
    pub api: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "WebSearchConfig::default_max_results")]
    pub max_results: usize,
    #[serde(default = "WebSearchConfig::default_timeout")]
    pub timeout: u64,
}

impl WebSearchConfig {
    fn default_api() -> String { "tavily".to_string() }
    fn default_max_results() -> usize { 5 }
    fn default_timeout() -> u64 { 10 }
}
```

---

## 4. Cron 调度器

### 4.1 goclaw 实现概述

Cron 调度器提供定时任务执行能力，支持：
- Cron 表达式调度
- 任务启用/禁用
- 与消息总线集成
- 任务状态追踪

**核心组件：**
- `Cron`: 基础调度器（秒级精度）
- `Scheduler`: 任务管理器
- `Job`: 定时任务定义

### 4.2 nanors 实现设计

#### 新增 Crate: `nanors_cron`

```
nanors_cron/
├── Cargo.toml
└── src/
    ├── lib.rs              # 公共接口
    ├── schedule.rs         # Cron 表达式解析
    ├── scheduler.rs        # 调度器
    └── job.rs             # 任务定义
```

#### `Cargo.toml`

```toml
[package]
name = "nanors_cron"
version = "0.1.0"
edition = "2024"

[dependencies]
tokio = { version = "1.0", features = ["time", "sync"] }
serde = { version = "1.0", features = ["derive"] }
tracing = "0.1"
anyhow = "1.0"
cron = "0.13"            # cron 表达式解析
parking_lot = "0.12"
```

#### `src/schedule.rs`

```rust
use chrono::{DateTime, Utc};
use cron::Schedule;
use anyhow::Result;

/// 调度接口
pub trait CronSchedule: Send + Sync {
    /// 计算下次执行时间
    fn next(&self, after: DateTime<Utc>) -> Option<DateTime<Utc>>;
}

/// Cron 表达式调度器
pub struct CronExpression {
    inner: Schedule,
}

impl CronExpression {
    /// 解析 cron 表达式
    ///
    /// 支持 5 段或 6 段格式：
    /// - 5 段: 分 时 日 月 周
    /// - 6 段: 秒 分 时 日 月 周
    pub fn parse(expr: &str) -> Result<Self> {
        let inner = Schedule::try_from(expr)?;
        Ok(Self { inner })
    }
}

impl CronSchedule for CronExpression {
    fn next(&self, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
        self.inner.next(after).next()
    }
}

/// 间隔调度器（用于测试或简单场景）
pub struct IntervalSchedule {
    seconds: i64,
}

impl IntervalSchedule {
    pub fn seconds(seconds: i64) -> Self {
        Self { seconds }
    }
}

impl CronSchedule for IntervalSchedule {
    fn next(&self, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
        Some(after + chrono::Duration::seconds(self.seconds))
    }
}
```

#### `src/job.rs`

```rust
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Cron 任务定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJob {
    /// 任务唯一 ID
    pub id: String,

    /// 任务名称
    pub name: String,

    /// Cron 表达式
    pub schedule: String,

    /// 任务内容（发送给 agent 的消息）
    pub task: String,

    /// 目标聊天 ID
    pub target_chat: Option<String>,

    /// 是否启用
    #[serde(default)]
    pub enabled: bool,

    /// 最后运行时间
    #[serde(default)]
    pub last_run: Option<i64>,

    /// 下次运行时间
    #[serde(default)]
    pub next_run: Option<i64>,

    /// 运行次数
    #[serde(default)]
    pub run_count: usize,
}

/// 任务状态
#[derive(Debug, Clone)]
pub struct JobState {
    pub job: Arc<RwLock<CronJob>>,
    pub next: Arc<RwLock<Option<i64>>>,
}

impl JobState {
    pub fn new(job: CronJob) -> Self {
        Self {
            job: Arc::new(RwLock::new(job)),
            next: Arc::new(RwLock::new(None)),
        }
    }
}
```

#### `src/scheduler.rs`

```rust
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};
use tokio::time::{interval, Duration};
use tracing::{error, info, warn};

use super::job::{CronJob, JobState};
use super::schedule::{CronExpression, CronSchedule};

/// 调度器
pub struct Scheduler {
    jobs: Arc<RwLock<HashMap<String, JobState>>>,

    /// 任务执行通知
    executor_tx: mpsc::Sender<JobExecution>,
}

/// 任务执行事件
pub struct JobExecution {
    pub job_id: String,
    pub task: String,
    pub target_chat: Option<String>,
}

impl Scheduler {
    pub fn new() -> (Self, mpsc::Receiver<JobExecution>) {
        let (executor_tx, executor_rx) = mpsc::channel(100);
        let scheduler = Self {
            jobs: Arc::new(RwLock::new(HashMap::new())),
            executor_tx,
        };
        (scheduler, executor_rx)
    }

    /// 启动调度器
    pub async fn start(&self) -> Result<()> {
        info!("Cron scheduler starting");

        let jobs = self.jobs.clone();
        let executor_tx = self.executor_tx.clone();

        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(1));
            ticker.tick().await; // 跳过第一次立即触发

            loop {
                ticker.tick().await;

                let now = Utc::now();
                let jobs_guard = jobs.read().await;

                for (id, state) in jobs_guard.iter() {
                    let job = state.job.read().await;
                    if !job.enabled {
                        continue;
                    }

                    let next_guard = state.next.read().await;
                    let next = match *next_guard {
                        Some(ts) => DateTime::<Utc>::from_timestamp(ts, 0)
                            .unwrap_or_else(|| DateTime::from_timestamp(0, 0).unwrap()),
                        None => continue,
                    };
                    drop(next_guard);

                    if now >= next {
                        drop(job);
                        drop(jobs_guard);

                        // 执行任务
                        let job_id = id.clone();
                        let task = state.job.read().await.task.clone();
                        let target_chat = state.job.read().await.target_chat.clone();

                        if let Err(e) = executor_tx.send(JobExecution {
                            job_id,
                            task,
                            target_chat,
                        }).await {
                            error!("Failed to send job execution: {}", e);
                        }

                        // 更新下次执行时间
                        self.update_next_run(id).await;

                        let jobs_guard = jobs.read().await;
                    }
                }
            }
        });

        Ok(())
    }

    /// 添加任务
    pub async fn add_job(&self, job: CronJob) -> Result<()> {
        let id = job.id.clone();

        // 检查是否已存在
        {
            let jobs = self.jobs.read().await;
            if jobs.contains_key(&id) {
                anyhow::bail!("Job {} already exists", id);
            }
        }

        let state = JobState::new(job);

        // 计算下次执行时间
        if state.job.read().await.enabled {
            self.update_next_run(&id).await;
        }

        let mut jobs = self.jobs.write().await;
        jobs.insert(id, state);

        info!("Cron job added: {}", id);
        Ok(())
    }

    /// 移除任务
    pub async fn remove_job(&self, id: &str) -> Result<()> {
        let mut jobs = self.jobs.write().await;
        if jobs.remove(id).is_some() {
            info!("Cron job removed: {}", id);
            Ok(())
        } else {
            anyhow::bail!("Job {} not found", id);
        }
    }

    /// 启用任务
    pub async fn enable_job(&self, id: &str) -> Result<()> {
        let jobs = self.jobs.read().await;
        let state = jobs.get(id)
            .ok_or_else(|| anyhow::anyhow!("Job {} not found", id))?;

        {
            let mut job = state.job.write().await;
            if job.enabled {
                return Ok(());
            }
            job.enabled = true;
        }

        drop(jobs);
        self.update_next_run(id).await;

        info!("Cron job enabled: {}", id);
        Ok(())
    }

    /// 禁用任务
    pub async fn disable_job(&self, id: &str) -> Result<()> {
        let jobs = self.jobs.read().await;
        let state = jobs.get(id)
            .ok_or_else(|| anyhow::anyhow!("Job {} not found", id))?;

        {
            let mut job = state.job.write().await;
            if !job.enabled {
                return Ok(());
            }
            job.enabled = false;
        }

        info!("Cron job disabled: {}", id);
        Ok(())
    }

    /// 列出所有任务
    pub async fn list_jobs(&self) -> Vec<CronJob> {
        let jobs = self.jobs.read().await;
        jobs.values()
            .map(|s| s.job.read().await.clone())
            .collect()
    }

    /// 更新下次执行时间
    async fn update_next_run(&self, id: &str) {
        let jobs = self.jobs.read().await;
        let Some(state) = jobs.get(id) else {
            return;
        };

        let job = state.job.read().await;
        if !job.enabled {
            return;
        }

        let schedule_expr = job.schedule.clone();
        drop(job);

        let schedule = match CronExpression::parse(&schedule_expr) {
            Ok(s) => s,
            Err(e) => {
                warn!("Invalid cron expression for {}: {}", id, e);
                return;
            }
        };

        let now = Utc::now();
        let next = schedule.next(now).map(|dt| dt.timestamp());

        {
            let mut state_next = state.next.write().await;
            *state_next = next;

            if let Some(next_ts) = next {
                let mut job = state.job.write().await;
                job.next_run = Some(next_ts);
            }
        }
    }

    /// 获取任务
    pub async fn get_job(&self, id: &str) -> Option<CronJob> {
        let jobs = self.jobs.read().await;
        jobs.get(id).map(|s| s.job.read().await.clone())
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        let (scheduler, _) = Self::new();
        scheduler
    }
}
```

#### `src/lib.rs`

```rust
mod job;
mod schedule;
mod scheduler;

pub use job::{CronJob, JobState};
pub use schedule::{CronExpression, CronSchedule, IntervalSchedule};
pub use scheduler::{Scheduler, JobExecution};
```

### 4.3 集成到 nanors_core

#### 修改 `nanors_core/src/agent/mod.rs`

```rust
pub mod agent_loop;
pub mod cron_runner;  // 新增

pub use cron_runner::CronRunner;
```

#### 新增文件: `nanors_core/src/agent/cron_runner.rs`

```rust
use anyhow::{Context, Result};
use nanors_cron::{JobExecution, Scheduler};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info};

/// Cron 运行器
pub struct CronRunner {
    _scheduler: Arc<Scheduler>,
}

impl CronRunner {
    pub fn new(
        scheduler: Arc<Scheduler>,
        mut executor_rx: mpsc::Receiver<JobExecution>,
    ) -> Result<Self> {
        // 启动任务执行处理
        tokio::spawn(async move {
            while let Some(execution) = executor_rx.recv().await {
                info!("Executing cron job: {}", execution.job_id);

                // 这里可以将任务发送给 agent 处理
                // 或者直接执行预定义的操作

                if let Err(e) = Self::execute_job(execution).await {
                    error!("Job execution failed: {}", e);
                }
            }
        });

        Ok(Self {
            _scheduler: scheduler,
        })
    }

    async fn execute_job(execution: JobExecution) -> Result<()> {
        // 这里可以实现任务的实际执行逻辑
        // 例如：
        // 1. 将任务消息发送给 AgentLoop
        // 2. 或者调用特定的工具
        // 3. 或者发送到外部消息通道

        info!(
            "Job {} executed: {}",
            execution.job_id, execution.task
        );

        Ok(())
    }
}
```

### 4.4 配置集成

#### 修改 `nanors_config/src/schema.rs`

```rust
#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct Config {
    #[serde(default)]
    pub agents: AgentsConfig,
    #[serde(default)]
    pub providers: ProvidersConfig,
    #[serde(default)]
    pub database: DatabaseConfig,
    #[serde(default)]
    pub memory: MemoryConfig,
    #[serde(default)]
    pub telegram: TelegramConfig,
    #[serde(default)]
    pub tools: ToolsConfig,
    #[serde(default)]
    pub cron: CronConfig,  // 新增
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct CronConfig {
    /// 预定义的任务
    #[serde(default)]
    pub jobs: Vec<CronJobDef>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CronJobDef {
    pub id: String,
    pub name: String,
    pub schedule: String,
    pub task: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_chat: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool { true }
```

---

## 5. 配置集成

### 5.1 更新 Cargo.toml 依赖

#### `nanors/Cargo.toml`

```toml
[workspace.dependencies]
# 新增依赖
nanors_skills = { path = "nanors_skills" }
nanors_cron = { path = "nanors_cron" }

# 新增外部依赖
tokio-cron-schedule = "0.6"
cron = "0.13"
notify = "7.0"
parking_lot = "0.12"
dirs = "5.0"
chrono = "0.4"

# nanors_tools 新增依赖
url = "2.5"
```

#### `nanors_tools/Cargo.toml`

```toml
[dependencies]
# 新增
url = "2.5"
```

### 5.2 示例配置文件

#### `~/.nanors/config.json`

```json
{
  "agents": {
    "defaults": {
      "model": "glm-4.7-flash",
      "max_tokens": 8192,
      "temperature": 0.7
    }
  },
  "providers": {
    "zhipu": {
      "api_key": "your-zhipu-api-key-here"
    }
  },
  "database": {
    "url": "postgresql://user:pass@localhost:5432/nanors"
  },
  "tools": {
    "web_fetch": {
      "timeout": 10,
      "max_size": 1000000
    },
    "web_search": {
      "api": "tavily",
      "api_key": "your-tavily-api-key",
      "max_results": 5
    }
  },
  "cron": {
    "jobs": [
      {
        "id": "daily-reminder",
        "name": "Daily Reminder",
        "schedule": "0 9 * * *",
        "task": "Remind me to check my tasks",
        "enabled": true
      }
    ]
  },
  "skills": {
    "enabled": true,
    "extra_dirs": [],
    "watch": true
  }
}
```

---

## 6. 实施步骤

### Phase 1: 基础工具 (Week 1)

1. **web_fetch 工具**
   - [ ] 创建 `nanors_tools/src/web_fetch.rs`
   - [ ] 实现基础 HTTP 抓取
   - [ ] 实现 HTML 转纯文本
   - [ ] 添加测试
   - [ ] 更新配置

2. **web_search 工具**
   - [ ] 创建 `nanors_tools/src/web_search.rs`
   - [ ] 实现 Tavily API 集成
   - [ ] 实现 Serper API 集成
   - [ ] 添加 mock 模式
   - [ ] 更新配置

### Phase 2: Skills 系统 (Week 2-3)

1. **核心结构**
   - [ ] 创建 `nanors_skills` crate
   - [ ] 实现 `skill.rs` 数据结构
   - [ ] 实现 `frontmatter.rs` 解析
   - [ ] 添加测试

2. **发现和加载**
   - [ ] 实现 `discovery.rs`
   - [ ] 实现递归目录扫描
   - [ ] 实现优先级合并
   - [ ] 添加测试

3. **环境检测**
   - [ ] 实现 `eligibility.rs`
   - [ ] 实现 OS 兼容性检测
   - [ ] 实现二进制检测
   - [ ] 实现环境变量检测

4. **集成**
   - [ ] 修改 `nanors_core` 集成技能系统
   - [ ] 实现热重载（可选）

### Phase 3: Cron 调度器 (Week 4)

1. **核心调度器**
   - [ ] 创建 `nanors_cron` crate
   - [ ] 实现 `schedule.rs`
   - [ ] 实现 `scheduler.rs`
   - [ ] 实现 `job.rs`

2. **集成**
   - [ ] 创建 `nanors_core/src/agent/cron_runner.rs`
   - [ ] 更新配置
   - [ ] 添加 CLI 命令管理任务

### Phase 4: 文档和测试 (Week 5)

1. **文档**
   - [ ] 更新 README.md
   - [ ] 添加 API 文档
   - [ ] 添加示例

2. **测试**
   - [ ] 单元测试
   - [ ] 集成测试
   - [ ] 端到端测试

---

## 7. 关键决策记录

### 7.1 Skills 目录结构

**决策**: 使用 workspace > managed > bundled 优先级

**理由**:
- 用户可以覆盖内置技能
- 项目技能优先于全局技能
- 与 goclaw 行为一致

### 7.2 Web 搜索 API 选择

**决策**: 默认使用 Tavily，支持 Serper

**理由**:
- Tavily 有免费额度，专为 AI 设计
- Serper 作为 Google 搜索备选
- Mock 模式用于无 API Key 场景

### 7.3 Cron 精度

**决策**: 秒级精度，使用 `tokio::time::interval`

**理由**:
- nanors 是异步架构，适合 tokio
- 秒级精度满足大多数场景
- 比 cron crate 更轻量

---

## 附录

### A. 依赖版本

| Crate | 版本 | 用途 |
|-------|------|------|
| tokio | 1.49+ | 异步运行时 |
| reqwest | 0.13+ | HTTP 客户端 |
| serde | 1.0+ | 序列化 |
| serde_json | 1.0+ | JSON 解析 |
| serde_yaml | 0.9+ | YAML 解析 |
| chrono | 0.4+ | 时间处理 |
| cron | 0.13+ | Cron 表达式 |
| notify | 7.0+ | 文件监听 |
| parking_lot | 0.12+ | 高性能锁 |
| url | 2.5+ | URL 解析 |
| anyhow | 1.0+ | 错误处理 |
| tracing | 0.1+ | 日志 |

### B. API 密钥获取

- **Tavily**: https://tavily.com
- **Serper**: https://serper.dev
- **Zhipu**: https://open.bigmodel.cn

---

*文档版本: 1.0*
*更新日期: 2026-02-13*
