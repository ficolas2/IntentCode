// intentcode: ../../intent/expiry.icode
// spec-hash: 7f681ba1267c9bec  (sha256 prefix of: expiry.icode, repository.icode, utils.icode, types.icode)
// generated: claude-sonnet-5-5, by hand-driven compile, not reproducible
use std::time::Duration;

use crate::repository::Repository;
use crate::utils::Clock;

// intent: ExpiryJob::run
/// One run: `@Repository.soft_delete_expired(now)`.
/// On `Unavailable`, log it and skip this run, the next run catches up.
pub async fn run(repo: &Repository, clock: &Clock) {
    if let Err(e) = repo.soft_delete_expired(clock.now()).await {
        eprintln!("expiry job: skipped this run: {e:?}");
    }
}

// intent: ExpiryJob
/// Runs in the background, every 5s, independent of requests.
pub fn spawn(repo: Repository, clock: Clock) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            run(&repo, &clock).await;
        }
    })
}
