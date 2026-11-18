// Entry point and config. Not in the spec (target, port and database path are asked at
// compile time, see "project config" decision): taken from the environment.
use url_shortener::api::{AppState, router};
use url_shortener::repository::Repository;
use url_shortener::utils::Clock;
use url_shortener::expiry;

#[tokio::main]
async fn main() {
    let db = std::env::var("DATABASE_PATH").unwrap_or_else(|_| "links.db".into());
    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".into());

    let repo = Repository::open(&db).expect("open database");
    let clock = Clock::system();
    expiry::spawn(repo.clone(), clock.clone());

    let app = router(AppState { repo, clock });
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await.expect("bind");
    println!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
