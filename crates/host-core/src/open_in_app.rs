//! macOS open-in-app catalog adapted from DSH (MIT); see docs/third-party/DSH-LICENSE.
//! Only catalog IDs cross IPC. Launch paths come from validated task execution roots.
use crate::{HostError, Workbench};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{process::Command, sync::Mutex};
type Result<T> = std::result::Result<T, HostError>;

#[derive(Clone, Serialize)]
pub struct OpenApp {
    pub id: String,
    pub name: String,
}
#[derive(Clone)]
struct Resolved {
    app: OpenApp,
    bundle: PathBuf,
}
// Menu order and bundle aliases match DSH's macOS catalog.
const CATALOG: &[(&str, &str, &[&str])] = &[
    (
        "finder",
        "访达",
        &["/System/Library/CoreServices/Finder.app"],
    ),
    ("cursor", "Cursor", &["Cursor.app"]),
    ("vscode", "VS Code", &["Visual Studio Code.app"]),
    (
        "vscodeinsiders",
        "VS Code Insiders",
        &["Visual Studio Code - Insiders.app"],
    ),
    ("windsurf", "Windsurf", &["Windsurf.app"]),
    ("zed", "Zed", &["Zed.app", "Zed Preview.app"]),
    ("sublimetext", "Sublime Text", &["Sublime Text.app"]),
    ("xcode", "Xcode", &[]),
    ("androidstudio", "Android Studio", &["Android Studio.app"]),
    (
        "intellij",
        "IntelliJ IDEA",
        &[
            "IntelliJ IDEA.app",
            "IntelliJ IDEA Ultimate.app",
            "IntelliJ IDEA CE.app",
        ],
    ),
    (
        "pycharm",
        "PyCharm",
        &[
            "PyCharm.app",
            "PyCharm Professional.app",
            "PyCharm CE.app",
            "PyCharm Community.app",
        ],
    ),
    ("webstorm", "WebStorm", &["WebStorm.app"]),
    ("phpstorm", "PhpStorm", &["PhpStorm.app"]),
    ("goland", "GoLand", &["GoLand.app"]),
    ("rider", "Rider", &["Rider.app", "JetBrains Rider.app"]),
    ("rustrover", "RustRover", &["RustRover.app"]),
    ("fork", "Fork", &["Fork.app"]),
    ("sourcetree", "Sourcetree", &["Sourcetree.app"]),
    ("github", "GitHub Desktop", &["GitHub Desktop.app"]),
    ("tower", "Tower", &["Tower.app"]),
    ("gitkraken", "GitKraken", &["GitKraken.app"]),
    ("smartgit", "SmartGit", &["SmartGit.app"]),
    ("sublimemerge", "Sublime Merge", &["Sublime Merge.app"]),
    ("ghostty", "Ghostty", &["Ghostty.app"]),
    ("warp", "Warp", &["Warp.app"]),
    ("iterm", "iTerm2", &["iTerm.app"]),
    ("kitty", "kitty", &["kitty.app"]),
    (
        "terminal",
        "终端",
        &["/System/Applications/Utilities/Terminal.app"],
    ),
];
static APPS: Mutex<Option<Vec<Resolved>>> = Mutex::const_new(None);

fn native_command(program: &str) -> Command {
    let mut command = Command::new(program);
    command
        .env_clear()
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");
    for key in [
        "HOME",
        "USER",
        "LOGNAME",
        "TMPDIR",
        "LANG",
        "__CF_USER_TEXT_ENCODING",
    ] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    command.kill_on_drop(true);
    command
}
async fn output(command: &mut Command) -> Result<Vec<u8>> {
    let result = tokio::time::timeout(Duration::from_secs(5), command.output())
        .await
        .map_err(|_| HostError::new("open_app_timeout", "系统打开请求超时，请重试"))?
        .map_err(|_| HostError::new("open_app_failed", "无法执行系统打开请求"))?;
    if !result.status.success() {
        return Err(HostError::new(
            "open_app_failed",
            "应用未接受打开请求，请检查安装状态后重试",
        ));
    }
    Ok(result.stdout)
}
fn find_bundle(roots: &[PathBuf], names: &[&str]) -> Option<PathBuf> {
    roots
        .iter()
        .flat_map(|root| names.iter().map(move |name| root.join(name)))
        .find(|path| path.is_dir())
}
async fn detect() -> Vec<Resolved> {
    if !cfg!(target_os = "macos") {
        return vec![];
    }
    let mut roots = vec![PathBuf::from("/Applications")];
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(home).join("Applications"));
    }
    let developer = output(native_command("/usr/bin/xcode-select").arg("-p"))
        .await
        .ok();
    let xcode = developer
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .and_then(|path| {
            let bundle = Path::new(path.trim()).parent()?.parent()?;
            (bundle.extension().is_some_and(|e| e == "app") && bundle.is_dir())
                .then(|| bundle.to_owned())
        });
    CATALOG
        .iter()
        .filter_map(|(id, name, names)| {
            let bundle = if *id == "xcode" {
                xcode.clone()
            } else {
                find_bundle(&roots, names)
            }?;
            Some(Resolved {
                app: OpenApp {
                    id: id.to_string(),
                    name: name.to_string(),
                },
                bundle,
            })
        })
        .collect()
}
async fn resolved(id: &str) -> Result<Resolved> {
    if !CATALOG.iter().any(|entry| entry.0 == id) {
        return Err(HostError::new("unknown_app", "不支持此打开方式"));
    }
    let mut cache = APPS.lock().await;
    if cache.is_none() {
        *cache = Some(detect().await);
    }
    if !cache
        .as_ref()
        .unwrap()
        .iter()
        .any(|entry| entry.app.id == id && entry.bundle.is_dir())
    {
        *cache = Some(detect().await);
    }
    cache
        .as_ref()
        .unwrap()
        .iter()
        .find(|entry| entry.app.id == id)
        .cloned()
        .ok_or_else(|| HostError::new("app_unavailable", "应用未安装或已移除，请刷新打开方式"))
}
pub async fn list(refresh: bool) -> Vec<OpenApp> {
    let mut cache = APPS.lock().await;
    if refresh || cache.is_none() {
        *cache = Some(detect().await);
    }
    cache
        .as_ref()
        .unwrap()
        .iter()
        .map(|entry| entry.app.clone())
        .collect()
}

/// Icon files are replaceable application-cache artifacts, never frontend-selected paths.
pub async fn icon(id: &str, cache: &Path) -> Result<String> {
    let app = resolved(id).await?;
    let resources = app.bundle.join("Contents/Resources");
    let plist = app.bundle.join("Contents/Info.plist");
    let declared = output(
        native_command("/usr/bin/plutil")
            .args(["-extract", "CFBundleIconFile", "raw", "-o", "-"])
            .arg(plist),
    )
    .await
    .ok()
    .and_then(|bytes| String::from_utf8(bytes).ok());
    let mut source = declared.and_then(|name| {
        let name = name.trim();
        // A plist may name only a resource filename, never an arbitrary path.
        if name.is_empty() || Path::new(name).components().count() != 1 || name.contains('/') {
            return None;
        }
        let path = resources.join(if name.ends_with(".icns") {
            name.to_owned()
        } else {
            format!("{name}.icns")
        });
        path.is_file().then_some(path)
    });
    if source.is_none() {
        if let Ok(entries) = std::fs::read_dir(&resources) {
            source = entries
                .filter_map(|entry| entry.ok().map(|e| e.path()))
                .find(|path| path.extension().is_some_and(|e| e == "icns") && path.is_file());
        }
    }
    let source = source.ok_or_else(|| {
        HostError::new("app_icon_missing", "应用未提供可读取的图标，使用通用图标")
    })?;
    let cache = cache.join("open-in-app");
    tokio::fs::create_dir_all(&cache)
        .await
        .map_err(|_| HostError::new("app_icon_cache", "无法创建应用图标缓存"))?;
    let target = cache.join(format!("{id}.png"));
    output(
        native_command("/usr/bin/sips")
            .args(["-s", "format", "png", "-Z", "128"])
            .arg(source)
            .arg("--out")
            .arg(&target),
    )
    .await?;
    let bytes = tokio::fs::read(target)
        .await
        .map_err(|_| HostError::new("app_icon_failed", "无法读取应用图标，使用通用图标"))?;
    Ok(format!("data:image/png;base64,{}", STANDARD.encode(bytes)))
}
fn open_command(app: &Resolved, directory: &Path) -> Command {
    let mut command = native_command("/usr/bin/open");
    // Explicitly address Finder so a different OS directory default does not change this action.
    command.arg("-a").arg(&app.bundle).arg(directory);
    command
}
impl Workbench {
    pub async fn open_in_app(&self, task_id: &str, app_id: &str) -> Result<()> {
        let _guard = self.gate.lock().await;
        let roots = self.store.validate_task_roots(task_id).await?;
        let directory = roots
            .first()
            .ok_or_else(|| HostError::new("invalid_workspace", "任务没有工作目录"))?;
        let app = resolved(app_id).await?;
        if app_id == "xcode" {
            // xed understands source directories; open -a is DSH's fallback for failed xed.
            if output(native_command("/usr/bin/xed").arg(directory))
                .await
                .is_ok()
            {
                return Ok(());
            }
        }
        output(&mut open_command(&app, directory)).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mac_catalog_resolution_and_argument_boundaries() {
        let root = std::env::temp_dir().join(format!("open-app-test-{}", uuid::Uuid::new_v4()));
        let system = root.join("system");
        let user = root.join("user");
        std::fs::create_dir_all(user.join("Zed Preview.app")).unwrap();
        assert_eq!(
            find_bundle(
                &[system.clone(), user.clone()],
                &["Zed.app", "Zed Preview.app"]
            ),
            Some(user.join("Zed Preview.app"))
        );
        assert_eq!(find_bundle(&[system, user.clone()], &["Warp.app"]), None);
        let app = Resolved {
            app: OpenApp {
                id: "zed".into(),
                name: "Zed".into(),
            },
            bundle: user.join("Zed Preview.app"),
        };
        let directory = Path::new("/tmp/中文 workspace/$(touch bad); 'quoted'");
        let command = open_command(&app, directory);
        let args: Vec<_> = command.as_std().get_args().collect();
        assert_eq!(
            args,
            vec![
                std::ffi::OsStr::new("-a"),
                app.bundle.as_os_str(),
                directory.as_os_str()
            ]
        );
        // Entire fixture was created above; move it to the OS trash rather than deleting it.
        trash::delete(root).unwrap();
    }
    #[tokio::test]
    async fn rejects_arbitrary_program_ids() {
        assert_eq!(resolved("/bin/sh").await.err().unwrap().code, "unknown_app");
    }
    #[tokio::test]
    async fn reports_native_launch_failure() {
        assert_eq!(
            output(&mut native_command("/usr/bin/false"))
                .await
                .unwrap_err()
                .code,
            "open_app_failed"
        );
    }
    #[tokio::test]
    #[ignore = "opens Finder and installed Warp on a real isolated task"]
    async fn live_task_launch_and_rejection() {
        let root = std::env::temp_dir().join(format!("open-app-live-{}", uuid::Uuid::new_v4()));
        let repo = root.join("中文 workspace");
        std::fs::create_dir_all(&repo).unwrap();
        for args in [
            vec!["init"],
            vec![
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "--allow-empty",
                "-m",
                "base",
            ],
        ] {
            assert!(
                std::process::Command::new("git")
                    .current_dir(&repo)
                    .args(args)
                    .output()
                    .unwrap()
                    .status
                    .success()
            );
        }
        let w = Workbench::open(root.join("data")).await.unwrap();
        let project = w
            .store
            .register_project("Open app fixture", vec![repo.clone()], true)
            .await
            .unwrap();
        let task = w
            .create_task(&project.id, "isolated", "isolated")
            .await
            .unwrap();
        let execution = w.store.validate_task_roots(&task.id).await.unwrap();
        assert_ne!(execution[0], repo.canonicalize().unwrap());
        let resolved_app = resolved("finder").await.unwrap();
        let command = open_command(&resolved_app, &execution[0]);
        assert_eq!(
            command.as_std().get_args().last().unwrap(),
            execution[0].as_os_str()
        );
        w.open_in_app(&task.id, "finder").await.unwrap();
        println!("Finder accepted isolated task directory");
        if list(false).await.iter().any(|app| app.id == "warp") {
            w.open_in_app(&task.id, "warp").await.unwrap();
            println!("Warp accepted isolated task directory");
        }
        w.store
            .update_task(&task.id, "isolated", false, true)
            .await
            .unwrap();
        assert_eq!(
            w.open_in_app(&task.id, "finder").await.unwrap_err().code,
            "task_archived"
        );
        assert!(w.open_in_app("missing-task", "finder").await.is_err());
        drop(w);
        // The repo, data and worktree are all owned by this fixture.
        trash::delete(root).unwrap();
    }
    #[tokio::test]
    #[ignore = "live installed macOS catalog and icon extraction"]
    async fn installed_mac_apps() {
        let apps = list(true).await;
        assert!(apps.iter().any(|app| app.id == "finder"));
        assert!(apps.iter().any(|app| app.id == "terminal"));
        let cache = std::env::temp_dir().join(format!("open-app-icons-{}", uuid::Uuid::new_v4()));
        for app in apps {
            let result = icon(&app.id, &cache).await;
            println!(
                "{}: {}",
                app.name,
                if result.is_ok() {
                    "PNG icon"
                } else {
                    "generic icon"
                }
            );
            if app.id == "finder" {
                assert!(result.unwrap().starts_with("data:image/png;base64,iVBOR"));
            }
        }
        trash::delete(cache).unwrap();
    }
}
