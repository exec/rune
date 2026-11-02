# Ryzen 9 7900X comparison

Measured November 2, 2026 UTC against the current unreleased Rune 1.6.0 workspace.

The Ryzen 9 7900X machine (`dylan@10.0.0.18`, fishbowl) has 12 cores / 24 threads, about 96 GB of RAM, CachyOS Linux 7.3.1, and an NVMe-backed Btrfs home filesystem with `zstd:1` compression enabled. CPU policy remained `powersave` with AMD P-state active and `balance_performance`; no system tuning was applied. The reference is Apple M3 Pro/macOS 27.0.1.

Both builds use Rust 1.96.0 and the same hashed Rust source files, Cargo.lock, default release settings, and Python harness. GNU nano is 9.3 on both machines; Vim is 9.2.1178 on Linux and Apple Vim 9.1 on macOS. These are whole-system comparisons: CPU, operating system, filesystem, storage, and editor builds can all contribute.

## Method

The [existing PTY harness](editor-performance.md) runs locally on each host at 120×40. SSH only starts the remote run; editing traffic stays inside the remote PTY. Personal configuration/plugins and update checks are excluded. All editors run serially in seeded shuffled order: one excluded warmup and seven measured rounds, 14 workloads × three editors × eight rounds = **336 verified sessions per host**, 672 total.

Every saved file must exactly match its expected bytes and have a changed modification timestamp. These are warm-cache fixtures. Times below are medians in milliseconds. Task time starts after startup and a fixed settling interval, and includes editing, save, and exit; it does not measure individual key-to-screen latency. Search uses a single literal `@`, not six successive incremental query changes. Rune retains atomic saving and external-change verification.

The first Linux calibration showed an artificial approximately 100 ms Vim exit wait. The PTY does not answer terminal queries; Vim waits for pending replies in [stoptermcap](https://github.com/vim/vim/blob/v9.2.1178/src/term.c#L4155). The harness already disabled several queries; it now also clears `t_RF` and `t_RB`. A save-only calibration fell from 100.965 ms to 0.591 ms. Both final captures use the identical corrected harness. Preliminary captures are excluded.

Linux `wait4` peak RSS includes the Python child’s memory before exec. The all-workload capture has an inherited approximately 279 MiB floor for every editor, so **Linux RSS is excluded from all memory comparisons**. Raw samples retain that counter for provenance; it is not editor-only memory.

## Linux task + save + exit

| Workload | Rune ms | nano ms | Vim ms |
| --- | ---: | ---: | ---: |
| Type 256 ASCII characters; 59 KB | 0.913 | 0.772 | 1.158 |
| Move down 256 lines; 59 KB | 0.789 | 1.388 | 2.711 |
| Find near EOF; 59 KB | 0.970 | 0.687 | 0.865 |
| Save unchanged; 59 KB | 0.449 | 0.325 | 0.640 |
| Type 256 ASCII characters; 5.9 MB | 4.628 | 5.679 | 6.496 |
| Move down 256 lines; 5.9 MB | 4.212 | 6.225 | 7.746 |
| Find near EOF; 5.9 MB | 4.493 | 20.277 | 7.958 |
| Save unchanged; 5.9 MB | 3.727 | 5.188 | 5.849 |
| Type 256 ASCII characters; 59 MB | 39.484 | 47.723 | 59.027 |
| Move down 256 lines; 59 MB | 39.299 | 48.399 | 60.862 |
| Find near EOF; 59 MB | 42.994 | 200.591 | 83.047 |
| Save unchanged; 59 MB | 38.430 | 47.809 | 58.744 |
| Append 256 characters to a million-character line | 9.717 | 149.676 | 21.168 |
| Type 256 CJK characters with wrapping | 1.251 | 1.671 | 4.068 |

Rune has the lowest median in **11/14 workloads**. Nano remains ahead in small-file typing, search, and saving; these differences are fractions of a millisecond and seven rounds do not establish tail behavior. For the 59 MB search workflow, nano takes 4.67× as long as Rune and Vim takes 1.93× as long. On the million-character line those ratios are 15.40× and 2.18×.

## Rune across machines

| Workload | M3 Pro ms | 7900X ms | M3 time / 7900X time |
| --- | ---: | ---: | ---: |
| Type 256 ASCII characters; 59 KB | 5.457 | 0.913 | 5.98× |
| Save unchanged; 59 KB | 5.283 | 0.449 | 11.76× |
| Type 256 ASCII characters; 5.9 MB | 13.017 | 4.628 | 2.81× |
| Find near EOF; 5.9 MB | 12.320 | 4.493 | 2.74× |
| Type 256 ASCII characters; 59 MB | 57.389 | 39.484 | 1.45× |
| Find near EOF; 59 MB | 58.627 | 42.994 | 1.36× |
| Save unchanged; 59 MB | 57.884 | 38.430 | 1.51× |
| Append 256 characters to a million-character line | 15.844 | 9.717 | 1.63× |
| Type 256 CJK characters with wrapping | 7.382 | 1.251 | 5.90× |

Large-file opening moves in the opposite direction. First-fixture output for Rune’s 59 MB typing case arrives in **30.231 ms on M3 Pro versus 46.399 ms on 7900X**. Linux Vim is 47.485 ms and nano 132.199 ms. This is a first-paint lower bound, not full startup completion. Adding the separate startup/task medians gives roughly 87.6 ms on M3 Pro and 85.9 ms on 7900X for the large-file typing workflow, before the excluded settling interval; faster post-startup work does not imply a proportionately faster complete open/edit/save session.

## Reproduce and artifacts

On fishbowl, the isolated setup is `~/rune-bench/7900x` (a symlink to `7900x-20261102T041228Z`). It contains the source snapshot, private Rust toolchain, native release binary, version/source manifests, benchmark driver, and results. Existing editor installations, shell startup files, and CPU settings were not changed.

```sh
ssh dylan@10.0.0.18
cd ~/rune-bench/7900x
./run-comparison.sh --output target/editor-compare/recheck.json
# More samples, or one workload:
./run-comparison.sh --runs 15 --case find_1000000 --output target/editor-compare/search-recheck.json
```

Local raw captures and provenance are in `target/editor-compare/7900x-pass/`: `7900x.json`, `apple.json`, `machine.json`, `source-manifest.json`, `source.tar.gz`, and summary/check logs. The final harness SHA-256 is `f49b2d9ed197b1aa61e08df6345ea47ddd65730f76871c3f5f661ca8252c3afb`. The snapshot records the original Git revision and dirty state because the remote copy deliberately has no Git history.

The native locked release build and all six harness self-checks pass on Linux; all six harness self-checks also pass on macOS. This setup changes no Rust runtime code. No graphical terminal rendering, cold-storage tests, native stack profiles, tail-latency claims, or CPU/RAM-only attribution are included.
