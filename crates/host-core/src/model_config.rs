//! Credential-blind configuration editing. The host owns paths and preserves opaque YAML fields.
use crate::{HostError, runtime::Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};
static EDIT: Mutex<()> = Mutex::new(());
const LIMIT: u64 = 8 * 1024 * 1024;
const MODEL_FIELDS: &[&str] = &[
    "id",
    "name",
    "api",
    "contextWindow",
    "maxTokens",
    "reasoning",
    "thinking",
    "input",
    "cost",
];
const APIS: &[&str] = &[
    "openai-completions",
    "openai-responses",
    "openai-codex-responses",
    "azure-openai-responses",
    "anthropic-messages",
    "bedrock-converse-stream",
    "google-generative-ai",
    "google-gemini-cli",
    "google-vertex",
];
fn err(code: &str, message: &str) -> HostError {
    HostError::new(code, message)
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Provider {
    pub id: String,
    pub base_url: Option<String>,
    pub api: Option<String>,
    pub auth: Option<String>,
    pub auth_header: Option<bool>,
    pub credential_configured: bool,
    pub models: Vec<Value>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub path: String,
    pub exists: bool,
    pub revision: String,
    pub providers: Vec<Provider>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Edit {
    pub original_id: Option<String>,
    pub revision: String,
    pub provider: Provider,
    pub credential_action: String,
    pub credential: Option<String>,
    pub deleted: bool,
}
pub fn config_path() -> Result<PathBuf> {
    let root = if let Some(p) = std::env::var_os("PI_CODING_AGENT_DIR") {
        PathBuf::from(p)
    } else {
        PathBuf::from(
            std::env::var_os("HOME").ok_or_else(|| err("config_path", "无法定位用户目录"))?,
        )
        .join(".omp/agent")
    };
    if !root.is_absolute() {
        return Err(err("config_path", "OMP 配置目录必须是绝对路径"));
    }
    let yml = root.join("models.yml");
    let yaml = root.join("models.yaml");
    if !yml.exists() && yaml.exists() {
        Ok(yaml)
    } else if !yml.exists() && root.join("models.json").exists() {
        Err(err(
            "config_legacy",
            "请先运行 OMP 完成旧 models.json 的官方迁移，再重新加载",
        ))
    } else {
        Ok(yml)
    }
}
fn revision(path: &Path) -> Result<String> {
    match fs::metadata(path) {
        Ok(m) => Ok(format!(
            "{}:{}",
            m.len(),
            m.modified()
                .map_err(|_| err("config_read", "无法读取配置修改时间"))?
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok("missing".into()),
        Err(_) => Err(err("config_read", "无法读取配置修改时间")),
    }
}
fn read(path: &Path) -> Result<Value> {
    let meta = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(json!({"providers":{}})),
        Err(_) => return Err(err("config_read", "无法读取模型配置")),
    };
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > LIMIT {
        return Err(err(
            "config_path",
            "配置必须是小于 8 MiB 的普通文件，不能是符号链接",
        ));
    }
    let bytes = fs::read(path).map_err(|_| err("config_read", "无法读取模型配置"))?;
    let doc: Value = serde_yaml_ng::from_slice(&bytes).map_err(|_| {
        err(
            "config_parse",
            "模型 YAML 解析失败；原文件未修改，请检查语法",
        )
    })?;
    if !doc["providers"].is_object() {
        return Err(err("config_parse", "配置需要 providers 对象"));
    }
    Ok(doc)
}
fn public_provider(id: &str, v: &Value) -> Provider {
    Provider {
        id: id.into(),
        base_url: v["baseUrl"]
            .as_str()
            .filter(|s| safe_url(s))
            .map(str::to_owned),
        api: v["api"].as_str().map(str::to_owned),
        auth: v["auth"].as_str().map(str::to_owned),
        auth_header: v["authHeader"].as_bool(),
        credential_configured: v.get("apiKey").is_some(),
        models: v["models"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|m| {
                let mut x = json!({});
                for k in MODEL_FIELDS {
                    if let Some(v) = m.get(*k) {
                        x[*k] = v.clone();
                    }
                }
                x["originalId"] = m["id"].clone();
                x
            })
            .collect(),
    }
}
pub fn load(path: &Path) -> Result<Config> {
    let doc = read(path)?;
    Ok(Config {
        path: path.to_string_lossy().into(),
        exists: path.exists(),
        revision: revision(path)?,
        providers: doc["providers"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(id, v)| public_provider(id, v))
            .collect(),
    })
}
fn safe_url(s: &str) -> bool {
    reqwest::Url::parse(s).is_ok_and(|u| {
        matches!(u.scheme(), "http" | "https")
            && u.host_str().is_some()
            && u.username().is_empty()
            && u.password().is_none()
            && u.query().is_none()
            && u.fragment().is_none()
    })
}
fn valid_id(s: &str) -> bool {
    !s.trim().is_empty() && s == s.trim() && s.len() <= 512 && !s.chars().any(char::is_control)
}
fn validate(p: &Provider) -> Result<()> {
    if !valid_id(&p.id) {
        return Err(err(
            "config_validation",
            "Provider ID 不能为空或包含控制字符",
        ));
    }
    if let Some(s) = &p.base_url {
        if !safe_url(s) {
            return Err(err(
                "config_validation",
                "基础地址必须为无内嵌凭据和查询参数的 HTTP/HTTPS URL",
            ));
        }
    }
    if p.api.as_ref().is_some_and(|s| !APIS.contains(&s.as_str())) {
        return Err(err("config_validation", "不支持该 API 类型"));
    }
    if p.auth
        .as_ref()
        .is_some_and(|s| !matches!(s.as_str(), "apiKey" | "none" | "oauth"))
    {
        return Err(err("config_validation", "认证方式无效"));
    }
    let mut ids = HashSet::new();
    for m in &p.models {
        let id = m["id"].as_str().unwrap_or_default();
        if !valid_id(id) || !ids.insert(id) {
            return Err(err("config_validation", "模型 ID 不能为空或重复"));
        }
        if !m.is_object()
            || m.as_object()
                .unwrap()
                .keys()
                .any(|k| k != "originalId" && !MODEL_FIELDS.contains(&k.as_str()))
        {
            return Err(err("config_validation", "模型包含不支持的编辑字段"));
        }
        if let Some(v) = m.get("name") {
            if !v.as_str().is_some_and(valid_id) {
                return Err(err("config_validation", "显示名称无效"));
            }
        }
        if let Some(v) = m.get("api") {
            if !v.as_str().is_some_and(|s| APIS.contains(&s)) {
                return Err(err("config_validation", "模型 API 类型无效"));
            }
        }
        if p.api.is_none() && m.get("api").is_none() {
            return Err(err("config_validation", "请设置 Provider 或模型 API 类型"));
        }
        for k in ["contextWindow", "maxTokens"] {
            if let Some(v) = m.get(k) {
                if !v.as_u64().is_some_and(|n| n > 0 && n <= 1_000_000_000) {
                    return Err(err("config_validation", "上下文和输出上限必须是正整数"));
                }
            }
        }
        if m.get("reasoning").is_some_and(|v| !v.is_boolean()) {
            return Err(err("config_validation", "reasoning 必须是布尔值"));
        }
        if let Some(v) = m.get("input") {
            if !v.as_array().is_some_and(|a| {
                !a.is_empty()
                    && a.iter()
                        .all(|v| matches!(v.as_str(), Some("text" | "image")))
            }) {
                return Err(err("config_validation", "输入类型只能包含 text / image"));
            }
        }
        if let Some(v) = m.get("cost") {
            if !["input", "output", "cacheRead", "cacheWrite"]
                .iter()
                .all(|k| v[*k].as_f64().is_some_and(|n| n >= 0.0))
            {
                return Err(err("config_validation", "价格需要四项非负数值"));
            }
        }
        if let Some(v) = m.get("thinking") {
            let efforts = ["minimal", "low", "medium", "high", "xhigh", "max"];
            if !v["mode"].as_str().is_some_and(|s| {
                [
                    "effort",
                    "budget",
                    "google-level",
                    "anthropic-adaptive",
                    "anthropic-budget-effort",
                ]
                .contains(&s)
            }) {
                return Err(err("config_validation", "thinking.mode 无效"));
            }
            let levels = v.get("efforts").or_else(|| v.get("levels"));
            if let Some(a) = levels {
                if !a.as_array().is_some_and(|a| {
                    !a.is_empty()
                        && a.iter()
                            .all(|x| x.as_str().is_some_and(|s| efforts.contains(&s)))
                }) {
                    return Err(err(
                        "config_validation",
                        "thinking.efforts 必须包含有效推理等级",
                    ));
                }
            } else if !(v["minLevel"].as_str().is_some_and(|s| efforts.contains(&s))
                && v["maxLevel"].as_str().is_some_and(|s| efforts.contains(&s)))
            {
                return Err(err(
                    "config_validation",
                    "thinking 需要 efforts 推理等级列表",
                ));
            }
            for k in ["defaultLevel", "minLevel", "maxLevel"] {
                if v.get(k)
                    .is_some_and(|x| !x.as_str().is_some_and(|s| efforts.contains(&s)))
                {
                    return Err(err("config_validation", "thinking 推理等级无效"));
                }
            }
            for k in ["supportsDisplay", "requiresEffort"] {
                if v.get(k).is_some_and(|x| !x.is_boolean()) {
                    return Err(err("config_validation", "thinking 开关必须为布尔值"));
                }
            }
        }
    }
    if !p.models.is_empty() && p.base_url.is_none() {
        return Err(err("config_validation", "自定义模型需要基础地址"));
    }
    Ok(())
}
fn merge(mut doc: Value, e: &Edit) -> Result<Value> {
    let providers = doc["providers"].as_object_mut().unwrap();
    if e.deleted {
        let id = e
            .original_id
            .as_ref()
            .ok_or_else(|| err("config_validation", "删除需要现有 Provider"))?;
        providers.remove(id);
        return Ok(doc);
    }
    validate(&e.provider)?;
    let p = &e.provider;
    if e.original_id.as_deref() != Some(&p.id) && providers.contains_key(&p.id) {
        return Err(err("config_conflict", "Provider ID 已存在"));
    }
    let mut current = e
        .original_id
        .as_ref()
        .and_then(|id| providers.get(id))
        .cloned()
        .unwrap_or(json!({}));
    if !current.is_object() {
        return Err(err("config_parse", "Provider 结构无效"));
    }
    for (key, value) in [
        ("baseUrl", serde_json::to_value(&p.base_url).unwrap()),
        ("api", serde_json::to_value(&p.api).unwrap()),
        ("auth", serde_json::to_value(&p.auth).unwrap()),
        ("authHeader", serde_json::to_value(p.auth_header).unwrap()),
    ] {
        if value.is_null() {
            // A legacy URL with embedded auth is deliberately not returned to the UI.
            // Leaving its blank field unchanged must not destroy the opaque existing value.
            if key == "baseUrl" && current[key].as_str().is_some_and(|url| !safe_url(url)) {
                continue;
            }
            current.as_object_mut().unwrap().remove(key);
        } else {
            current[key] = value;
        }
    }
    match e.credential_action.as_str() {
        "keep" => {}
        "clear" => {
            current.as_object_mut().unwrap().remove("apiKey");
        }
        "replace" => {
            let key = e
                .credential
                .as_deref()
                .filter(|s| !s.trim().is_empty() && s.len() < 16384 && !s.starts_with('!'))
                .ok_or_else(|| {
                    err(
                        "config_validation",
                        "请输入密钥或环境变量名；不支持从界面新增命令型凭据",
                    )
                })?;
            current["apiKey"] = json!(key);
        }
        _ => return Err(err("config_validation", "凭据操作无效")),
    }
    if !p.models.is_empty() && p.auth.as_deref() != Some("none") && current.get("apiKey").is_none()
    {
        return Err(err(
            "config_validation",
            "请配置 API Key / 环境变量，或选择无需认证",
        ));
    }
    if p.models.is_empty()
        && current["auth"] != "none"
        && ![
            "baseUrl",
            "apiKey",
            "headers",
            "compat",
            "disableStrictTools",
            "modelOverrides",
            "discovery",
            "remoteCompaction",
        ]
        .iter()
        .any(|k| current.get(*k).is_some())
    {
        return Err(err(
            "config_validation",
            "Provider 至少需要地址、认证或覆盖配置",
        ));
    }
    let old = current["models"].as_array().cloned().unwrap_or_default();
    let models: Vec<_> = p
        .models
        .iter()
        .map(|m| {
            let mut out = old
                .iter()
                .find(|o| o["id"] == *m.get("originalId").unwrap_or(&m["id"]))
                .cloned()
                .unwrap_or(json!({}));
            for k in MODEL_FIELDS {
                out.as_object_mut().unwrap().remove(*k);
                if let Some(v) = m.get(*k) {
                    out[*k] = v.clone();
                }
            }
            out
        })
        .collect();
    current["models"] = json!(models);
    if let Some(id) = &e.original_id {
        providers.remove(id);
    }
    providers.insert(p.id.clone(), current);
    Ok(doc)
}
pub fn save(path: &Path, e: &Edit) -> Result<Config> {
    let _guard = EDIT
        .lock()
        .map_err(|_| err("config_write", "配置写入锁不可用"))?;
    if revision(path)? != e.revision {
        return Err(err(
            "config_conflict",
            "配置已被其他程序修改，请重载后再编辑",
        ));
    }
    let doc = merge(read(path)?, e)?;
    let data = serde_yaml_ng::to_string(&doc).map_err(|_| err("config_write", "配置编码失败"))?;
    let parent = path
        .parent()
        .ok_or_else(|| err("config_path", "配置目录无效"))?;
    if data.len() as u64 > LIMIT {
        return Err(err("config_validation", "配置超过 8 MiB，未写入"));
    }
    fs::create_dir_all(parent).map_err(|_| err("config_write", "无法创建配置目录"))?;
    // Same-directory rename is atomic. Never truncate the user's original configuration.
    let temp = parent.join(format!(".models-{}.tmp", uuid::Uuid::new_v4()));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temp)
        .map_err(|_| err("config_write", "无法创建安全临时配置"))?;
    file.write_all(data.as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|_| err("config_write", "写入失败，原配置保留"))?;
    if revision(path)? != e.revision {
        return Err(err("config_conflict", "写入前发现外部修改，原文件已保留"));
    }
    fs::rename(&temp, path).map_err(|_| err("config_write", "原子替换失败，原配置保留"))?;
    load(path)
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub version: String,
    pub source: String,
    pub cached: bool,
    pub models: Vec<Value>,
}
pub async fn catalog(cache: &Path, version: &str) -> Result<Catalog> {
    semver::Version::parse(version).map_err(|_| err("catalog_unavailable", "无法确定 OMP 版本"))?;
    let source = format!(
        "https://raw.githubusercontent.com/can1357/oh-my-pi/v{version}/packages/catalog/src/models.json"
    );
    let path = cache.join(format!("omp-catalog-{version}.json"));
    let (bytes, cached) = if let Ok(b) = fs::read(&path) {
        (b, true)
    } else {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(25))
            .build()
            .map_err(|_| err("catalog_unavailable", "目录请求初始化失败"))?;
        let mut response = client
            .get(&source)
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|_| {
                err(
                    "catalog_unavailable",
                    "该 OMP 版本的预置目录获取失败，且无缓存；可继续手动配置",
                )
            })?;
        if response.content_length().unwrap_or(0) > 32 * 1024 * 1024 {
            return Err(err("catalog_unavailable", "目录过大"));
        }
        let mut b = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| err("catalog_unavailable", "目录下载失败"))?
        {
            if b.len() + chunk.len() > 32 * 1024 * 1024 {
                return Err(err("catalog_unavailable", "目录过大"));
            }
            b.extend_from_slice(&chunk);
        }
        (b, false)
    };
    let doc: Value =
        serde_json::from_slice(&bytes).map_err(|_| err("catalog_unavailable", "目录内容无效"))?;
    let mut models = Vec::new();
    for provider in doc
        .as_object()
        .ok_or_else(|| err("catalog_unavailable", "目录格式无效"))?
        .values()
    {
        for m in provider.as_object().into_iter().flat_map(|v| v.values()) {
            if m["id"].is_string() {
                let mut x = json!({"provider":m["provider"],"baseUrl":m["baseUrl"]});
                for k in MODEL_FIELDS {
                    if let Some(v) = m.get(*k) {
                        x[*k] = v.clone();
                    }
                }
                models.push(x);
            }
        }
    }
    if !cached {
        fs::create_dir_all(cache)
            .and_then(|_| fs::write(&path, &bytes))
            .map_err(|_| err("catalog_cache", "目录已获取但缓存写入失败"))?;
    }
    Ok(Catalog {
        version: version.into(),
        source,
        cached,
        models,
    })
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Verification {
    pub default_model: Option<String>,
    pub project_model: Option<String>,
    pub stage: String,
    pub message: String,
    pub models: Vec<Value>,
}
// Do not forward OMP output: it can contain resolved headers, keys, URLs or echoed requests.
async fn output(
    executable: &Path,
    cwd: &Path,
    args: &[String],
) -> Result<(bool, Vec<u8>, Vec<u8>)> {
    use tokio::io::AsyncReadExt;
    fs::create_dir_all(cwd).map_err(|_| err("config_verify", "无法创建模型验证目录"))?;
    let mut child = crate::runtime::command(executable)
        .args(args)
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|_| err("config_verify", "无法启动所选 OMP"))?;
    let mut stdout = child.stdout.take().unwrap().take(32 * 1024 * 1024);
    let mut stderr = child.stderr.take().unwrap().take(1024 * 1024);
    let result = tokio::time::timeout(std::time::Duration::from_secs(90), async {
        let mut out = Vec::new();
        let mut errors = Vec::new();
        let (a, b) = tokio::join!(
            stdout.read_to_end(&mut out),
            stderr.read_to_end(&mut errors)
        );
        a.map_err(|_| err("config_verify", "读取验证输出失败"))?;
        b.map_err(|_| err("config_verify", "读取验证结果失败"))?;
        let status = child
            .wait()
            .await
            .map_err(|_| err("config_verify", "验证进程异常"))?;
        Ok((status.success(), out, errors))
    })
    .await;
    match result {
        Ok(v) => v,
        Err(_) => {
            crate::runtime::reap(&mut child).await?;
            Err(err(
                "connection_timeout",
                "OMP 验证超时，请检查网络和服务地址",
            ))
        }
    }
}
pub async fn discover(executable: &Path, cwd: &Path) -> Result<Verification> {
    let (ok, out, errors) = output(
        executable,
        cwd,
        &["models".into(), "--json".into(), "--no-extensions".into()],
    )
    .await?;
    if !ok || !errors.is_empty() {
        return Err(err(
            "config_load_failed",
            "OMP 加载模型配置失败或产生诊断；请检查配置格式和认证来源",
        ));
    }
    let doc: Value = serde_json::from_slice(&out)
        .map_err(|_| err("config_load_failed", "OMP 未返回有效模型列表"))?;
    let list = doc["models"]
        .as_array()
        .ok_or_else(|| err("config_load_failed", "OMP 模型列表格式无效"))?;
    let models = list
        .iter()
        .map(|m| json!({"id":m["id"],"provider":m["provider"],"name":m["name"]}))
        .collect();
    let (ok, out, errors) = output(executable, cwd, &["config".into(), "get".into(), "modelRoles".into(), "--json".into()]).await?;
    if !ok || !errors.is_empty() {
        return Err(err("config_load_failed", "无法读取 OMP 默认模型配置"));
    }
    let config: Value = serde_json::from_slice(&out)
        .map_err(|_| err("config_load_failed", "OMP 默认模型配置格式无效"))?;
    let default_model = config["value"]["default"].as_str().map(str::to_owned);
    Ok(Verification {
        default_model,
        project_model: None,
        stage: "loaded".into(),
        message: "OMP 已加载配置；模型列表不代表真实连接成功".into(),
        models,
    })
}
pub async fn connect(
    executable: &Path,
    cwd: &Path,
    provider: &str,
    id: &str,
) -> Result<Verification> {
    let available = discover(executable, cwd).await?;
    if !available
        .models
        .iter()
        .any(|m| m["provider"] == provider && m["id"] == id)
    {
        return Err(err(
            "model_unavailable",
            "该模型未被 OMP 发现，请先保存配置并检查认证",
        ));
    }
    let args = vec![
        "--provider".into(),
        provider.into(),
        "--model".into(),
        id.into(),
        "--mode".into(),
        "json".into(),
        "--print".into(),
        "--no-session".into(),
        "--no-tools".into(),
        "--no-extensions".into(),
        "--no-skills".into(),
        "--no-rules".into(),
        "--no-lsp".into(),
        "--no-pty".into(),
        "--no-title".into(),
        "Reply with exactly OK.".into(),
    ];
    let (ok, out, errors) = output(executable, cwd, &args).await?;
    let combined = format!(
        "{} {}",
        String::from_utf8_lossy(&out),
        String::from_utf8_lossy(&errors)
    )
    .to_lowercase();
    if combined.contains("401")
        || combined.contains("403")
        || combined.contains("unauthorized")
        || combined.contains("invalid api key")
    {
        return Err(err(
            "authentication_failed",
            "认证失败，请检查密钥或环境变量权限",
        ));
    }
    let mut success = false;
    let mut failed = false;
    for line in out.split(|b| *b == b'\n') {
        if let Ok(v) = serde_json::from_slice::<Value>(line) {
            let m = if v["type"] == "message_end" {
                &v["message"]
            } else {
                &v
            };
            if m["role"] == "assistant" {
                if matches!(m["stopReason"].as_str(), Some("error" | "aborted")) {
                    failed = true;
                } else if m["content"].as_array().is_some_and(|a| {
                    a.iter().any(|p| {
                        p["type"] == "text" && p["text"].as_str().is_some_and(|s| !s.is_empty())
                    })
                }) {
                    success = true;
                }
            }
        }
    }
    if !ok || failed || !success {
        return Err(err(
            "protocol_failed",
            "模型调用未成功完成；请检查 API 类型、请求 ID 和网关协议",
        ));
    }
    Ok(Verification {
        default_model: available.default_model,
        project_model: available.project_model,
        stage: "connected".into(),
        message: "真实连接成功：OMP 已收到该模型的完整回复".into(),
        models: available.models,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn path() -> PathBuf {
        let root = std::env::temp_dir().join(format!("model-config-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        root.join("models.yml")
    }
    fn edit(path: &Path) -> Edit {
        Edit {
            revision: revision(path).unwrap(),
            original_id: None,
            provider: Provider {
                id: "test".into(),
                base_url: Some("https://example.com/v1".into()),
                api: Some("openai-completions".into()),
                auth: Some("none".into()),
                auth_header: None,
                credential_configured: false,
                models: vec![json!({"id":"alias","contextWindow":12000})],
            },
            credential_action: "keep".into(),
            credential: None,
            deleted: false,
        }
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn connection_errors_are_classified_without_echoing_output() {
        use std::os::unix::fs::PermissionsExt;
        let config = path();
        let exe = config.with_file_name("omp-fixture");
        for (raw, expected) in [
            ("401 fixture-credential", "authentication_failed"),
            ("invalid protocol fixture-credential", "protocol_failed"),
        ] {
            fs::write(&exe, format!("#!/bin/sh\nif [ \"$1\" = models ]; then\n echo '{{\"models\":[{{\"provider\":\"test\",\"id\":\"model\"}}]}}'\nelif [ \"$1\" = config ]; then\n echo '{{\"value\":{{}}}}'\nelse\n echo '{}' >&2\n exit 1\nfi\n", raw)).unwrap();
            fs::set_permissions(&exe, fs::Permissions::from_mode(0o700)).unwrap();
            let error = connect(&exe, config.parent().unwrap(), "test", "model")
                .await
                .err()
                .unwrap();
            assert_eq!(error.code, expected);
            assert!(!error.to_string().contains("fixture-credential"));
        }
    }
    #[test]
    fn first_use_and_atomic_save() {
        let p = path();
        assert!(!load(&p).unwrap().exists);
        save(&p, &edit(&p)).unwrap();
        let c = load(&p).unwrap();
        assert_eq!(c.providers[0].models[0]["contextWindow"], 12000);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(p).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }
    #[test]
    fn preserve_opaque_fields_and_never_return_credentials() {
        let p = path();
        fs::write(&p,"providers:\n  test:\n    apiKey: fixture-secret\n    headers: {Authorization: fixture-header}\n    custom: {future: true}\n    models: [{id: alias, compat: {supportsDeveloperRole: false}}]\n  untouched: {apiKey: other-secret, discovery: {type: ollama}}\n").unwrap();
        let c = load(&p).unwrap();
        let encoded = serde_json::to_string(&c).unwrap();
        assert!(!encoded.contains("secret"));
        assert!(!encoded.contains("fixture-header"));
        let mut e = edit(&p);
        e.original_id = Some("test".into());
        e.provider.models = vec![json!({"id":"renamed","originalId":"alias"})];
        save(&p, &e).unwrap();
        let d = read(&p).unwrap();
        assert_eq!(d["providers"]["test"]["apiKey"], "fixture-secret");
        assert_eq!(
            d["providers"]["test"]["models"][0]["compat"]["supportsDeveloperRole"],
            false
        );
        assert_eq!(d["providers"]["untouched"]["apiKey"], "other-secret");
        assert_eq!(d["providers"]["test"]["custom"]["future"], true);
    }
    #[test]
    fn invalid_edit_keeps_original_and_stale_write_fails() {
        let p = path();
        save(&p, &edit(&p)).unwrap();
        let before = fs::read(&p).unwrap();
        let mut e = edit(&p);
        e.original_id = Some("test".into());
        e.provider.models[0]["maxTokens"] = json!(-1);
        assert_eq!(save(&p, &e).err().unwrap().code, "config_validation");
        assert_eq!(fs::read(&p).unwrap(), before);
        e.revision = "old".into();
        assert_eq!(save(&p, &e).err().unwrap().code, "config_conflict");
    }
    #[test]
    fn malformed_yaml_and_symlinks_are_rejected_without_echo() {
        let p = path();
        fs::write(&p, "providers: [secret: [\n").unwrap();
        let error = load(&p).err().unwrap();
        assert_eq!(error.code, "config_parse");
        assert!(!error.message.contains("secret"));
        #[cfg(unix)]
        {
            let link = p.with_extension("yaml");
            std::os::unix::fs::symlink(&p, &link).unwrap();
            assert_eq!(load(&link).err().unwrap().code, "config_path");
        }
    }
    #[test]
    fn credentials_replace_clear_and_delete_are_explicit() {
        let p = path();
        let mut e = edit(&p);
        e.credential_action = "replace".into();
        e.credential = Some("TEST_KEY_ENV".into());
        save(&p, &e).unwrap();
        assert!(load(&p).unwrap().providers[0].credential_configured);
        e.revision = revision(&p).unwrap();
        e.original_id = Some("test".into());
        e.credential_action = "clear".into();
        save(&p, &e).unwrap();
        assert!(!load(&p).unwrap().providers[0].credential_configured);
        e.revision = revision(&p).unwrap();
        e.deleted = true;
        save(&p, &e).unwrap();
        assert!(load(&p).unwrap().providers.is_empty());
    }
    #[test]
    fn duplicate_ids_and_command_credentials_are_invalid() {
        let p = path();
        let mut e = edit(&p);
        e.provider.models.push(e.provider.models[0].clone());
        assert!(save(&p, &e).is_err());
        e.provider.models.pop();
        e.credential_action = "replace".into();
        e.credential = Some("!printenv".into());
        assert!(save(&p, &e).is_err());
        assert!(!p.exists());
    }
}
