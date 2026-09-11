use host_core::Workbench;
use std::{path::PathBuf, process::Command};
#[tokio::main]
async fn main() {
    let root = PathBuf::from(std::env::args().nth(1).expect("fixture root"));
    std::fs::create_dir_all(&root).unwrap();
    let git = |args: &[&str]| {
        assert!(
            Command::new("git")
                .current_dir(&root)
                .args(args)
                .status()
                .unwrap()
                .success()
        );
    };
    git(&["init"]);
    git(&["config", "user.name", "Native Acceptance"]);
    git(&["config", "user.email", "native@example.invalid"]);
    std::fs::write(root.join("accept.txt"), "base\n").unwrap();
    git(&["add", "accept.txt"]);
    git(&["commit", "-m", "fixture"]);
    let data = PathBuf::from(std::env::var("HOME").unwrap())
        .join("Library/Application Support/com.coolstudio.cool-pi-desktop");
    let w = Workbench::open(data).await.unwrap();
    let p = w
        .store
        .register_project("P2 Native Acceptance", vec![root], true)
        .await
        .unwrap();
    for name in ["Native A", "Native B"] {
        let t = w.create_task(&p.id, name, "isolated").await.unwrap();
        println!("{}", serde_json::to_string(&t).unwrap());
        println!(
            "{}",
            serde_json::to_string(&w.task_roots(&t.id).await.unwrap()).unwrap()
        );
    }
}
