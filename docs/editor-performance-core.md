# Preparing Rune for a graphical frontend

The shared document API and optional terminal feature were introduced in four stages: shared safe saving; renderer-independent highlight roles; scalar-indexed documents with shared undo; and terminal feature gating. Existing terminal editing/rendering algorithms were retained. Etch consumes the same crate using `default-features = false`; see [embedding instructions](embedding.md).

## Measurement protocol

Apple M3 Pro/macOS, Rust 1.96.0, default locked release configuration. The baseline app and Criterion executables were frozen before editing. Each stage was built and checked, then measured serially without concurrent builds/tests. Criterion uses 20 samples, a 0.3-second warmup, and a one-second measurement target (extended where necessary). The table uses mean point estimates. Each stage also has a separate three-second native `sample` profile under sustained verified terminal editing. Profiles are not used as timing samples.

Units below are microseconds except the two rows explicitly labelled milliseconds.

| Workload | Before | Saving | Tokens | Document API | Feature gating | Final |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 256 queued plain keys, µs | 207.368 | 205.471 | 204.334 | 204.018 | 203.082 | 205.642 |
| 100 insert+redraw frames, ms | 9.116 | 9.277 | 9.457 | 9.397 | 9.372 | 9.431 |
| Six search query changes; 5.9 MB, ms | 3.162 | 3.275 | 3.244 | 3.272 | 3.340 | 3.372 |
| 50 highlighted Rust lines, cold, µs | 44.403 | 44.978 | 44.897 | 45.356 | 45.232 | 45.973 |
| 50 highlighted Rust lines, warm, µs | 7.600 | 7.693 | 7.649 | 7.653 | 7.634 | 7.763 |
| Invalidate and rehighlight 50 lines, µs | 50.903 | 54.582 | 52.864 | 53.116 | 53.204 | 53.659 |
| Save 10 MB, ms | 11.322 | 14.024 | 11.402 | 11.638 | 11.507 | 12.170 |

## Confirmation runs

The initial saving stage reported 14.024 ms against an initial 11.322 ms baseline. Work paused for alternating 30-sample, two-second confirmation captures: **11.487 and 11.476 ms before; 11.781 and 11.714 ms after**. That is about 2.3% extra time, rather than the initial 24% difference. Separate saving stack profiles remain dominated by filesystem synchronization and writes. No save protection was removed.

The feature-gating pass also prompted longer alternating checks of search, redraw, and highlight rebuilding. The order was after/before/before/after; each capture uses 30 samples, a 0.5-second warmup, and a two-second target. Ratios below compare the average of the two per-build mean estimates; they are not confidence bounds.

| Workload | Before | After | Change |
| --- | ---: | ---: | ---: |
| typing_cache/insert_100_frames | 9.296 ms | 9.363 ms | +0.7% |
| live_search_latency/type_6_chars_100000_lines | 3.180 ms | 3.284 ms | +3.3% |
| syntax_highlight/invalidate_then_rehighlight_50_lines | 53.417 µs | 52.049 µs | -2.6% |

Final shorter captures vary more: search is about 6.6% above the original baseline (roughly 0.21 ms over six query changes), and saving about 7.5%. The longer confirmations above show smaller differences. These measurements support no substantial regression in the checked paths, not a claim of identical timings or an improvement in every workload.

## Headless API

Setup and rendering are excluded from these Criterion measurements; they are engine costs, not GUI input latency. The core uses the existing rope and undo snapshot implementation.

| New document operation | Headless release |
| --- | ---: |
| insert_one | 0.557 µs |
| replace_in_million_character_line | 1.033 µs |
| undo_redo_million_character_line | 0.127 µs |

A separate native stack profile exercises headless replacement in a million-character line. It includes benchmark setup; use the Criterion figures for isolated operation costs. No terminal renderer or generic GUI bridge is on that path.

## Verified terminal replays

Frozen before/final executables run the same five PTY tasks in seeded shuffled order, with one excluded warmup and seven measured rounds: **80 verified sessions**. Every saved file must exactly match its expected bytes and have a changed modification timestamp. These timers include the task, saving, and exit; startup and settling are separate.

| Task | Before ms | Final ms |
| --- | ---: | ---: |
| type_1000 | 5.973 | 6.190 |
| save_1000 | 4.818 | 4.753 |
| find_100000 | 15.422 | 16.331 |
| longline_append | 19.485 | 20.050 |
| unicode_wrap | 6.998 | 7.195 |

## Verification and artifacts

410 default tests and 116 headless tests pass in debug and release on macOS; two explicitly live-network tests remain ignored in the default build. Clippy passes with all targets/features and separately with terminal support disabled. An external Cargo project verifies the actual sibling path dependency with default features disabled; its normal dependency graph excludes terminal, CLI/config, and updater dependencies. Linux headless tests and default release builds also pass, with final API tests in both feature configurations.

Coverage includes scalar offsets through combining marks/emoji/CRLF, directed selection undo/redo, invalid edits, randomized reference editing, bounded highlight invalidation, failed loads, typed save conflicts, explicit overwrite permission, backup preservation/failure, and existing terminal save/permission/symlink protections.

Raw Criterion samples/estimates, frozen executables, stage logs, paired confirmations, stack profiles, replay verification, and dependency trees are retained in `target/etch-core-profile/`. `measurements.json` summarizes the stage estimates. No GUI has been implemented or measured here; Windows runtime, font layout, IME, and actual graphical input-to-frame latency remain unverified.
