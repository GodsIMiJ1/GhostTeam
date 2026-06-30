use axum::{
    Router,
    http::header,
    response::{Html, IntoResponse},
    routing::get,
};

const INDEX_HTML: &str = include_str!("../../../dashboard/index.html");
const DASHBOARD_CSS: &str = include_str!("../../../dashboard/dashboard.css");
const DASHBOARD_JS: &str = include_str!("../../../dashboard/dashboard.js");
const DASHBOARD_ICON: &str = include_str!("../../../dashboard/dashboard.svg");

pub fn router() -> Router {
    Router::new()
        .route("/", get(index))
        .route("/dashboard", get(index))
        .route("/dashboard/", get(index))
        .route("/dashboard.css", get(styles))
        .route("/dashboard.js", get(script))
        .route("/dashboard.svg", get(icon))
}

pub async fn index() -> impl IntoResponse {
    Html(INDEX_HTML)
}

pub async fn styles() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/css; charset=utf-8")], DASHBOARD_CSS)
}

pub async fn script() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "application/javascript; charset=utf-8")], DASHBOARD_JS)
}

pub async fn icon() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "image/svg+xml; charset=utf-8")], DASHBOARD_ICON)
}
