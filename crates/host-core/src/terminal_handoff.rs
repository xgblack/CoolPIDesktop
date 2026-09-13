use crate::HostError;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, ffi::OsString, path::PathBuf};

const MAX_FRAME: usize = 1024 * 1024;

#[derive(Debug, PartialEq)]
struct Invocation {
    data: PathBuf,
    task_id: String,
    executable: String,
    arguments: Vec<String>,
    cwd: PathBuf,
}

fn invalid() -> HostError {
    HostError::new(
        "invalid_command",
        "Use the complete terminal command copied from the task's Runtime tab.",
    )
}

fn parse(args: Vec<OsString>) -> Option<Result<Invocation, HostError>> {
    if args.get(1).is_none_or(|arg| arg != "session") {
        return None;
    }
    Some((|| {
        let args: Vec<String> = args
            .into_iter()
            .map(|arg| arg.into_string().map_err(|_| invalid()))
            .collect::<Result<_, _>>()?;
        if args.get(2).map(String::as_str) != Some("open") {
            return Err(invalid());
        }
        let mut data = None;
        let mut task = None;
        let mut i = 3;
        while args.get(i).map(String::as_str) != Some("--") {
            let key = args.get(i).ok_or_else(invalid)?;
            let value = args
                .get(i + 1)
                .filter(|s| !s.is_empty())
                .ok_or_else(invalid)?;
            match key.as_str() {
                "--data" if data.is_none() => data = Some(PathBuf::from(value)),
                "--task" if task.is_none() => task = Some(value.clone()),
                _ => return Err(invalid()),
            }
            i += 2;
        }
        let executable = args
            .get(i + 1)
            .filter(|s| !s.is_empty())
            .ok_or_else(invalid)?
            .clone();
        let arguments = args.get(i + 2..).ok_or_else(invalid)?.to_vec();
        let mut cwd = None;
        for (index, arg) in arguments.iter().enumerate() {
            if arg == "--cwd" {
                if cwd.is_some() {
                    return Err(invalid());
                }
                cwd = Some(PathBuf::from(arguments.get(index + 1).ok_or_else(invalid)?));
            }
        }
        let invocation = Invocation {
            data: data.ok_or_else(invalid)?,
            task_id: task.ok_or_else(invalid)?,
            executable,
            arguments,
            cwd: cwd.ok_or_else(invalid)?,
        };
        if !invocation.data.is_absolute()
            || !invocation.cwd.is_absolute()
            || !PathBuf::from(&invocation.executable).is_absolute()
        {
            return Err(invalid());
        }
        Ok(invocation)
    })())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Request<'a> {
    task_id: &'a str,
    executable: &'a str,
    arguments: &'a [String],
    pid: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Response {
    result: Option<LaunchEnvironment>,
    error: Option<HostError>,
}

#[derive(Deserialize)]
struct LaunchEnvironment {
    environment: BTreeMap<String, String>,
}

#[cfg(unix)]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ServerRequest {
    task_id: String,
    executable: String,
    arguments: Vec<String>,
    pid: u32,
}

#[cfg(unix)]
async fn read_request(stream: &mut tokio::net::UnixStream) -> Result<ServerRequest, HostError> {
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
    let peer = stream.peer_cred().map_err(|_| protocol_error())?;
    if peer.uid() != unsafe { libc::geteuid() } {
        return Err(HostError::new(
            "handoff_denied",
            "Terminal handoff requires the same system user.",
        ));
    }
    let mut frame = Vec::new();
    BufReader::new(stream)
        .take((MAX_FRAME + 1) as u64)
        .read_until(b'\n', &mut frame)
        .await
        .map_err(|_| protocol_error())?;
    if frame.len() > MAX_FRAME || frame.last() != Some(&b'\n') {
        return Err(protocol_error());
    }
    let request: ServerRequest = serde_json::from_slice(&frame).map_err(|_| protocol_error())?;
    if request.pid == 0 || peer.pid().is_some_and(|pid| pid as u32 != request.pid) {
        return Err(protocol_error());
    }
    Ok(request)
}

#[cfg(unix)]
async fn serve_connection(workbench: crate::Workbench, mut stream: tokio::net::UnixStream) {
    use tokio::{
        io::AsyncWriteExt,
        time::{Duration, timeout},
    };
    let result = timeout(Duration::from_secs(25), async {
        let request = read_request(&mut stream).await?;
        workbench
            .handoff(&request.task_id, &request.executable, &request.arguments)
            .await
    })
    .await
    .unwrap_or_else(|_| {
        Err(HostError::new(
            "handoff_timeout",
            "Terminal handoff timed out; check the task before retrying.",
        ))
    });
    let value = match result {
        Ok(environment) => serde_json::json!({"result":{"environment":environment}}),
        Err(error) => serde_json::json!({"error":error}),
    };
    let Ok(mut response) = serde_json::to_vec(&value) else {
        return;
    };
    if response.len() >= MAX_FRAME {
        response = serde_json::to_vec(&serde_json::json!({"error":protocol_error()}))
            .expect("HostError is serializable");
    }
    response.push(b'\n');
    if !matches!(
        timeout(Duration::from_secs(5), stream.write_all(&response)).await,
        Ok(Ok(()))
    ) {
        eprintln!("Terminal handoff client disconnected before receiving the result.");
    }
}

#[cfg(unix)]
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Endpoint {
    version: u32,
    socket: PathBuf,
}

fn unavailable() -> HostError {
    HostError::new(
        "desktop_unavailable",
        "Open Cool PI Desktop before running the terminal handoff command.",
    )
}

#[cfg(unix)]
fn read_endpoint(data: &std::path::Path) -> Result<Endpoint, HostError> {
    use std::{
        io::Read,
        os::unix::fs::{MetadataExt, OpenOptionsExt},
    };
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(data.join("runtime-endpoint.json"))
        .map_err(|_| unavailable())?;
    let metadata = file.metadata().map_err(|_| protocol_error())?;
    if !metadata.is_file()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
    {
        return Err(protocol_error());
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(4097)
        .read_to_end(&mut bytes)
        .map_err(|_| protocol_error())?;
    if bytes.len() > 4096 {
        return Err(protocol_error());
    }
    let endpoint: Endpoint = serde_json::from_slice(&bytes).map_err(|_| protocol_error())?;
    if endpoint.version != 1 || !endpoint.socket.is_absolute() {
        return Err(protocol_error());
    }
    Ok(endpoint)
}

#[cfg(unix)]
fn bind_socket(
    data: &std::path::Path,
) -> Result<
    (
        std::os::unix::net::UnixListener,
        crate::writer_lock::WriterLease,
    ),
    HostError,
> {
    use std::{
        io::Write,
        os::unix::{
            fs::{OpenOptionsExt, PermissionsExt},
            net::UnixListener,
        },
    };
    let lease = crate::writer_lock::WriterLease::acquire(data, "handoff-server", "desktop-server")?;
    let discovery = data.join("runtime-endpoint.json");
    match std::fs::symlink_metadata(&discovery) {
        Ok(_) => {
            read_endpoint(data).map_err(|_| {
                HostError::new(
                    "handoff_socket_conflict",
                    "The runtime discovery path contains an unrecognized file; it was preserved.",
                )
            })?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => {
            return Err(HostError::new(
                "handoff_socket_conflict",
                "Could not inspect the runtime discovery file.",
            ));
        }
    }
    // Unix sockets cannot be moved to Trash on macOS. Keep stale socket nodes in /tmp,
    // where the OS manages cleanup, and atomically replace only our discovery document.
    let path = PathBuf::from("/tmp").join(format!(
        "cool-pi-{}-{}.sock",
        unsafe { libc::geteuid() },
        uuid::Uuid::new_v4().simple()
    ));
    let listener = UnixListener::bind(&path).map_err(|_| {
        HostError::new(
            "handoff_bind",
            "Could not create the terminal handoff socket.",
        )
    })?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).map_err(|_| {
        HostError::new(
            "handoff_bind",
            "Could not restrict terminal handoff socket permissions.",
        )
    })?;
    listener.set_nonblocking(true).map_err(|_| {
        HostError::new(
            "handoff_bind",
            "Could not configure the terminal handoff listener.",
        )
    })?;
    let temporary = data.join(format!(
        ".runtime-endpoint-{}.json",
        uuid::Uuid::new_v4().simple()
    ));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)
        .map_err(|_| protocol_error())?;
    let bytes = serde_json::to_vec(&Endpoint {
        version: 1,
        socket: path,
    })
    .map_err(|_| protocol_error())?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| protocol_error())?;
    std::fs::rename(&temporary, discovery).map_err(|_| protocol_error())?;
    Ok((listener, lease))
}

#[cfg(unix)]
impl crate::Workbench {
    pub async fn start_handoff_server(&self) -> Result<(), HostError> {
        let (listener, lease) = bind_socket(&self.data)?;
        let listener = tokio::net::UnixListener::from_std(listener).map_err(|_| {
            HostError::new(
                "handoff_bind",
                "Could not start the terminal handoff listener.",
            )
        })?;
        let workbench = self.clone();
        tokio::spawn(async move {
            let _lease = lease;
            let permits = std::sync::Arc::new(tokio::sync::Semaphore::new(16));
            loop {
                let stream = match listener.accept().await {
                    Ok((stream, _)) => stream,
                    Err(_) => {
                        eprintln!("Terminal handoff listener failed.");
                        break;
                    }
                };
                let Ok(permit) = permits.clone().try_acquire_owned() else {
                    drop(stream);
                    continue;
                };
                let workbench = workbench.clone();
                tokio::spawn(async move {
                    let _permit = permit;
                    serve_connection(workbench, stream).await;
                });
            }
        });
        Ok(())
    }
}

fn protocol_error() -> HostError {
    HostError::new(
        "handoff_protocol",
        "The desktop returned an invalid terminal handoff response.",
    )
}

#[cfg(unix)]
fn exchange(
    stream: &mut std::os::unix::net::UnixStream,
    invocation: &Invocation,
) -> Result<BTreeMap<String, String>, HostError> {
    use std::io::{BufRead, BufReader, Read, Write};
    let request = Request {
        task_id: &invocation.task_id,
        executable: &invocation.executable,
        arguments: &invocation.arguments,
        pid: std::process::id(),
    };
    let mut payload = serde_json::to_vec(&request).map_err(|_| invalid())?;
    payload.push(b'\n');
    if payload.len() > MAX_FRAME {
        return Err(invalid());
    }
    stream.write_all(&payload).map_err(|_| {
        HostError::new(
            "handoff_connection",
            "Could not send the handoff request to the desktop.",
        )
    })?;
    let mut response = Vec::new();
    BufReader::new(stream)
        .take((MAX_FRAME + 1) as u64)
        .read_until(b'\n', &mut response)
        .map_err(|_| {
            HostError::new(
                "handoff_connection",
                "The desktop did not complete the handoff. Check the task and try again.",
            )
        })?;
    if response.len() > MAX_FRAME || response.last() != Some(&b'\n') {
        return Err(protocol_error());
    }
    let response: Response = serde_json::from_slice(&response).map_err(|_| protocol_error())?;
    match (response.result, response.error) {
        (Some(result), None) => {
            if result.environment.iter().any(|(key, value)| {
                key.is_empty() || key.contains(['=', '\0']) || value.contains('\0')
            }) {
                return Err(protocol_error());
            }
            Ok(result.environment)
        }
        (None, Some(error)) => Err(error),
        _ => Err(protocol_error()),
    }
}

#[cfg(unix)]
fn run(invocation: Invocation) -> Result<(), HostError> {
    use std::{
        io::IsTerminal,
        os::unix::{net::UnixStream, process::CommandExt},
        process::Command,
        time::Duration,
    };
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Err(HostError::new(
            "terminal_required",
            "Run this command in an interactive terminal.",
        ));
    }
    if !invocation.data.is_dir() || !invocation.cwd.is_dir() {
        return Err(HostError::new(
            "directory_missing",
            "The application data or task working directory is missing.",
        ));
    }
    let endpoint = read_endpoint(&invocation.data)?;
    {
        use std::os::unix::fs::{FileTypeExt, MetadataExt};
        let metadata = std::fs::symlink_metadata(&endpoint.socket).map_err(|_| unavailable())?;
        if !metadata.file_type().is_socket()
            || metadata.uid() != unsafe { libc::geteuid() }
            || metadata.mode() & 0o077 != 0
        {
            return Err(protocol_error());
        }
    }
    let mut stream = UnixStream::connect(endpoint.socket).map_err(|_| unavailable())?;
    let timeout = Some(Duration::from_secs(30));
    stream
        .set_read_timeout(timeout)
        .and_then(|_| stream.set_write_timeout(timeout))
        .map_err(|_| {
            HostError::new(
                "handoff_connection",
                "Could not configure the desktop connection.",
            )
        })?;
    let mut environment = exchange(&mut stream, &invocation)?;
    // Execution configuration comes from the Host; terminal capabilities come from this TTY.
    for key in ["TERM", "COLORTERM", "TERM_PROGRAM", "TERM_PROGRAM_VERSION"] {
        if let Ok(value) = std::env::var(key) {
            environment.insert(key.into(), value);
        }
    }
    let mut lease = crate::writer_lock::WriterLease::acquire(
        &invocation.data,
        &invocation.task_id,
        "terminal",
    )?;
    lease.set_pid(std::process::id())?;
    lease.make_inheritable()?;
    let _error = Command::new(&invocation.executable)
        .args(&invocation.arguments)
        .current_dir(&invocation.cwd)
        .env_clear()
        .envs(environment)
        .exec();
    Err(HostError::new(
        "terminal_launch_failed",
        "Could not execute the configured OMP. Check its installation and executable permissions.",
    ))
}

#[cfg(not(unix))]
fn run(_invocation: Invocation) -> Result<(), HostError> {
    Err(HostError::new(
        "unsupported_platform",
        "Terminal handoff is only supported on Unix platforms.",
    ))
}

/// Handles terminal subcommands before Tauri initializes a graphical application.
pub fn run_if_requested() -> Option<i32> {
    parse(std::env::args_os().collect()).map(|invocation| match invocation.and_then(run) {
        Ok(()) => 0,
        Err(error) => {
            let message: String = error
                .message
                .chars()
                .filter(|c| !c.is_control())
                .take(2048)
                .collect();
            eprintln!("{message}");
            1
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[cfg(unix)]
    async fn actual_server_routes_to_workbench_and_returns_safe_error() {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        let root = PathBuf::from("/tmp").join(format!("handoff-{}", uuid::Uuid::new_v4().simple()));
        let workbench = crate::Workbench::open(root.clone()).await.unwrap();
        workbench.start_handoff_server().await.unwrap();
        let endpoint = read_endpoint(&root).unwrap();
        let mut stream = tokio::net::UnixStream::connect(endpoint.socket)
            .await
            .unwrap();
        let request = serde_json::json!({"taskId":"missing", "executable":"/opt/omp", "arguments":["--cwd", "/tmp"], "pid":std::process::id()});
        stream
            .write_all(format!("{request}\n").as_bytes())
            .await
            .unwrap();
        let mut frame = String::new();
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            BufReader::new(stream).read_line(&mut frame),
        )
        .await
        .unwrap()
        .unwrap();
        let response: Response = serde_json::from_str(&frame).unwrap();
        assert_eq!(response.error.unwrap().code, "task_missing");
        assert!(response.result.is_none());
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn server_accepts_same_user_and_checks_request_identity() {
        use tokio::io::AsyncWriteExt;
        let (mut client, mut server) = tokio::net::UnixStream::pair().unwrap();
        let request = serde_json::json!({"taskId":"task", "executable":"/opt/omp", "arguments":["--cwd", "/tmp"], "pid":std::process::id()});
        client
            .write_all(format!("{request}\n").as_bytes())
            .await
            .unwrap();
        assert_eq!(read_request(&mut server).await.unwrap().task_id, "task");
        let (mut client, mut server) = tokio::net::UnixStream::pair().unwrap();
        client
            .write_all(
                b"{\"taskId\":\"task\",\"executable\":\"/opt/omp\",\"arguments\":[],\"pid\":0}\n",
            )
            .await
            .unwrap();
        assert!(read_request(&mut server).await.is_err());
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn server_rejects_truncated_and_oversized_frames() {
        use tokio::io::AsyncWriteExt;
        for frame in [b"{}".to_vec(), vec![b'x'; MAX_FRAME + 1]] {
            let (mut client, mut server) = tokio::net::UnixStream::pair().unwrap();
            let writer = tokio::spawn(async move {
                let _ = client.write_all(&frame).await;
            });
            assert!(read_request(&mut server).await.is_err());
            drop(server);
            writer.await.unwrap();
        }
    }

    #[test]
    #[cfg(unix)]
    fn server_socket_is_private_and_never_replaces_unknown_files() {
        use std::os::unix::fs::PermissionsExt;
        let root = PathBuf::from("/tmp").join(format!("handoff-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).unwrap();
        let marker = root.join("runtime-endpoint.json");
        std::fs::write(&marker, b"keep me").unwrap();
        assert!(bind_socket(&root).is_err());
        assert_eq!(std::fs::read(marker).unwrap(), b"keep me");
        let root = PathBuf::from("/tmp").join(format!("handoff-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).unwrap();
        let (_listener, _lease) = bind_socket(&root).unwrap();
        assert_eq!(
            std::fs::metadata(read_endpoint(&root).unwrap().socket)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert!(bind_socket(&root).is_err());
    }

    #[test]
    #[cfg(unix)]
    fn server_restart_replaces_discovery_without_deleting_old_socket() {
        let root = PathBuf::from("/tmp").join(format!("handoff-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).unwrap();
        let first = bind_socket(&root).unwrap();
        let old_socket = read_endpoint(&root).unwrap().socket;
        drop(first);
        let (_listener, _lease) = bind_socket(&root).unwrap();
        let new_socket = read_endpoint(&root).unwrap().socket;
        assert_ne!(old_socket, new_socket);
        assert!(old_socket.exists());
        assert!(std::os::unix::net::UnixStream::connect(new_socket).is_ok());
    }

    fn command() -> Vec<OsString> {
        [
            "desktop",
            "session",
            "open",
            "--data",
            "/tmp/app data",
            "--task",
            "task-1",
            "--",
            "/opt/omp",
            "--cwd",
            "/tmp/project",
            "--add-dir",
            "/tmp/extra dir",
            "--approval-mode",
            "write",
            "--resume",
            "/tmp/session.jsonl",
        ]
        .into_iter()
        .map(Into::into)
        .collect()
    }

    #[test]
    fn ignores_desktop_arguments() {
        assert!(parse(vec!["desktop".into()]).is_none());
        assert!(parse(vec!["desktop".into(), "--verbose".into()]).is_none());
    }

    #[test]
    fn preserves_complete_argv_without_shell_interpretation() {
        let mut args = command();
        args.extend(
            ["--system-prompt", "$(touch /tmp/no); 'quoted'"]
                .into_iter()
                .map(OsString::from),
        );
        let parsed = parse(args).unwrap().unwrap();
        assert_eq!(parsed.data, PathBuf::from("/tmp/app data"));
        assert_eq!(parsed.arguments[3], "/tmp/extra dir");
        assert_eq!(
            parsed.arguments.last().unwrap(),
            "$(touch /tmp/no); 'quoted'"
        );
    }

    #[test]
    fn rejects_incomplete_duplicate_and_relative_arguments() {
        for length in 2..11 {
            assert!(parse(command()[..length].to_vec()).unwrap().is_err());
        }
        let mut args = command();
        args[4] = "relative".into();
        assert!(parse(args).unwrap().is_err());
        let mut args = command();
        args.extend(["--cwd", "/tmp/other"].into_iter().map(OsString::from));
        assert!(parse(args).unwrap().is_err());
    }

    #[cfg(unix)]
    fn reply(payload: Vec<u8>) -> Result<BTreeMap<String, String>, HostError> {
        use std::{
            io::{BufRead, BufReader, Write},
            os::unix::net::UnixStream,
        };
        let (mut client, mut server) = UnixStream::pair().unwrap();
        let worker = std::thread::spawn(move || {
            let mut request = String::new();
            BufReader::new(&mut server).read_line(&mut request).unwrap();
            let request: serde_json::Value = serde_json::from_str(&request).unwrap();
            assert_eq!(request["taskId"], "task-1");
            assert_eq!(request["pid"], std::process::id());
            let _ = server.write_all(&payload);
        });
        let result = exchange(&mut client, &parse(command()).unwrap().unwrap());
        drop(client);
        worker.join().unwrap();
        result
    }

    #[test]
    #[cfg(unix)]
    fn receives_environment_and_propagates_busy_error() {
        let env = reply(b"{\"result\":{\"environment\":{\"PATH\":\"/bin\"}}}\n".to_vec()).unwrap();
        assert_eq!(env["PATH"], "/bin");
        let error = reply(b"{\"error\":{\"code\":\"task_busy\",\"message\":\"Task busy\",\"suggestion\":\"Wait\"}}\n".to_vec()).unwrap_err();
        assert_eq!(error.code, "task_busy");
    }

    #[test]
    #[cfg(unix)]
    fn rejects_malformed_truncated_oversized_and_invalid_environment() {
        for payload in [
            b"{}\n".to_vec(),
            b"not json\n".to_vec(),
            b"{\"result\":{\"environment\":{}}}".to_vec(),
            b"{\"result\":{\"environment\":{\"BAD=KEY\":\"value\"}}}\n".to_vec(),
            vec![b'x'; MAX_FRAME + 1],
        ] {
            assert_eq!(reply(payload).unwrap_err().code, "handoff_protocol");
        }
    }
}
