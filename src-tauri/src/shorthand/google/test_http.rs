//! A one-request-per-route HTTP stub for the Google module's tests, so they
//! need no mock-server crate. Each route answers once; a request that
//! matches no unused route gets a 404, which fails the call under test.

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

pub struct Route {
    /// `"METHOD /path"`, matched against the request line without its query.
    pub matches: &'static str,
    pub status: u16,
    pub body: String,
}

impl Route {
    pub fn json(matches: &'static str, status: u16, body: &str) -> Self {
        Self {
            matches,
            status,
            body: body.to_string(),
        }
    }
}

#[derive(Debug)]
pub struct Recorded {
    pub method: String,
    /// Path and query, as sent.
    pub target: String,
    pub body: String,
}

/// A client that never reuses a connection, since the stub closes each one.
pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .pool_max_idle_per_host(0)
        .build()
        .unwrap()
}

/// Serves `routes` on a fresh port. The handle resolves to every request
/// seen, in order, once each route has answered or a request missed.
pub async fn serve(routes: Vec<Route>) -> (String, JoinHandle<Vec<Recorded>>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let handle = tokio::spawn(async move {
        let mut remaining: Vec<Option<Route>> = routes.into_iter().map(Some).collect();
        let mut seen = Vec::new();
        while remaining.iter().any(Option::is_some) {
            let Ok(Ok((mut stream, _))) =
                tokio::time::timeout(std::time::Duration::from_secs(5), listener.accept()).await
            else {
                break;
            };
            let request = read_request(&mut stream).await;
            let path = request.target.split('?').next().unwrap_or("").to_string();
            let key = format!("{} {}", request.method, path);
            let slot = remaining
                .iter_mut()
                .find(|route| route.as_ref().is_some_and(|r| r.matches == key));
            let (status, body) = match slot.and_then(Option::take) {
                Some(route) => (route.status, route.body),
                None => (404, String::new()),
            };
            let response = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.shutdown().await;
            let missed = status == 404;
            seen.push(request);
            if missed {
                break;
            }
        }
        seen
    });
    (base, handle)
}

async fn read_request(stream: &mut tokio::net::TcpStream) -> Recorded {
    let mut data = Vec::new();
    let mut buffer = [0u8; 4096];
    let header_end = loop {
        if let Some(index) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            break index + 4;
        }
        let read = stream.read(&mut buffer).await.unwrap_or(0);
        if read == 0 {
            break data.len();
        }
        data.extend_from_slice(&buffer[..read]);
    };
    let head = String::from_utf8_lossy(&data[..header_end]).to_string();
    let length = head
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    while data.len() < header_end + length {
        let read = stream.read(&mut buffer).await.unwrap_or(0);
        if read == 0 {
            break;
        }
        data.extend_from_slice(&buffer[..read]);
    }
    let mut first = head.lines().next().unwrap_or("").split_whitespace();
    Recorded {
        method: first.next().unwrap_or("").to_string(),
        target: first.next().unwrap_or("").to_string(),
        body: String::from_utf8_lossy(&data[header_end..]).to_string(),
    }
}
