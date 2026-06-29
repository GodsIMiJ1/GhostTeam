use std::env;
use std::ffi::OsString;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{mpsc, Mutex};
use std::thread;

#[path = "../src/model/mod.rs"]
mod model;

static TEST_LOCK: Mutex<()> = Mutex::new(());

struct EnvGuard {
    key: &'static str,
    previous: Option<OsString>,
}

impl EnvGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let previous = env::var_os(key);
        unsafe {
            env::set_var(key, value);
        }
        Self { key, previous }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        unsafe {
            match &self.previous {
                Some(previous) => env::set_var(self.key, previous),
                None => env::remove_var(self.key),
            }
        }
    }
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|window| window == b"\r\n\r\n")
}

fn content_length(headers: &str) -> usize {
    headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.eq_ignore_ascii_case("content-length") {
                value.trim().parse::<usize>().ok()
            } else {
                None
            }
        })
        .unwrap_or(0)
}

fn read_http_body(stream: &mut TcpStream) -> String {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    let mut expected_body_len = None;

    loop {
        let n = stream.read(&mut chunk).expect("failed to read request");
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);

        if expected_body_len.is_none() {
            if let Some(header_end) = find_header_end(&buf) {
                let headers = String::from_utf8_lossy(&buf[..header_end]);
                expected_body_len = Some(content_length(&headers));
            }
        }

        if let Some(header_end) = find_header_end(&buf) {
            let body_len = buf.len().saturating_sub(header_end + 4);
            if body_len >= expected_body_len.unwrap_or(0) {
                break;
            }
        }
    }

    let header_end = find_header_end(&buf).expect("missing HTTP header terminator");
    String::from_utf8(buf[header_end + 4..].to_vec()).expect("request body is not utf-8")
}

fn spawn_ollama_server(response_json: String) -> (String, mpsc::Receiver<String>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("failed to bind mock ollama server");
    let addr = listener.local_addr().expect("failed to read local addr");
    let (tx, rx) = mpsc::channel();

    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("failed to accept ollama connection");
        let body = read_http_body(&mut stream);
        tx.send(body).expect("failed to send ollama body");

        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response_json.len(),
            response_json
        );
        stream
            .write_all(response.as_bytes())
            .expect("failed to write ollama response");
    });

    (format!("http://{addr}/api/generate"), rx, handle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ModelBackend;

    #[test]
    fn ollama_backend_posts_expected_payload() {
        let _guard = TEST_LOCK.lock().expect("test lock poisoned");
        let response = r#"{"response":"ollama reply"}"#.to_string();
        let (endpoint, rx, handle) = spawn_ollama_server(response);
        let _endpoint_guard = EnvGuard::set("GHOSTTEAM_OLLAMA_ENDPOINT", &endpoint);
        let _model_guard = EnvGuard::set("GHOSTTEAM_OLLAMA_MODEL", "llama3");

        let backend = model::ollama::OllamaBackend::default();
        let reply = backend.generate("hello from ollama").expect("ollama generation failed");

        assert_eq!(reply, "ollama reply");

        let body = rx.recv().expect("failed to receive ollama request body");
        assert!(body.contains("\"model\":\"llama3\""));
        assert!(body.contains("\"prompt\":\"hello from ollama\""));
        assert!(body.contains("\"stream\":false"));

        handle.join().expect("mock ollama server panicked");
    }

    #[test]
    fn llamacpp_backend_spawns_subprocess_and_reads_stdout() {
        let _guard = TEST_LOCK.lock().expect("test lock poisoned");
        let command = "$inputText = [Console]::In.ReadToEnd(); [Console]::Out.Write('mock:' + $inputText)";
        let args = serde_json::json!(["-NoProfile", "-Command", command]).to_string();
        let _bin_guard = EnvGuard::set("GHOSTTEAM_LLAMA_CPP_BIN", "powershell");
        let _args_guard = EnvGuard::set("GHOSTTEAM_LLAMA_CPP_ARGS", &args);

        let backend = model::llamacpp::LlamaCppBackend::default();
        let reply = backend
            .generate("hello from llama.cpp")
            .expect("llama.cpp generation failed");

        assert_eq!(reply, "mock:hello from llama.cpp");
    }

    #[test]
    fn ghostos_backend_returns_stub_text() {
        let _guard = TEST_LOCK.lock().expect("test lock poisoned");
        let backend = model::ghostos::GhostOsBackend::default();
        let reply = backend.generate("ping").expect("ghostos generation failed");

        assert!(reply.contains("ghostos stub"));
        assert!(reply.contains("ping"));
    }
}
