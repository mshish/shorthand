use std::{
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use interprocess::local_socket::{
    tokio::{Listener, Stream},
    traits::tokio::{Listener as _, Stream as _},
    ListenerOptions, Name,
};
use tauri::AppHandle;
use tokio::io::AsyncWriteExt;

use super::{socket_name_owned, FollowStreamHub};

type TaskHandle = tauri::async_runtime::JoinHandle<()>;

pub struct FollowStreamServer {
    lifecycle: tokio::sync::Mutex<()>,
    inner: Mutex<Option<RunningServer>>,
}

struct RunningServer {
    hub: Arc<FollowStreamHub>,
    accepting: Arc<AtomicBool>,
    listener: TaskHandle,
    followers: Arc<Mutex<Vec<TaskHandle>>>,
}

impl Default for FollowStreamServer {
    fn default() -> Self {
        Self {
            lifecycle: tokio::sync::Mutex::new(()),
            inner: Mutex::new(None),
        }
    }
}

impl FollowStreamServer {
    pub(crate) async fn lock_lifecycle(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.lifecycle.lock().await
    }

    pub async fn start(&self, app: &AppHandle, hub: Arc<FollowStreamHub>) -> io::Result<()> {
        self.start_with_name(
            socket_name_owned()?,
            &app.package_info().version.to_string(),
            hub,
        )
        .await
    }

    pub(crate) async fn start_with_name(
        &self,
        name: Name<'static>,
        app_version: &str,
        hub: Arc<FollowStreamHub>,
    ) -> io::Result<()> {
        const MAX_ATTEMPTS: usize = 10;
        const RETRY_DELAY: Duration = Duration::from_millis(50);

        for attempt in 1..=MAX_ATTEMPTS {
            match self.start_inner(name.clone(), app_version.to_string(), Arc::clone(&hub)) {
                Ok(()) => return Ok(()),
                Err(error) if attempt < MAX_ATTEMPTS && is_retryable_listener_error(&error) => {
                    tokio::time::sleep(RETRY_DELAY).await;
                }
                Err(error) => return Err(error),
            }
        }

        unreachable!("listener retry loop always returns")
    }

    fn start_inner(
        &self,
        name: Name<'static>,
        app_version: String,
        hub: Arc<FollowStreamHub>,
    ) -> io::Result<()> {
        let mut running = self.inner.lock().unwrap();
        if running.is_some() {
            return Ok(());
        }

        // Tokio's Windows named-pipe constructor requires an entered runtime
        // with its I/O driver enabled, so enter Tauri's runtime explicitly for
        // construction.
        let listener_result = {
            let runtime = tauri::async_runtime::handle();
            let _runtime_guard = runtime.inner().enter();
            create_listener(name)
        };
        let listener = match listener_result {
            Ok(listener) => listener,
            Err(error) => {
                hub.set_enabled(false);
                log::error!("Failed to create follow-stream listener: {error}");
                return Err(error);
            }
        };

        let followers = Arc::new(Mutex::new(Vec::new()));
        let accepting = Arc::new(AtomicBool::new(true));

        // subscribe rejects while disabled, so enable before the accept task can
        // observe a connection and register its follower.
        hub.set_enabled(true);
        let listener_handle = tauri::async_runtime::spawn(accept_loop(
            listener,
            app_version,
            Arc::clone(&hub),
            Arc::clone(&accepting),
            Arc::clone(&followers),
        ));
        *running = Some(RunningServer {
            hub,
            accepting,
            listener: listener_handle,
            followers,
        });
        log::info!("Follow-stream listener started");
        Ok(())
    }

    pub fn stop(&self) {
        let Some(running) = self.inner.lock().unwrap().take() else {
            return;
        };

        running.hub.set_enabled(false);
        running.accepting.store(false, Ordering::Release);
        running.listener.abort();

        // Aborting is deliberately bounded: a suspended reader can leave its
        // follower task parked in write_all, which eviction alone cannot wake.
        let mut followers = running.followers.lock().unwrap();
        for follower in followers.drain(..) {
            follower.abort();
        }
        log::info!("Follow-stream listener stopped");
    }

    #[cfg(test)]
    fn is_running(&self) -> bool {
        self.inner.lock().unwrap().is_some()
    }
}

fn is_retryable_listener_error(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::PermissionDenied | io::ErrorKind::AddrInUse | io::ErrorKind::AlreadyExists
    )
}

impl Drop for FollowStreamServer {
    fn drop(&mut self) {
        self.stop();
    }
}

async fn accept_loop(
    listener: Listener,
    app_version: String,
    hub: Arc<FollowStreamHub>,
    accepting: Arc<AtomicBool>,
    follower_handles: Arc<Mutex<Vec<TaskHandle>>>,
) {
    loop {
        let stream = match listener.accept().await {
            Ok(stream) => stream,
            Err(error) => {
                // Listener errors can be transient (especially while Windows is
                // replenishing named-pipe instances). Retry with a delay so one
                // bad accept does not permanently disable the configured service
                // and a persistent failure cannot spin the runtime.
                log::warn!("Follow-stream accept failed: {error}");
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };

        #[cfg(unix)]
        if !peer_is_current_user(&stream) {
            continue;
        }

        let (follower, backlog) = match hub.subscribe(&app_version) {
            Ok(subscription) => subscription,
            Err(error) => {
                write_rejection(stream, hub.rejection_line(error)).await;
                continue;
            }
        };

        let mut handles = follower_handles.lock().unwrap();
        handles.retain(|handle| !handle.inner().is_finished());
        if !accepting.load(Ordering::Acquire) {
            hub.unsubscribe(follower.id());
            continue;
        }

        let follower_hub = Arc::clone(&hub);
        handles.push(tauri::async_runtime::spawn(async move {
            serve_follower(stream, follower, backlog, follower_hub).await;
        }));
    }
}

async fn write_rejection(stream: Stream, line: Arc<str>) {
    let (_, mut writer) = stream.split();
    if let Err(error) = async {
        writer.write_all(line.as_bytes()).await?;
        writer.flush().await
    }
    .await
    {
        log::debug!("Follow-stream rejection write ended early: {error}");
    }
}

async fn serve_follower(
    stream: Stream,
    follower: Arc<super::Follower>,
    backlog: Vec<Arc<str>>,
    hub: Arc<FollowStreamHub>,
) {
    let follower_id = follower.id();
    let (_, mut writer) = stream.split();
    let result = async {
        for line in backlog {
            writer.write_all(line.as_bytes()).await?;
        }
        writer.flush().await?;

        loop {
            for line in follower.drain() {
                writer.write_all(line.as_bytes()).await?;
            }
            writer.flush().await?;
            if !follower.wait().await {
                break;
            }
        }
        for line in follower.drain() {
            let _ = writer.write_all(line.as_bytes()).await;
        }
        let _ = writer.flush().await;
        Ok::<(), io::Error>(())
    }
    .await;

    if let Err(error) = result {
        log::debug!("Follow-stream follower {follower_id} disconnected: {error}");
    }
    // Harmless when set_enabled(false) or overflow eviction removed it first.
    hub.unsubscribe(follower_id);
}

#[cfg(unix)]
fn peer_is_current_user(stream: &Stream) -> bool {
    use interprocess::local_socket::traits::StreamCommon as _;

    let expected = unsafe { libc::geteuid() };
    match stream.peer_creds() {
        Ok(credentials) if credentials.euid() == Some(expected) => true,
        Ok(credentials) => {
            log::warn!(
                "Rejecting follow-stream peer with euid {:?}; expected {expected}",
                credentials.euid()
            );
            false
        }
        Err(error) => {
            log::warn!("Rejecting follow-stream peer without credentials: {error}");
            false
        }
    }
}

#[cfg(windows)]
fn protected_sddl(sid: &str) -> String {
    format!("D:P(A;;GA;;;{sid})")
}

#[cfg(windows)]
fn create_listener(name: Name<'static>) -> io::Result<Listener> {
    use interprocess::os::windows::{
        local_socket::ListenerOptionsExt, security_descriptor::SecurityDescriptor,
    };
    use widestring::U16CString;

    let sid = super::name::current_user_sid()?;
    let sddl = U16CString::from_str(protected_sddl(&sid))
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.to_string()))?;
    let descriptor = SecurityDescriptor::deserialize(&sddl)?;
    ListenerOptions::new()
        .name(name)
        .security_descriptor(descriptor)
        .create_tokio()
}

#[cfg(unix)]
fn create_listener(name: Name<'static>) -> io::Result<Listener> {
    use interprocess::os::unix::local_socket::ListenerOptionsExt;

    ListenerOptions::new().name(name).mode(0o600).create_tokio()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use interprocess::local_socket::{GenericNamespaced, ToNsName};
    use tokio::io::{AsyncBufReadExt, BufReader};

    use crate::managers::transcription::StreamSource;

    use super::*;
    use crate::follow_stream::Speaker;

    static NEXT_TEST_NAME: AtomicU64 = AtomicU64::new(1);

    fn unique_name_text(test: &str) -> String {
        format!(
            "{}.test.{test}.{}.{}",
            super::super::socket_name().unwrap(),
            std::process::id(),
            NEXT_TEST_NAME.fetch_add(1, Ordering::Relaxed)
        )
    }

    fn unique_name(test: &str) -> Name<'static> {
        unique_name_text(test)
            .to_ns_name::<GenericNamespaced>()
            .unwrap()
    }

    async fn connect(name: Name<'static>) -> BufReader<Stream> {
        let stream = tokio::time::timeout(Duration::from_secs(2), Stream::connect(name))
            .await
            .expect("client connection timed out")
            .expect("client failed to connect");
        BufReader::new(stream)
    }

    async fn read_raw_line(reader: &mut BufReader<Stream>) -> String {
        let mut line = String::new();
        let bytes = tokio::time::timeout(Duration::from_secs(2), reader.read_line(&mut line))
            .await
            .expect("client read timed out")
            .expect("client read failed");
        assert_ne!(bytes, 0, "unexpected EOF");
        line
    }

    /// These tests are about the transport, not the clock, so drop the trailing
    /// stamp. `stamps_survive_the_transport_unmodified` covers the stamp itself,
    /// and the hub tests pin its exact bytes.
    async fn read_line(reader: &mut BufReader<Stream>) -> String {
        let line = read_raw_line(reader).await;
        let start = line
            .find(",\"emitted_at\":")
            .unwrap_or_else(|| panic!("every event is stamped, got {line}"));
        format!("{}}}\n", &line[..start])
    }

    #[tokio::test]
    async fn transport_round_trip_preserves_exact_ndjson_order() {
        let name = unique_name("round_trip");
        let hub = Arc::new(FollowStreamHub::default());
        let server = FollowStreamServer::default();
        server
            .start_with_name(name.clone(), "test-version", Arc::clone(&hub))
            .await
            .unwrap();
        let mut client = connect(name).await;

        assert_eq!(
            read_line(&mut client).await,
            "{\"t\":\"hello\",\"protocol\":1,\"version\":\"test-version\"}\n"
        );
        hub.begin(true);
        assert_eq!(
            read_line(&mut client).await,
            "{\"t\":\"begin\",\"session\":1,\"streaming\":true}\n"
        );
        hub.partial(StreamSource::Mic, "hello ", "wor");
        assert_eq!(
            read_line(&mut client).await,
            "{\"t\":\"partial\",\"session\":1,\"speaker\":\"me\",\"committed\":\"hello \",\"tentative\":\"wor\"}\n"
        );
        hub.finish(Some(Speaker::Me), "Hello world.");
        assert_eq!(
            read_line(&mut client).await,
            "{\"t\":\"final\",\"session\":1,\"speaker\":\"me\",\"text\":\"Hello world.\"}\n"
        );

        server.stop();
    }

    #[tokio::test]
    async fn stamps_survive_the_transport_unmodified() {
        let name = unique_name("stamps");
        let clock = super::super::hub::TestClock::new();
        let hub = Arc::new(FollowStreamHub::with_clock(Arc::clone(&clock) as Arc<_>));
        let server = FollowStreamServer::default();
        server
            .start_with_name(name.clone(), "test-version", Arc::clone(&hub))
            .await
            .unwrap();
        let mut client = connect(name).await;

        assert_eq!(
            read_raw_line(&mut client).await,
            "{\"t\":\"hello\",\"protocol\":1,\"version\":\"test-version\",\"emitted_at\":\"2026-08-15T14:03:20.100-07:00\"}\n"
        );
        clock.advance(100);
        hub.begin(true);
        clock.advance(1112);
        hub.partial(StreamSource::Mic, "hello ", "wor");

        assert_eq!(
            read_raw_line(&mut client).await,
            "{\"t\":\"begin\",\"session\":1,\"streaming\":true,\"emitted_at\":\"2026-08-15T14:03:20.200-07:00\",\"session_elapsed_ms\":0}\n"
        );
        assert_eq!(
            read_raw_line(&mut client).await,
            "{\"t\":\"partial\",\"session\":1,\"speaker\":\"me\",\"committed\":\"hello \",\"tentative\":\"wor\",\"emitted_at\":\"2026-08-15T14:03:21.312-07:00\",\"session_elapsed_ms\":1112}\n"
        );

        server.stop();
    }

    #[tokio::test]
    async fn late_attach_receives_live_session_snapshot() {
        let name = unique_name("late_attach");
        let hub = Arc::new(FollowStreamHub::default());
        let server = FollowStreamServer::default();
        server
            .start_with_name(name.clone(), "test-version", Arc::clone(&hub))
            .await
            .unwrap();
        let mut first = connect(name.clone()).await;
        assert!(read_line(&mut first).await.contains("\"t\":\"hello\""));

        hub.begin(true);
        hub.partial(StreamSource::System, "system", " audio");

        let mut late = connect(name).await;
        assert_eq!(
            read_line(&mut late).await,
            "{\"t\":\"hello\",\"protocol\":1,\"version\":\"test-version\"}\n"
        );
        assert_eq!(
            read_line(&mut late).await,
            "{\"t\":\"begin\",\"session\":1,\"streaming\":true}\n"
        );
        assert_eq!(
            read_line(&mut late).await,
            "{\"t\":\"partial\",\"session\":1,\"speaker\":\"them\",\"committed\":\"system\",\"tentative\":\" audio\"}\n"
        );

        server.stop();
    }

    #[tokio::test]
    async fn ninth_transport_connection_receives_limit_error_and_eof() {
        let name = unique_name("follower_limit");
        let hub = Arc::new(FollowStreamHub::default());
        let server = FollowStreamServer::default();
        server
            .start_with_name(name.clone(), "test-version", Arc::clone(&hub))
            .await
            .unwrap();

        let mut clients = Vec::new();
        for _ in 0..super::super::MAX_FOLLOWERS {
            let mut client = connect(name.clone()).await;
            assert!(read_line(&mut client).await.contains("\"t\":\"hello\""));
            clients.push(client);
        }

        let mut rejected = connect(name).await;
        assert_eq!(
            read_line(&mut rejected).await,
            "{\"t\":\"error\",\"code\":\"follower_limit\",\"message\":\"maximum number of follow-stream followers reached\"}\n"
        );
        let mut tail = String::new();
        let bytes = tokio::time::timeout(Duration::from_secs(2), rejected.read_line(&mut tail))
            .await
            .expect("rejected connection did not close")
            .unwrap();
        assert_eq!(bytes, 0);

        server.stop();
    }

    #[tokio::test]
    async fn stop_closes_followers_and_listener_within_the_timeout() {
        let name = unique_name("bounded_stop");
        let hub = Arc::new(FollowStreamHub::default());
        let server = FollowStreamServer::default();
        server
            .start_with_name(name.clone(), "test-version", hub)
            .await
            .unwrap();
        let mut client = connect(name.clone()).await;
        assert!(read_line(&mut client).await.contains("\"t\":\"hello\""));

        server.stop();
        assert!(!server.is_running());

        let mut tail = String::new();
        let bytes = tokio::time::timeout(Duration::from_secs(2), client.read_line(&mut tail))
            .await
            .expect("follower did not observe bounded EOF")
            .unwrap();
        assert_eq!(bytes, 0);

        tokio::time::timeout(Duration::from_secs(2), async {
            while let Ok(stream) = Stream::connect(name.clone()).await {
                drop(stream);
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("listener still accepted connections after stop");
    }

    #[tokio::test]
    async fn listener_contention_retries_until_existing_listener_is_released() {
        let name = unique_name("retry_contention");
        let first_listener = create_listener(name.clone()).unwrap();
        let collision = match create_listener(name.clone()) {
            Ok(_) => panic!("a second listener unexpectedly acquired the same name"),
            Err(error) => error,
        };
        assert!(
            is_retryable_listener_error(&collision),
            "listener collision produced non-retryable error kind {:?} (raw OS error {:?})",
            collision.kind(),
            collision.raw_os_error()
        );

        let hub = Arc::new(FollowStreamHub::default());
        let server = FollowStreamServer::default();
        let result = tokio::time::timeout(Duration::from_secs(2), async {
            tokio::join!(
                server.start_with_name(name, "test-version", Arc::clone(&hub)),
                async move {
                    tokio::time::sleep(Duration::from_millis(75)).await;
                    drop(first_listener);
                }
            )
            .0
        })
        .await
        .expect("listener retry did not complete after contention cleared");

        result.expect("listener retry should succeed after contention clears");
        assert!(server.is_running());
        assert!(hub.is_enabled());
        server.stop();
    }

    #[tokio::test]
    async fn listener_contention_gives_up_and_rolls_back_without_hanging() {
        let name = unique_name("retry_exhaustion");
        let _first_listener = create_listener(name.clone()).unwrap();
        let hub = Arc::new(FollowStreamHub::default());
        let server = FollowStreamServer::default();

        let error = tokio::time::timeout(
            Duration::from_secs(2),
            server.start_with_name(name, "test-version", Arc::clone(&hub)),
        )
        .await
        .expect("listener retry budget hung under permanent contention")
        .expect_err("listener unexpectedly started under permanent contention");

        assert!(is_retryable_listener_error(&error));
        assert!(!server.is_running());
        assert!(!hub.is_enabled());
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn created_listener_has_protected_current_user_only_dacl() {
        use windows::{
            core::PWSTR,
            Win32::{
                Foundation::{CloseHandle, LocalFree, HLOCAL},
                Security::{
                    Authorization::{
                        ConvertSecurityDescriptorToStringSecurityDescriptorW, GetSecurityInfo,
                        SDDL_REVISION_1, SE_FILE_OBJECT,
                    },
                    DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
                },
                Storage::FileSystem::{
                    CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE,
                    OPEN_EXISTING, READ_CONTROL,
                },
            },
        };

        let test_name = unique_name_text("kernel_dacl");
        let name = test_name.clone().to_ns_name::<GenericNamespaced>().unwrap();
        let _listener = create_listener(name).unwrap();
        let sid = super::super::name::current_user_sid().unwrap();

        let pipe_path = widestring::U16CString::from_str(format!(r"\\.\pipe\{test_name}")).unwrap();
        let handle = unsafe {
            CreateFileW(
                windows::core::PCWSTR(pipe_path.as_ptr()),
                READ_CONTROL.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                None,
            )
        }
        .unwrap();

        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        unsafe {
            GetSecurityInfo(
                handle,
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                None,
                None,
                None,
                None,
                Some(&mut descriptor),
            )
            .ok()
            .expect("GetSecurityInfo should read back the pipe's DACL");
        }

        let mut rendered = PWSTR::null();
        unsafe {
            ConvertSecurityDescriptorToStringSecurityDescriptorW(
                descriptor,
                SDDL_REVISION_1,
                DACL_SECURITY_INFORMATION,
                &mut rendered,
                None,
            )
            .unwrap();
        }
        let sddl = unsafe { rendered.to_string().unwrap() };

        unsafe {
            let _ = LocalFree(Some(HLOCAL(rendered.0.cast())));
            let _ = LocalFree(Some(HLOCAL(descriptor.0)));
            CloseHandle(handle).unwrap();
        }

        assert!(sddl.starts_with("D:P"));
        assert!(sddl.contains(&sid));
        assert!(!sddl.contains(";;;WD)"), "Everyone must not have access");
        assert!(!sddl.contains(";;;AN)"), "Anonymous must not have access");
        // GENERIC_ALL is generic-mapped to FILE_ALL_ACCESS when Windows stores
        // the descriptor on a file object, so the kernel returns FA, not GA.
        assert_eq!(sddl, format!("D:P(A;;FA;;;{sid})"));
    }
}
