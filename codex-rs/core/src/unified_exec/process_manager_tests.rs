use super::*;
use crate::sandboxing::ExecRequest;
use crate::sandboxing::ExecServerEnvConfig;
use crate::unified_exec::DEFAULT_MAX_BACKGROUND_TERMINAL_TIMEOUT_MS;
use crate::unified_exec::HeadTailBuffer;
use crate::unified_exec::NoopSpawnLifecycle;
use crate::unified_exec::process_manager::apply_unified_exec_env;
use crate::unified_exec::process_manager::env_overlay_for_exec_server;
use crate::unified_exec::process_manager::exec_server_params_for_request;
use crate::unified_exec::process_manager::exec_server_process_id;
use codex_sandboxing::SandboxType;
use codex_utils_pty::TerminalSize;
use codex_utils_pty::spawn_pty_process;
use pretty_assertions::assert_eq;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Weak;
use std::sync::atomic::AtomicBool;
use tokio::sync::Mutex;
use tokio::time::Duration;
use tokio::time::Instant;

#[test]
fn unified_exec_env_injects_defaults() {
    let env = apply_unified_exec_env(HashMap::new());
    let expected = HashMap::from([
        ("NO_COLOR".to_string(), "1".to_string()),
        ("TERM".to_string(), "dumb".to_string()),
        ("LANG".to_string(), "C.UTF-8".to_string()),
        ("LC_CTYPE".to_string(), "C.UTF-8".to_string()),
        ("LC_ALL".to_string(), "C.UTF-8".to_string()),
        ("COLORTERM".to_string(), String::new()),
        ("PAGER".to_string(), "cat".to_string()),
        ("GIT_PAGER".to_string(), "cat".to_string()),
        ("GH_PAGER".to_string(), "cat".to_string()),
        ("CODEX_CI".to_string(), "1".to_string()),
    ]);

    assert_eq!(env, expected);
}

#[test]
fn unified_exec_env_overrides_existing_values() {
    let mut base = HashMap::new();
    base.insert("NO_COLOR".to_string(), "0".to_string());
    base.insert("PATH".to_string(), "/usr/bin".to_string());

    let env = apply_unified_exec_env(base);

    assert_eq!(env.get("NO_COLOR"), Some(&"1".to_string()));
    assert_eq!(env.get("PATH"), Some(&"/usr/bin".to_string()));
}

#[test]
fn env_overlay_for_exec_server_keeps_runtime_changes_only() {
    let local_policy_env = HashMap::from([
        ("HOME".to_string(), "/client-home".to_string()),
        ("PATH".to_string(), "/client-path".to_string()),
        ("SHELL_SET".to_string(), "policy".to_string()),
    ]);
    let request_env = HashMap::from([
        ("HOME".to_string(), "/client-home".to_string()),
        ("PATH".to_string(), "/sandbox-path".to_string()),
        ("SHELL_SET".to_string(), "policy".to_string()),
        ("CODEX_THREAD_ID".to_string(), "thread-1".to_string()),
        (
            "CODEX_SANDBOX_NETWORK_DISABLED".to_string(),
            "1".to_string(),
        ),
    ]);

    assert_eq!(
        env_overlay_for_exec_server(&request_env, &local_policy_env),
        HashMap::from([
            ("PATH".to_string(), "/sandbox-path".to_string()),
            ("CODEX_THREAD_ID".to_string(), "thread-1".to_string()),
            (
                "CODEX_SANDBOX_NETWORK_DISABLED".to_string(),
                "1".to_string()
            ),
        ])
    );
}

#[test]
fn exec_server_params_use_env_policy_overlay_contract() {
    let request = ExecRequest {
        command: vec!["bash".to_string(), "-lc".to_string(), "true".to_string()],
        cwd: std::env::current_dir()
            .expect("current dir")
            .try_into()
            .expect("absolute path"),
        env: HashMap::from([
            ("HOME".to_string(), "/client-home".to_string()),
            ("PATH".to_string(), "/sandbox-path".to_string()),
            ("CODEX_THREAD_ID".to_string(), "thread-1".to_string()),
        ]),
        exec_server_env_config: Some(ExecServerEnvConfig {
            policy: codex_exec_server::ExecEnvPolicy {
                inherit: codex_config::types::ShellEnvironmentPolicyInherit::Core,
                ignore_default_excludes: false,
                exclude: Vec::new(),
                r#set: HashMap::new(),
                include_only: Vec::new(),
            },
            local_policy_env: HashMap::from([
                ("HOME".to_string(), "/client-home".to_string()),
                ("PATH".to_string(), "/client-path".to_string()),
            ]),
        }),
        network: None,
        expiration: crate::exec::ExecExpiration::DefaultTimeout,
        capture_policy: crate::exec::ExecCapturePolicy::ShellTool,
        sandbox: codex_sandboxing::SandboxType::None,
        windows_sandbox_level: codex_protocol::config_types::WindowsSandboxLevel::Disabled,
        windows_sandbox_private_desktop: false,
        sandbox_policy: codex_protocol::protocol::SandboxPolicy::DangerFullAccess,
        file_system_sandbox_policy: codex_protocol::permissions::FileSystemSandboxPolicy::from(
            &codex_protocol::protocol::SandboxPolicy::DangerFullAccess,
        ),
        network_sandbox_policy: codex_protocol::permissions::NetworkSandboxPolicy::Restricted,
        windows_sandbox_filesystem_overrides: None,
        arg0: None,
    };

    let params =
        exec_server_params_for_request(/*process_id*/ 123, &request, /*tty*/ true);

    assert_eq!(params.process_id.as_str(), "123");
    assert!(params.env_policy.is_some());
    assert_eq!(
        params.env,
        HashMap::from([
            ("PATH".to_string(), "/sandbox-path".to_string()),
            ("CODEX_THREAD_ID".to_string(), "thread-1".to_string()),
        ])
    );
}

#[test]
fn exec_server_process_id_matches_unified_exec_process_id() {
    assert_eq!(exec_server_process_id(/*process_id*/ 4321), "4321");
}

#[test]
fn pruning_prefers_exited_processes_outside_recently_used() {
    let now = Instant::now();
    let meta = vec![
        (1, now - Duration::from_secs(40), false),
        (2, now - Duration::from_secs(30), true),
        (3, now - Duration::from_secs(20), false),
        (4, now - Duration::from_secs(19), false),
        (5, now - Duration::from_secs(18), false),
        (6, now - Duration::from_secs(17), false),
        (7, now - Duration::from_secs(16), false),
        (8, now - Duration::from_secs(15), false),
        (9, now - Duration::from_secs(14), false),
        (10, now - Duration::from_secs(13), false),
    ];

    let candidate = UnifiedExecProcessManager::process_id_to_prune_from_meta(&meta);

    assert_eq!(candidate, Some(2));
}

#[test]
fn pruning_falls_back_to_lru_when_no_exited() {
    let now = Instant::now();
    let meta = vec![
        (1, now - Duration::from_secs(40), false),
        (2, now - Duration::from_secs(30), false),
        (3, now - Duration::from_secs(20), false),
        (4, now - Duration::from_secs(19), false),
        (5, now - Duration::from_secs(18), false),
        (6, now - Duration::from_secs(17), false),
        (7, now - Duration::from_secs(16), false),
        (8, now - Duration::from_secs(15), false),
        (9, now - Duration::from_secs(14), false),
        (10, now - Duration::from_secs(13), false),
    ];

    let candidate = UnifiedExecProcessManager::process_id_to_prune_from_meta(&meta);

    assert_eq!(candidate, Some(1));
}

#[test]
fn pruning_protects_recent_processes_even_if_exited() {
    let now = Instant::now();
    let meta = vec![
        (1, now - Duration::from_secs(40), false),
        (2, now - Duration::from_secs(30), false),
        (3, now - Duration::from_secs(20), true),
        (4, now - Duration::from_secs(19), false),
        (5, now - Duration::from_secs(18), false),
        (6, now - Duration::from_secs(17), false),
        (7, now - Duration::from_secs(16), false),
        (8, now - Duration::from_secs(15), false),
        (9, now - Duration::from_secs(14), false),
        (10, now - Duration::from_secs(13), true),
    ];

    let candidate = UnifiedExecProcessManager::process_id_to_prune_from_meta(&meta);

    // (10) is exited but among the last 8; we should drop the LRU outside that set.
    assert_eq!(candidate, Some(1));
}

async fn local_terminal_process() -> UnifiedExecProcess {
    let cwd = std::env::current_dir().expect("current dir");
    let spawned = spawn_pty_process(
        "/bin/sh",
        &["-lc".to_string(), "cat".to_string()],
        &cwd,
        &HashMap::new(),
        &None,
        TerminalSize { rows: 24, cols: 80 },
    )
    .await
    .expect("spawn local PTY");

    UnifiedExecProcess::from_spawned(spawned, SandboxType::None, Box::new(NoopSpawnLifecycle))
        .await
        .expect("wrap spawned PTY")
}

async fn insert_background_terminal(
    manager: &UnifiedExecProcessManager,
    process_id: i32,
) -> Arc<UnifiedExecProcess> {
    let process = Arc::new(local_terminal_process().await);
    let entry = ProcessEntry {
        process: Arc::clone(&process),
        call_id: "call".to_string(),
        process_id,
        command: vec!["bash".to_string(), "-lc".to_string(), "cat".to_string()],
        tty: true,
        network_approval_id: None,
        session: Weak::new(),
        transcript: Arc::new(Mutex::new(HeadTailBuffer::default())),
        attachment_state: AttachmentState::Detached,
        resume_after_user_interaction: Arc::new(AtomicBool::new(false)),
        last_used: Instant::now(),
    };
    manager
        .process_store
        .lock()
        .await
        .processes
        .insert(process_id, entry);
    process
}

#[tokio::test]
async fn attach_blocks_model_writes_and_allows_user_writes() {
    let manager = UnifiedExecProcessManager::new(DEFAULT_MAX_BACKGROUND_TERMINAL_TIMEOUT_MS);
    let process_id = 1000;
    let _process = insert_background_terminal(&manager, process_id).await;

    let attach = manager
        .attach_process(process_id, "conn-1".to_string())
        .await
        .expect("attach succeeds");
    assert_eq!(
        attach.summary.attachment_state,
        AttachmentState::UserAttached {
            owner_id: "conn-1".to_string(),
        }
    );

    let err = manager
        .write_stdin(WriteStdinRequest {
            process_id,
            input: "hello\n",
            yield_time_ms: 1_000,
            max_output_tokens: None,
        })
        .await
        .expect_err("model writes should be blocked while attached");
    assert!(matches!(
        err,
        UnifiedExecError::ProcessAttachedByUser {
            process_id: blocked_process_id,
        } if blocked_process_id == process_id
    ));

    let output = manager
        .write_user_stdin(
            "conn-1",
            WriteStdinRequest {
                process_id,
                input: "hello\n",
                yield_time_ms: 1_000,
                max_output_tokens: None,
            },
        )
        .await
        .expect("attached user writes succeed");
    assert_eq!(output.process_id, Some(process_id));
    assert!(String::from_utf8_lossy(&output.raw_output).contains("hello"));
}

#[tokio::test]
async fn detach_restores_model_writes_and_process_stays_alive() {
    let manager = UnifiedExecProcessManager::new(DEFAULT_MAX_BACKGROUND_TERMINAL_TIMEOUT_MS);
    let process_id = 1001;
    let _process = insert_background_terminal(&manager, process_id).await;

    manager
        .attach_process(process_id, "conn-1".to_string())
        .await
        .expect("attach succeeds");
    manager
        .detach_process(process_id, "conn-1")
        .await
        .expect("detach succeeds");

    let summaries = manager.list_background_terminals().await;
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].process_id, process_id);
    assert_eq!(summaries[0].attachment_state, AttachmentState::Detached);

    let output = manager
        .write_stdin(WriteStdinRequest {
            process_id,
            input: "after-detach\n",
            yield_time_ms: 1_000,
            max_output_tokens: None,
        })
        .await
        .expect("model writes work again after detach");
    assert!(String::from_utf8_lossy(&output.raw_output).contains("after-detach"));
}

#[tokio::test]
async fn resize_and_secure_prompt_state_work_while_attached() {
    let manager = UnifiedExecProcessManager::new(DEFAULT_MAX_BACKGROUND_TERMINAL_TIMEOUT_MS);
    let process_id = 1002;
    let _process = insert_background_terminal(&manager, process_id).await;

    manager
        .attach_process(process_id, "conn-1".to_string())
        .await
        .expect("attach succeeds");
    manager
        .resize_process(
            process_id,
            "conn-1",
            TerminalSize {
                rows: 30,
                cols: 100,
            },
        )
        .await
        .expect("attached PTY resize succeeds");
    manager
        .set_secure_input_pending(process_id, "conn-1", TerminalInputRedactionKind::Password)
        .await
        .expect("secure prompt state set");

    let summaries = manager.list_background_terminals().await;
    assert_eq!(summaries.len(), 1);
    assert_eq!(
        summaries[0].attachment_state,
        AttachmentState::SecureInputPending {
            owner_id: "conn-1".to_string(),
            kind: TerminalInputRedactionKind::Password,
        }
    );

    manager
        .clear_secure_input_pending(process_id, "conn-1")
        .await
        .expect("secure prompt state cleared");
    let summaries = manager.list_background_terminals().await;
    assert_eq!(
        summaries[0].attachment_state,
        AttachmentState::UserAttached {
            owner_id: "conn-1".to_string(),
        }
    );
}
