use std::{io, sync::OnceLock};

use interprocess::local_socket::{GenericNamespaced, Name, ToNsName};

static SOCKET_NAME: OnceLock<String> = OnceLock::new();

/// Returns the deterministic, per-user local-socket name shared by the server
/// and the follow-stream client.
pub fn socket_name() -> io::Result<String> {
    if let Some(name) = SOCKET_NAME.get() {
        return Ok(name.clone());
    }

    let name = format!("shorthand.follow-stream.{}", current_identity()?);
    match SOCKET_NAME.set(name.clone()) {
        Ok(()) => Ok(name),
        Err(_) => SOCKET_NAME
            .get()
            .cloned()
            .ok_or_else(|| io::Error::other("socket name initialization raced unsuccessfully")),
    }
}

/// Returns the platform mapping for the shared socket name with owned storage.
pub fn socket_name_owned() -> io::Result<Name<'static>> {
    socket_name()?.to_ns_name::<GenericNamespaced>()
}

#[cfg(unix)]
fn current_identity() -> io::Result<String> {
    // SAFETY: geteuid takes no pointers and has no preconditions.
    Ok(unsafe { libc::geteuid() }.to_string())
}

#[cfg(windows)]
fn current_identity() -> io::Result<String> {
    current_user_sid()
}

#[cfg(windows)]
pub(crate) fn current_user_sid() -> io::Result<String> {
    use windows::{
        core::PWSTR,
        Win32::{
            Foundation::{CloseHandle, LocalFree, HANDLE, HLOCAL},
            Security::{GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER},
            System::Threading::{GetCurrentProcess, OpenProcessToken},
        },
    };

    use windows::Win32::Security::Authorization::ConvertSidToStringSidW;

    fn io_error(error: windows::core::Error) -> io::Error {
        io::Error::other(error.to_string())
    }

    let mut token = HANDLE::default();
    // SAFETY: the current-process pseudo-handle is always valid. The first token
    // query deliberately uses a null buffer to obtain its size; the second uses
    // aligned, sufficiently large storage. ConvertSidToStringSidW allocates its
    // result with LocalAlloc, so LocalFree releases it after copying. CloseHandle
    // releases the process-token handle on every success or failure path.
    unsafe {
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).map_err(io_error)?;

        let result = (|| {
            let mut required_bytes = 0;
            let sizing_result = GetTokenInformation(token, TokenUser, None, 0, &mut required_bytes);
            if required_bytes == 0 {
                return Err(sizing_result
                    .err()
                    .map(io_error)
                    .unwrap_or_else(|| io::Error::other("empty TokenUser information")));
            }

            let word_size = std::mem::size_of::<usize>();
            let word_count = (required_bytes as usize).div_ceil(word_size);
            let mut storage = vec![0usize; word_count];
            GetTokenInformation(
                token,
                TokenUser,
                Some(storage.as_mut_ptr().cast()),
                required_bytes,
                &mut required_bytes,
            )
            .map_err(io_error)?;

            let token_user = &*storage.as_ptr().cast::<TOKEN_USER>();
            let mut raw = PWSTR::null();
            ConvertSidToStringSidW(token_user.User.Sid, &mut raw).map_err(io_error)?;
            let sid = raw
                .to_string()
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error));
            let _ = LocalFree(Some(HLOCAL(raw.0.cast())));
            sid
        })();

        let close_result = CloseHandle(token).map_err(io_error);
        match (result, close_result) {
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(error),
            (Ok(sid), Ok(())) => Ok(sid),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_name_is_stable_nonempty_and_scoped_to_the_current_user() {
        let first = socket_name().unwrap();
        let second = socket_name().unwrap();

        assert_eq!(first, second);
        assert!(!first.is_empty());

        #[cfg(windows)]
        assert!(first.starts_with("shorthand.follow-stream.S-1-"));

        #[cfg(unix)]
        assert!(first.ends_with(&format!(".{}", unsafe { libc::geteuid() })));
    }
}
