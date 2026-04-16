use super::*;
use pretty_assertions::assert_eq;

fn test_thread_id() -> ThreadId {
    ThreadId::from_string("11111111-1111-1111-1111-111111111111").expect("valid thread id")
}

fn sample_terminal(
    secure_input_prompt: Option<AppServerTerminalInputRedactionKind>,
) -> BackgroundTerminal {
    BackgroundTerminal {
        process_id: "42".to_string(),
        command: "git push".to_string(),
        tty: true,
        attached: true,
        secure_input_prompt,
    }
}

#[tokio::test]
async fn attach_slash_command_requests_background_terminal_picker() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.thread_id = Some(test_thread_id());

    chat.dispatch_command(SlashCommand::Attach);

    match rx.try_recv().expect("attach should request terminal list") {
        AppEvent::ListBackgroundTerminals {
            thread_id,
            open_picker,
        } => {
            assert_eq!(thread_id, test_thread_id());
            assert!(open_picker);
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[tokio::test]
async fn attached_terminal_routes_keyboard_input_and_detach() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.thread_id = Some(test_thread_id());
    chat.attach_background_terminal(sample_terminal(None), b"");

    chat.handle_key_event(KeyEvent::from(KeyCode::Char('a')));
    match rx.try_recv().expect("key press should route to terminal") {
        AppEvent::WriteBackgroundTerminalInput {
            thread_id,
            process_id,
            data,
        } => {
            assert_eq!(thread_id, test_thread_id());
            assert_eq!(process_id, "42");
            assert_eq!(data, "a");
        }
        other => panic!("unexpected event: {other:?}"),
    }

    chat.handle_key_event(KeyEvent::new(KeyCode::Char(']'), KeyModifiers::CONTROL));
    match rx.try_recv().expect("detach shortcut should emit detach") {
        AppEvent::DetachBackgroundTerminal {
            thread_id,
            process_id,
        } => {
            assert_eq!(thread_id, test_thread_id());
            assert_eq!(process_id, "42");
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[tokio::test]
async fn secure_input_popup_masks_text_and_submits_redacted_event() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.thread_id = Some(test_thread_id());
    chat.attach_background_terminal(
        sample_terminal(Some(AppServerTerminalInputRedactionKind::Password)),
        b"Password: ",
    );

    let before = render_bottom_popup(&chat, /*width*/ 80);
    assert_chatwidget_snapshot!("background_terminal_secure_input_popup", before);

    chat.handle_paste("hunter2".to_string());
    let after = render_bottom_popup(&chat, /*width*/ 80);
    assert!(!after.contains("hunter2"));
    assert_chatwidget_snapshot!("background_terminal_secure_input_popup_masked", after);

    chat.handle_key_event(KeyEvent::from(KeyCode::Enter));
    match rx
        .try_recv()
        .expect("secure input submit should emit app event")
    {
        AppEvent::WriteBackgroundTerminalSecureInput {
            thread_id,
            process_id,
            data,
            kind,
        } => {
            assert_eq!(thread_id, test_thread_id());
            assert_eq!(process_id, "42");
            assert_eq!(data.0, "hunter2");
            assert_eq!(kind, AppServerTerminalInputRedactionKind::Password);
        }
        other => panic!("unexpected event: {other:?}"),
    }
}
