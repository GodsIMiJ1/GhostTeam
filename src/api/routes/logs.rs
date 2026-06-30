use std::path::PathBuf;
use std::time::Duration;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use tokio::fs::OpenOptions;
use tokio::io::{AsyncBufReadExt, AsyncSeekExt, BufReader};
use tokio::time::sleep;

const LOGS_DIR: &str = ".ghostteam/logs";

pub fn router() -> Router {
    Router::new().route("/:agent/stream", get(stream_logs))
}

pub async fn stream_logs(Path(agent): Path<String>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| tail_log_stream(socket, agent))
}

async fn tail_log_stream(mut socket: WebSocket, agent: String) {
    let log_path = log_file_path(&agent);
    log::info!("opening log stream for agent={agent} path={}", log_path.display());

    loop {
        match OpenOptions::new().read(true).open(&log_path).await {
            Ok(file) => {
                if let Err(error) = stream_file(socket, file, &log_path, &agent).await {
                    log::error!(
                        "log stream terminated for agent={agent} path={} error={error}",
                        log_path.display()
                    );
                }
                break;
            }
            Err(error) => {
                if let Err(ws_error) = socket.send(Message::Text(format!(
                    "waiting for log file {}: {error}",
                    log_path.display()
                ).into()))
                .await
                {
                    log::error!(
                        "failed to notify websocket client for agent={agent}: {ws_error}"
                    );
                    break;
                }
                sleep(Duration::from_millis(500)).await;
            }
        }
    }
}

async fn stream_file(
    mut socket: WebSocket,
    file: tokio::fs::File,
    log_path: &PathBuf,
    agent: &str,
) -> anyhow::Result<()> {
    let mut reader = BufReader::new(file);
    reader.seek(std::io::SeekFrom::End(0)).await?;
    let mut lines = reader.lines();

    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                log::debug!("streaming log line agent={agent} bytes={}", line.len());
                if socket.send(Message::Text(line.into())).await.is_err() {
                    log::info!("websocket closed for agent={agent} path={}", log_path.display());
                    break;
                }
            }
            Ok(None) => {
                sleep(Duration::from_millis(500)).await;
                continue;
            }
            Err(error) => {
                return Err(error.into());
            }
        }
    }

    Ok(())
}

fn log_file_path(agent: &str) -> PathBuf {
    PathBuf::from(LOGS_DIR).join(format!("{agent}.log"))
}
