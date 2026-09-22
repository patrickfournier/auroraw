// SPDX-License-Identifier: GPL-3.0-or-later
//! WP3's "done when": a stress test with commands from several threads ends in the state a
//! sequential replay gives. Several threads hammer one engine with `SetRating`, `SetFlag`,
//! `AddKeyword` and `RemoveKeyword` on a shared pool of photos and keywords; the coordinator's
//! `Event::Applied` stream records the order it actually serialized them in (architecture §4.2,
//! §4.3). That exact order, replayed one command at a time against a second, freshly seeded
//! engine, must land on the same catalogue state.

use std::path::Path;
use std::thread;

use auroraw_catalogue::Catalogue;
use auroraw_engine::{Command, Engine, Event, EventReceiver};
use auroraw_format::sidecar::{Flag, PhotoSidecar};
use auroraw_format::state::{KeywordEntry, Vocabulary};
use auroraw_testkit::{rng, temp_dir};
use auroraw_types::{KeywordId, PhotoId, Timestamp};
use auroraw_workspace::Workspace;

const PHOTOS: usize = 20;
const KEYWORDS: usize = 5;
const WORKERS: usize = 6;
const COMMANDS_PER_WORKER: usize = 30;

fn seed_and_open(
    root: &Path,
    catalogue: &Path,
    photo_ids: &[PhotoId],
    keyword_ids: &[KeywordId],
) -> (Engine, EventReceiver) {
    let ws = Workspace::create(root, "Main").unwrap();
    let vocabulary = Vocabulary {
        updated: Timestamp::now(),
        keywords: keyword_ids
            .iter()
            .enumerate()
            .map(|(i, id)| KeywordEntry {
                id: *id,
                name: format!("K{i}"),
                parent: None,
                synonyms: Vec::new(),
                export: true,
                extra: Default::default(),
            })
            .collect(),
        extra: Default::default(),
    };
    ws.write_vocabulary(&vocabulary).unwrap();
    for id in photo_ids {
        ws.write_photo(&PhotoSidecar::new(*id)).unwrap();
    }
    let workspace_id = ws.workspace_id();
    drop(ws);
    drop(Catalogue::create(catalogue, workspace_id).unwrap());

    let (engine, events) = Engine::open(root, catalogue).unwrap();
    engine.submit_and_wait(Command::Rebuild).unwrap();
    events.drain();
    (engine, events)
}

fn plan(seed: u64, photo_ids: &[PhotoId], keyword_ids: &[KeywordId]) -> Vec<Command> {
    let mut r = rng(seed);
    (0..COMMANDS_PER_WORKER)
        .map(|_| {
            let photo_id = photo_ids[r.usize(..photo_ids.len())];
            let keyword_id = keyword_ids[r.usize(..keyword_ids.len())];
            match r.usize(..4) {
                0 => Command::SetRating {
                    photo_id,
                    rating: r.u8(0..=5),
                },
                1 => {
                    let flag = match r.usize(..3) {
                        0 => Some(Flag::Picked),
                        1 => Some(Flag::Rejected),
                        _ => None,
                    };
                    Command::SetFlag { photo_id, flag }
                }
                2 => Command::AddKeyword {
                    photo_id,
                    keyword_id,
                },
                _ => Command::RemoveKeyword {
                    photo_id,
                    keyword_id,
                },
            }
        })
        .collect()
}

#[test]
fn commands_from_several_threads_end_in_the_state_a_sequential_replay_gives() {
    let photo_ids: Vec<PhotoId> = (0..PHOTOS).map(|_| PhotoId::random()).collect();
    let keyword_ids: Vec<KeywordId> = (0..KEYWORDS).map(|_| KeywordId::random()).collect();
    let plans: Vec<Vec<Command>> = (0..WORKERS)
        .map(|w| plan(1000 + w as u64, &photo_ids, &keyword_ids))
        .collect();
    let total = WORKERS * COMMANDS_PER_WORKER;

    // Run the plans concurrently against a fresh engine, recording the order the single writer
    // actually applied them in.
    let concurrent_dir = temp_dir();
    let (concurrent, events) = seed_and_open(
        &concurrent_dir.path().join("Main"),
        &concurrent_dir.path().join("main.sqlite"),
        &photo_ids,
        &keyword_ids,
    );

    let collector = thread::spawn(move || {
        let mut applied = Vec::with_capacity(total);
        while applied.len() < total {
            match events.recv().expect("event before the channel closes") {
                Event::Applied(command) => applied.push(command),
                Event::Failed { command, error } => {
                    panic!("every command in this test always succeeds: {command:?}: {error}")
                }
                _ => {} // PhotoChanged and the like: not what this test tracks.
            }
        }
        applied
    });

    let workers: Vec<_> = plans
        .into_iter()
        .map(|commands| {
            let engine = concurrent.clone();
            thread::spawn(move || {
                for command in commands {
                    engine.submit_and_wait(command).unwrap();
                }
            })
        })
        .collect();
    for w in workers {
        w.join().unwrap();
    }
    let applied = collector.join().unwrap();
    assert_eq!(applied.len(), total);

    // Replay that exact order, sequentially, against a second engine seeded the same way.
    let sequential_dir = temp_dir();
    let (sequential, sequential_events) = seed_and_open(
        &sequential_dir.path().join("Main"),
        &sequential_dir.path().join("main.sqlite"),
        &photo_ids,
        &keyword_ids,
    );
    for command in applied {
        sequential.submit_and_wait(command).unwrap();
    }
    sequential_events.drain();

    let concurrent_catalogue = concurrent.read_catalogue().unwrap();
    let sequential_catalogue = sequential.read_catalogue().unwrap();
    for id in &photo_ids {
        let a = concurrent_catalogue
            .photo(id)
            .unwrap()
            .expect("seeded photo");
        let b = sequential_catalogue
            .photo(id)
            .unwrap()
            .expect("seeded photo");
        assert_eq!(
            (a.rating, a.effective_rating, a.flag, a.effective_flag),
            (b.rating, b.effective_rating, b.flag, b.effective_flag),
            "photo {id} diverged"
        );
    }
    for keyword_id in &keyword_ids {
        let mut a = concurrent_catalogue
            .photos_with_keywords(&[*keyword_id])
            .unwrap();
        let mut b = sequential_catalogue
            .photos_with_keywords(&[*keyword_id])
            .unwrap();
        a.sort();
        b.sort();
        assert_eq!(a, b, "keyword {keyword_id} diverged");
    }
}
