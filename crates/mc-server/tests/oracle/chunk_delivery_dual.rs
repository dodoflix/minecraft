//! How fast a joining player receives chunks, on the real vanilla server and on `mc-server` with the same seed and
//! settings: the number of chunks the bot holds over time after its login, and the structure of the chunk batches
//! (`chunk_batch_start` .. `chunk_batch_finished` sizes). The pace is not exact (threads, load), so the curves are
//! compared by the time each server needs to deliver a number of chunks, with a tolerance.
use mc_testbot::Bot;
use mc_testbot::join::BotOptions;
use mc_testkit::Server;
use std::time::{Duration, Instant};

/// Largest batch the client asks for (`ChunkBatchSizeCalculator` caps the rate at 64 chunks per tick).
const MAX_BATCH: i32 = 64;

/// What a bot saw of the chunk stream after its login packet.
struct Arrival {
    /// `(ms since the login packet, chunks held)` every 100 ms.
    curve: Vec<(u64, u64)>,
    /// Size of each `chunk_batch_finished`, in order.
    batches: Vec<i32>,
    /// Chunks held when the batches were read.
    held: u64,
}

impl Arrival {
    /// Milliseconds after the login until the bot held `chunks`.
    fn reached(&self, chunks: u64) -> Option<u64> {
        self.curve.iter().find(|c| c.1 >= chunks).map(|c| c.0)
    }
}

async fn arrival(server: &Server, view_distance: i8, window: Duration) -> Arrival {
    let mut options = BotOptions::new("127.0.0.1", server.port, "Bot");
    options.client_information.view_distance = view_distance;
    options.timeout = Duration::from_secs(120);
    let bot = Bot::connect(options).await.expect("connect");
    bot.wait_joined(Duration::from_secs(60))
        .await
        .expect("login packet");
    let t0 = Instant::now();
    let mut curve = Vec::new();
    while t0.elapsed() < window {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let held = bot.with_state(|s| s.chunks_received);
        curve.push((t0.elapsed().as_millis() as u64, held));
    }
    let (held, batches) = bot.with_state(|s| (s.chunks_received, s.chunk_batches.clone()));
    bot.disconnect().await;
    Arrival {
        curve,
        batches,
        held,
    }
}

fn describe(label: &str, a: &Arrival) {
    let at = |ms: u64| a.curve.iter().rfind(|c| c.0 <= ms).map_or(0, |c| c.1);
    println!(
        "{label}: chunks after 1s {} 2s {} 4s {} 8s {} | {} batches, first sizes {:?}",
        at(1000),
        at(2000),
        at(4000),
        at(8000),
        a.batches.len(),
        &a.batches[..a.batches.len().min(12)]
    );
}

/// Ours delivers `target` chunks within `factor` times vanilla's time (plus `slack_ms`), and the batches have the same
/// structure: the first batch is the same size, none is larger than the client's cap, and the batches
/// never account for more chunks than arrived.
async fn compare(props: &[&str], target: u64, factor: f64, slack_ms: u64) {
    let window = Duration::from_secs(20);
    let vanilla = mc_testbot::harness::vanilla(7, "survival", props)
        .start()
        .expect("start vanilla");
    let theirs = arrival(&vanilla, 12, window).await;
    vanilla.stop();
    drop(vanilla);
    let ours = super::common::ours(7, "survival", props)
        .start()
        .expect("start our server");
    let mine = arrival(&ours, 12, window).await;
    describe("vanilla", &theirs);
    describe("ours   ", &mine);

    let vanilla_ms = theirs
        .reached(target)
        .unwrap_or_else(|| panic!("vanilla held {} chunks, never {target}", theirs.held));
    let our_ms = mine
        .reached(target)
        .unwrap_or_else(|| panic!("ours held {} chunks, never {target}", mine.held));
    let limit = (vanilla_ms as f64 * factor) as u64 + slack_ms;
    assert!(
        our_ms <= limit,
        "{target} chunks after {our_ms} ms on ours, {vanilla_ms} ms on vanilla (limit {limit} ms)"
    );
    assert_eq!(
        mine.batches.first(),
        theirs.batches.first(),
        "size of the first chunk batch"
    );
    for (label, a) in [("vanilla", &theirs), ("ours", &mine)] {
        assert!(
            a.batches.iter().all(|b| (1..=MAX_BATCH).contains(b)),
            "{label}: batch sizes {:?}",
            a.batches
        );
        let sent: i64 = a.batches.iter().map(|b| i64::from(*b)).sum();
        assert!(
            sent <= a.held as i64,
            "{label}: batches account for {sent} chunks, {} arrived",
            a.held
        );
    }
}

/// A flat world needs almost no generation: the whole 12-chunk view (637 chunks) is paced by the tick loop, the
/// chunk sender and the network.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn flat_world_chunk_arrival_matches_vanilla() {
    compare(
        &[
            "level-type=minecraft:flat",
            "view-distance=12",
            "generate-structures=false",
        ],
        600,
        2.0,
        1500,
    )
    .await;
}

/// The default world is paced by terrain generation as well.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn generated_world_chunk_arrival_matches_vanilla() {
    compare(
        &["view-distance=12", "generate-structures=false"],
        150,
        2.5,
        3000,
    )
    .await;
}
