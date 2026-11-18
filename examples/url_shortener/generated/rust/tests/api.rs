// intentcode: ../../intent/tests.icode
// spec-hash: 7ab626896a4b870b  (sha256 prefix of: tests.icode, api.icode, expiry.icode, repository.icode, utils.icode, types.icode)
// generated: claude-sonnet-5-5, by hand-driven compile, not reproducible
// One test per `test` block. Each runs against a fresh in-memory database and a manual clock.
use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode, header};
use chrono::{DateTime, Duration, TimeZone, Utc};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use url_shortener::api::{AppState, hash_of, router};
use url_shortener::expiry;
use url_shortener::repository::{Link, Repository};
use url_shortener::utils::Clock;

struct App {
    router: Router,
    repo: Repository,
    clock: Clock,
}

struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    body: Value,
}

impl App {
    fn new() -> App {
        let repo = Repository::open_in_memory().unwrap();
        let clock = Clock::manual(Utc.with_ymd_and_hms(2030, 1, 1, 0, 0, 0).unwrap());
        let router = router(AppState { repo: repo.clone(), clock: clock.clone() });
        App { router, repo, clock }
    }

    async fn send(&self, req: Request<Body>) -> Reply {
        let res = self.router.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let headers = res.headers().clone();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        Reply { status, headers, body }
    }

    async fn post(&self, body: Value) -> Reply {
        self.post_raw(body.to_string()).await
    }

    async fn post_raw(&self, body: String) -> Reply {
        let req = Request::post("/url")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body))
            .unwrap();
        self.send(req).await
    }

    async fn get(&self, path: &str) -> Reply {
        self.send(Request::get(path).body(Body::empty()).unwrap()).await
    }

    fn now(&self) -> DateTime<Utc> {
        self.clock.now()
    }

    /// Stores a link for `target` directly, bypassing the API.
    async fn store(&self, slug: &str, target: &str, expires: Option<DateTime<Utc>>) {
        self.repo
            .insert(Link {
                slug: slug.to_string(),
                target: target.to_string(),
                created: self.now(),
                expires,
                deleted: None,
            })
            .await
            .unwrap();
    }
}

const TARGET: &str = "https://example.com/a";

fn matches_link_response(body: &Value) -> bool {
    let o = body.as_object().unwrap();
    o.len() == 4
        && o["slug"].is_string()
        && o["target"].is_string()
        && o["created"].is_string()
        && (o["expires"].is_string() || o["expires"].is_null())
}

fn slug_of(reply: &Reply) -> String {
    reply.body["slug"].as_str().unwrap().to_string()
}

// intent: test creates_a_short_link
#[tokio::test]
async fn creates_a_short_link() {
    let app = App::new();
    let r = app.post(json!({ "target": TARGET })).await;
    assert_eq!(r.status, StatusCode::CREATED);
    assert!(matches_link_response(&r.body));
    let slug = slug_of(&r);
    assert!((4..=12).contains(&slug.len()), "slug {slug}");
    let expires = DateTime::parse_from_rfc3339(r.body["expires"].as_str().unwrap()).unwrap();
    assert_eq!(expires.with_timezone(&Utc), app.now() + Duration::days(30));
}

// intent: test same_target_returns_the_same_link
#[tokio::test]
async fn same_target_returns_the_same_link() {
    let app = App::new();
    let first = app.post(json!({ "target": TARGET })).await;
    assert_eq!(first.status, StatusCode::CREATED);
    let second = app.post(json!({ "target": TARGET })).await;
    assert_eq!(second.status, StatusCode::OK);
    assert_eq!(slug_of(&first), slug_of(&second));
    let links = app.repo.with_prefix("").await.unwrap();
    assert_eq!(links.iter().filter(|l| l.target == TARGET).count(), 1);
}

// intent: test longer_slug_when_the_prefix_is_taken
#[tokio::test]
async fn longer_slug_when_the_prefix_is_taken() {
    let app = App::new();
    let hash = hash_of(TARGET);
    let prefix = &hash[..4];
    app.store(prefix, "https://other.example/x", None).await;
    let r = app.post(json!({ "target": TARGET })).await;
    assert_eq!(r.status, StatusCode::CREATED);
    let slug = slug_of(&r);
    assert!(slug.starts_with(prefix) && slug.len() > prefix.len(), "slug {slug}");
}

// intent: test no_free_slug
#[tokio::test]
async fn no_free_slug() {
    let app = App::new();
    let hash = hash_of(TARGET);
    for n in 4..=12 {
        app.store(&hash[..n], &format!("https://other.example/{n}"), None).await;
    }
    let r = app.post(json!({ "target": TARGET })).await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    assert_eq!(app.repo.with_prefix("").await.unwrap().len(), 9, "nothing inserted");
}

// intent: test concurrent_posts_of_the_same_target
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_posts_of_the_same_target() {
    for round in 0..20 {
        let app = App::new();
        let target = format!("https://example.com/race/{round}");
        let (a, b) = tokio::join!(
            app.post(json!({ "target": target })),
            app.post(json!({ "target": target }))
        );
        let mut statuses = [a.status, b.status];
        statuses.sort();
        assert_eq!(statuses, [StatusCode::OK, StatusCode::CREATED], "round {round}");
        assert_eq!(slug_of(&a), slug_of(&b));
        let links = app.repo.with_prefix("").await.unwrap();
        assert_eq!(links.len(), 1);
    }
}

// intent: test concurrent_posts_whose_hashes_share_a_prefix
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_posts_whose_hashes_share_a_prefix() {
    // Find two targets whose hashes share the first 4 characters (birthday search, ~2^12 tries).
    let mut seen = std::collections::HashMap::new();
    let (t1, t2) = (0u32..)
        .find_map(|i| {
            let t = format!("https://example.com/p/{i}");
            seen.insert(hash_of(&t)[..4].to_string(), t.clone()).filter(|old| *old != t).map(|old| (old, t))
        })
        .unwrap();
    for _ in 0..10 {
        let app = App::new();
        let (a, b) = tokio::join!(
            app.post(json!({ "target": t1 })),
            app.post(json!({ "target": t2 }))
        );
        assert_eq!((a.status, b.status), (StatusCode::CREATED, StatusCode::CREATED));
        assert_ne!(slug_of(&a), slug_of(&b));
    }
}

// intent: test renews_an_expired_link
#[tokio::test]
async fn renews_an_expired_link() {
    // expired but not yet soft deleted
    let app = App::new();
    let first = app.post(json!({ "target": TARGET, "expires": "2030-01-02T00:00:00Z" })).await;
    assert_eq!(first.status, StatusCode::CREATED);
    app.clock.advance(Duration::days(2));
    let r = app.post(json!({ "target": TARGET })).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(slug_of(&r), slug_of(&first));
    let stored = app.repo.get(&slug_of(&r)).await.unwrap();
    assert_eq!(stored.deleted, None);
    assert_eq!(stored.expires, Some(app.now() + Duration::days(30)));

    // soft deleted by the job
    let app = App::new();
    let first = app.post(json!({ "target": TARGET, "expires": "2030-01-02T00:00:00Z" })).await;
    app.clock.advance(Duration::days(2));
    expiry::run(&app.repo, &app.clock).await;
    assert!(app.repo.get(&slug_of(&first)).await.unwrap().deleted.is_some());
    let r = app.post(json!({ "target": TARGET })).await;
    assert_eq!(r.status, StatusCode::OK);
    let stored = app.repo.get(&slug_of(&r)).await.unwrap();
    assert_eq!(stored.deleted, None);
    assert_eq!(stored.expires, Some(app.now() + Duration::days(30)));
}

// intent: test rejects_invalid_input
#[tokio::test]
async fn rejects_invalid_input() {
    let app = App::new();
    for body in [
        json!({ "target": "not a url" }),
        json!({ "target": "ftp://example.com/a" }),
        json!({ "target": 5 }),
        json!({}),
        json!({ "target": TARGET, "expires": "tomorrow" }),
        json!({ "target": TARGET, "expires": "2020-01-01T00:00:00Z" }), // in the past
    ] {
        let r = app.post(body.clone()).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{body}");
    }
    assert_eq!(app.post_raw("{not json".into()).await.status, StatusCode::BAD_REQUEST);
    assert!(app.repo.with_prefix("").await.unwrap().is_empty(), "nothing inserted");
}

// intent: test redirects_to_the_target
#[tokio::test]
async fn redirects_to_the_target() {
    let app = App::new();
    let slug = slug_of(&app.post(json!({ "target": TARGET })).await);
    let r = app.get(&format!("/{slug}")).await;
    assert_eq!(r.status, StatusCode::FOUND);
    assert_eq!(r.headers[header::LOCATION], TARGET);
}

// intent: test get_link_info
#[tokio::test]
async fn get_link_info() {
    let app = App::new();
    let slug = slug_of(&app.post(json!({ "target": TARGET })).await);
    let r = app.get(&format!("/url?slug={slug}")).await;
    assert_eq!(r.status, StatusCode::OK);
    assert!(matches_link_response(&r.body));
    assert_eq!(r.body["target"], TARGET);
}

// intent: test unknown_expired_deleted_and_invalid_slugs
#[tokio::test]
async fn unknown_expired_deleted_and_invalid_slugs() {
    let app = App::new();
    for path in ["/abcd", "/url?slug=abcd"] {
        assert_eq!(app.get(path).await.status, StatusCode::NOT_FOUND, "{path}");
    }

    // expired, then soft deleted: 410 both times
    let slug = slug_of(&app.post(json!({ "target": TARGET, "expires": "2030-01-02T00:00:00Z" })).await);
    app.clock.advance(Duration::days(2));
    for path in [format!("/{slug}"), format!("/url?slug={slug}")] {
        assert_eq!(app.get(&path).await.status, StatusCode::GONE, "{path} expired");
    }
    expiry::run(&app.repo, &app.clock).await;
    for path in [format!("/{slug}"), format!("/url?slug={slug}")] {
        assert_eq!(app.get(&path).await.status, StatusCode::GONE, "{path} soft deleted");
    }

    // slug outside 4..=12 characters (and a missing slug)
    for path in ["/abc", "/abcdefghijklm", "/url?slug=abc", "/url"] {
        assert_eq!(app.get(path).await.status, StatusCode::BAD_REQUEST, "{path}");
    }
}

// intent: test database_unavailable
#[tokio::test]
async fn database_unavailable() {
    let app = App::new();
    let slug = slug_of(&app.post(json!({ "target": TARGET })).await);
    app.repo.set_unavailable(true);
    for path in [format!("/{slug}"), format!("/url?slug={slug}")] {
        assert_eq!(app.get(&path).await.status, StatusCode::SERVICE_UNAVAILABLE, "{path}");
    }
    let r = app.post(json!({ "target": "https://example.com/new" })).await;
    assert_eq!(r.status, StatusCode::SERVICE_UNAVAILABLE);
}

// intent: test expiry_job_soft_deletes_expired_links
#[tokio::test]
async fn expiry_job_soft_deletes_expired_links() {
    let app = App::new();
    let old = app.now() + Duration::days(1);
    let new = app.now() + Duration::days(10);
    app.store("oldd", "https://example.com/old", Some(old)).await;
    app.store("neww", "https://example.com/new", Some(new)).await;
    app.clock.advance(Duration::days(2));

    expiry::run(&app.repo, &app.clock).await;
    let deleted_at = app.repo.get("oldd").await.unwrap().deleted;
    assert_eq!(deleted_at, Some(app.now()));
    assert_eq!(app.repo.get("neww").await.unwrap().deleted, None);
    // its slug is still taken
    assert_eq!(app.repo.with_prefix("oldd").await.unwrap().len(), 1);

    // running it again changes nothing
    app.clock.advance(Duration::hours(1));
    expiry::run(&app.repo, &app.clock).await;
    assert_eq!(app.repo.get("oldd").await.unwrap().deleted, deleted_at);
}

// intent: test expiry_job_survives_unavailable
#[tokio::test]
async fn expiry_job_survives_unavailable() {
    let app = App::new();
    app.store("oldd", "https://example.com/old", Some(app.now() + Duration::days(1))).await;
    app.clock.advance(Duration::days(2));

    app.repo.set_unavailable(true);
    expiry::run(&app.repo, &app.clock).await; // logs and does not crash
    app.repo.set_unavailable(false);
    assert_eq!(app.repo.get("oldd").await.unwrap().deleted, None);

    expiry::run(&app.repo, &app.clock).await; // the next run catches up
    assert!(app.repo.get("oldd").await.unwrap().deleted.is_some());
}
