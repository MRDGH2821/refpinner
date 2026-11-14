use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;

/// A local stand-in for the GitHub REST API that records each request path.
pub struct Forge {
    pub url: String,
    pub requests: Arc<Mutex<Vec<String>>>,
}

impl Forge {
    /// Serves `(path, status, body)` routes; unknown paths get 404.
    pub fn new(routes: &[(&str, u16, &str)]) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock forge");
        let url = format!("http://{}", listener.local_addr().expect("mock address"));
        let routes: Vec<(String, u16, String)> = routes
            .iter()
            .map(|(path, status, body)| ((*path).to_owned(), *status, (*body).to_owned()))
            .collect();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&requests);
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(&stream);
                let mut request_line = String::new();
                let _ = reader.read_line(&mut request_line);
                let path = request_line.split(' ').nth(1).unwrap_or("").to_owned();
                loop {
                    let mut header = String::new();
                    if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
                        break;
                    }
                }
                log.lock().expect("request log").push(path.clone());
                let (status, body) = routes
                    .iter()
                    .find(|(route, ..)| *route == path)
                    .map_or((404, r#"{"message":"Not Found"}"#), |(_, status, body)| {
                        (*status, body.as_str())
                    });
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} Mock\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        Self { url, requests }
    }

    #[allow(dead_code)]
    pub fn count(&self, path: &str) -> usize {
        self.requests
            .lock()
            .expect("request log")
            .iter()
            .filter(|request| *request == path)
            .count()
    }

    pub fn total(&self) -> usize {
        self.requests.lock().expect("request log").len()
    }
}
