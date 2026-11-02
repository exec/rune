#!/usr/bin/env python3
"""Editor Bench v1: fetch, build, verify, time, and score Rune/nano/Vim. Python 3.12+."""
import argparse
from datetime import datetime, timezone
from functools import cache
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import random
import shutil
import statistics
import subprocess
import tarfile
import tempfile
import time
from urllib.request import urlopen

import editor_compare as pty

ROOT = pty.ROOT
CACHE = ROOT / "target/editor-bench"
MARKER = b"QZWVUGJKF"
SIZES = {1000: .60, 100000: .35, 1000000: .05}
# Weight %, then budgets in ms for small / medium / large files (or the fixed fixture).
METRICS = {
    "key": (20, [.5, .5, .5]),
    "burst": (10, [2, 2, 2]),
    "unicode": (10, [4]),
    "navigate": (15, [4, 4, 4]),
    "search": (15, [4, 12, 50]),
    "open": (10, [10, 20, 100]),
    "save": (10, [5, 15, 75]),
    "paste": (5, [25, 25, 25]),
    "longline": (5, [10]),
}
SOURCES = {
    "nano": ("https://www.nano-editor.org/dist/v9/nano-9.3.tar.xz",
             "a9affc52c9596ee0682bcdd4ceb3c0b235eb221d83d733dfd9b0dff45897cb0d",
             ["--disable-nls"]),
    "vim": ("https://codeload.github.com/vim/vim/tar.gz/refs/tags/v9.2.1178",
            "62b4a204676f0e2d6c11f1951ac204b1c83d22cd3f383599e7455e96cf87a68a",
            ["--with-features=huge", "--enable-gui=no", "--without-x", "--disable-nls"]),
}


def digest(path):
    with Path(path).open("rb") as file:
        return hashlib.file_digest(file, "sha256").hexdigest()


def prepare(name):
    url, checksum, options = SOURCES[name]
    patch = Path(__file__).parent / "editor-bench-patches" / f"{name}.patch"
    recipe = dict(url=url, sha256=checksum, configure=options, cflags="-O2 -g",
                  platform=platform.platform(), machine=platform.machine(),
                  cc=subprocess.check_output(["cc", "--version"], text=True).splitlines()[0],
                  patch_sha256=digest(patch) if patch.exists() else None)
    key = hashlib.sha256(json.dumps(recipe, sort_keys=True).encode()).hexdigest()[:16]
    destination = CACHE / f"{name}-{key}"
    manifest = destination / "build.json"
    binary = destination / "bin" / name
    if manifest.exists() and binary.exists():
        info = json.loads(manifest.read_text())
        if digest(binary) != info["binary_sha256"]:
            raise ValueError(f"changed cached binary: remove {destination} and retry")
        return binary, info
    downloads = CACHE / "downloads"
    downloads.mkdir(parents=True, exist_ok=True)
    archive = downloads / checksum
    if not archive.exists():
        partial = archive.with_suffix(".part")
        print(f"Downloading {name}: {url}", flush=True)
        with urlopen(url, timeout=60) as response, partial.open("wb") as output:
            shutil.copyfileobj(response, output)
        if digest(partial) != checksum:
            partial.unlink()
            raise ValueError(f"{name}: download checksum mismatch")
        partial.replace(archive)
    if digest(archive) != checksum:
        raise ValueError(f"{name}: cached source checksum mismatch: {archive}")
    began = time.perf_counter()
    with tempfile.TemporaryDirectory(prefix=f"{name}-build-", dir=CACHE) as scratch:
        source = Path(scratch)
        with tarfile.open(archive) as package:
            package.extractall(source, filter="data")
        source = next(source.iterdir())
        destination.mkdir(parents=True, exist_ok=True)
        with (destination / "build.log").open("w") as log:
            commands = []
            if patch.exists():
                commands.append(["patch", "--batch", "-p1", "-i", str(patch.resolve())])
            commands += [["./configure", f"--prefix={destination}", *options],
                         ["make", f"-j{min(os.cpu_count() or 1, 8)}"], ["make", "install"]]
            print(f"Building {name}; log: {destination / 'build.log'}", flush=True)
            for command in commands:
                subprocess.run(command, cwd=source, env=dict(os.environ, CFLAGS="-O2 -g"),
                               stdout=log, stderr=subprocess.STDOUT, check=True)
    info = dict(recipe, build_seconds=time.perf_counter() - began, binary_sha256=digest(binary))
    manifest.write_text(json.dumps(info, indent=2) + "\n")
    return binary, info


@cache
def body(count):
    return pty.READY + b"\n" + b"".join(
        f"record {i:08d}  alpha beta gamma delta epsilon 0123456789\n".encode()
        for i in range(count)) + b"quartz target at EOF\n"


def fixture(action, count):
    data = body(count)
    text = MARKER
    position = 0
    if action == "key":
        text = b"~"
    elif action == "burst":
        text = b"x" * (256 - len(MARKER)) + MARKER
    elif action == "unicode":
        data = pty.READY + b"\n" + ("界e\u0301 ".encode() * 50 + b"\n") * 500
        text = "界".encode() * (256 - len(MARKER)) + MARKER
    elif action == "longline":
        data = pty.READY + b"\n" + b"a" * 1_000_000 + b"\n"
        position = len(data) - 1
        text = b"x" * (256 - len(MARKER)) + MARKER
    elif action == "paste":
        line = b"pasted text 0123456789\n"
        text = (line * (1048576 // len(line) + 1))[:1048576 - len(MARKER) - 1] + MARKER + b"\n"
    elif action == "navigate":
        position = sum(map(len, data.splitlines(keepends=True)[:256]))
    elif action == "search":
        position = data.index(b"quartz")
    elif action == "save":
        text = b""
    expected = data[:position] + text + data[position:]
    return pty.Case(f"{action}_{count}", action, data, expected, action == "unicode"), text


def payload(editor, action, text):
    prefix = b""
    if action == "navigate":
        prefix = {"rune": b"\x1b[B", "nano": b"\x0e", "vim": b"j"}[editor] * 256
    elif action == "search":
        prefix = {"rune": b"\x06", "nano": b"\x17", "vim": b"/"}[editor] + b"quartz\r"
    elif action == "longline":
        prefix = {"rune": b"\x1b[B\x1b[F", "nano": b"\x0e\x05", "vim": b"j$"}[editor]
    if editor == "vim":
        prefix += b"a" if action == "longline" else b"i"
    if action == "paste":
        text = b"\x1b[200~" + text + b"\x1b[201~"
    return prefix + text


def measure(editor, binary, action, count, output):
    case, text = fixture(action, count)
    with tempfile.TemporaryDirectory(prefix="editor-bench-") as scratch:
        directory = Path(scratch)
        path = directory / "fixture.txt"
        path.write_bytes(case.data)
        mtime = path.stat().st_mtime_ns
        command = pty.launch(editor, binary, case, path, directory)
        session = pty.Session(command, 120, 40, 30)
        try:
            session.observe(b"", pty.READY)
            startup = (time.perf_counter() - session.started) * 1000
            session.settle(.03)
            observed = {} if action == "save" else session.observe(
                payload(editor, action, text), b"~" if action == "key" else MARKER)
            finish = {"rune": b"\x13\x11", "nano": b"\x13\x18", "vim": b"\x1c\x0e:wq\r"}[editor]
            result = session.finish(finish)
            if path.read_bytes() != case.expected or path.stat().st_mtime_ns == mtime:
                raise ValueError(f"{editor}/{case.name}: saved bytes or save timestamp differ")
            return dict(editor=editor, action=action, count=count, verified=True, command=command,
                        fixture_bytes=len(case.data), fixture_sha256=hashlib.sha256(case.data).hexdigest(),
                        startup_ms=startup, **result,
                        **(observed or dict(task_ms=result["task_save_exit_ms"])))
        except BaseException:
            output.with_suffix(f".{editor}-{case.name}.failure.ansi").write_bytes(session.tail)
            raise
        finally:
            session.close()


def summarize(samples, runs):
    rows = [s for s in samples if not s["warmup"]]
    if not rows or not all(s["verified"] for s in samples):
        raise ValueError("only a complete verified suite can be scored")
    result = {}
    for editor in ["rune", "nano", "vim"]:
        components = []
        for metric, (weight, budgets) in METRICS.items():
            sizes = list(SIZES) if len(budgets) == 3 else [0]
            for count, budget in zip(sizes, budgets):
                action = "save" if metric == "open" else metric
                field = "startup_ms" if metric == "open" else "task_ms"
                group = [s for s in rows if (s["editor"], s["action"], s["count"]) == (editor, action, count)]
                values = [s[field] for s in group]
                if (len(values) != runs or len({s["iteration"] for s in group}) != runs
                        or any(not math.isfinite(v) or v <= 0 for v in values)):
                    raise ValueError(f"missing/invalid timing: {editor}/{metric}/{count}")
                median = statistics.median(values)
                components.append(dict(metric=metric, count=count, weight=weight * SIZES.get(count, 1),
                                       budget_ms=budget, median_ms=median,
                                       p95_ms=sorted(values)[math.ceil(.95 * len(values)) - 1],
                                       samples=len(values), ratio=min(1, budget / median)))
        score = 100 * math.exp(sum(c["weight"] / 100 * math.log(c["ratio"]) for c in components))
        result[editor] = dict(score=score, components=components)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runs", type=int, default=15)
    parser.add_argument("--warmups", type=int, default=2)
    parser.add_argument("--rune", type=Path, help="use this Rune binary instead of building the workspace")
    parser.add_argument("--output", type=Path, default=CACHE / "latest.json")
    args = parser.parse_args()
    if args.runs < 1 or args.warmups < 0:
        parser.error("need positive runs and nonnegative warmups")
    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    report = dict(status="preparing", metadata=dict(suite="editor-bench-v1", weights_budgets=METRICS,
                  sizes=SIZES, runs=args.runs, warmups=args.warmups, seed=2026,
                  platform=platform.platform(), machine=platform.machine(),
                  timestamp=datetime.now(timezone.utc).isoformat(),
                  cpu=subprocess.check_output(["sysctl", "-n", "machdep.cpu.brand_string"], text=True).strip()
                  if platform.system() == "Darwin" else next((line.split(":", 1)[1].strip() for line in
                      Path("/proc/cpuinfo").read_text().splitlines() if line.startswith("model name")), platform.machine()),
                  resource_scope="whole child lifetime, including Python before exec; resources are unscored",
                  driver_sha256=digest(__file__), pty_sha256=digest(pty.__file__)), samples=[], scores=None)
    try:
        binaries, builds = {}, {}
        for editor in ["nano", "vim"]:
            binaries[editor], builds[editor] = prepare(editor)
        if args.rune is None:
            subprocess.run(["cargo", "build", "--locked", "--release"], cwd=ROOT, check=True)
        binaries["rune"] = (args.rune or ROOT / "target/release/rune").resolve()
        report["metadata"]["builds"] = builds
        report["metadata"]["editors"] = {name: dict(path=str(path), sha256=digest(path),
            version=subprocess.check_output([str(path), "--version"], text=True).strip()) for name, path in binaries.items()}
        report["metadata"]["rustc"] = subprocess.check_output(["rustc", "--version"], text=True).strip() if not args.rune else None
        report["metadata"]["source_hashes"] = {str(p.relative_to(ROOT)): digest(p) for p in
            [ROOT / "Cargo.toml", ROOT / "Cargo.lock", *sorted((ROOT / "src").glob("*.rs"))]}
        report["status"] = "running"
        jobs = [(editor, action, count) for editor in binaries for action in
                ["key", "burst", "navigate", "search", "save", "paste"] for count in SIZES]
        jobs += [(editor, action, 0) for editor in binaries for action in ["unicode", "longline"]]
        rng = random.Random(2026)
        for iteration in range(args.runs + args.warmups):
            rng.shuffle(jobs)
            for editor, action, count in jobs:
                sample = measure(editor, binaries[editor], action, count, output)
                report["samples"].append(dict(iteration=iteration, warmup=iteration < args.warmups, **sample))
            output.write_text(json.dumps(report, indent=2) + "\n")
            print(f"Round {iteration + 1}/{args.runs + args.warmups}", flush=True)
        report["scores"] = summarize(report["samples"], args.runs)
        report["status"] = "complete"
        for editor, result in sorted(report["scores"].items(), key=lambda pair: -pair[1]["score"]):
            print(f"{editor}: {result['score']:.2f}/100")
        print("Metric | Lines | Weight % | Budget ms | Rune ms | nano ms | Vim ms")
        print("--- | ---: | ---: | ---: | ---: | ---: | ---:")
        for index, component in enumerate(report["scores"]["rune"]["components"]):
            values = [report["scores"][e]["components"][index]["median_ms"] for e in ["rune", "nano", "vim"]]
            print(f"{component['metric']} | {component['count'] or 'fixed'} | {component['weight']:.2f} | "
                  f"{component['budget_ms']} | " + " | ".join(f"{v:.3f}" for v in values))
        print(f"Verified sessions: {len(report['samples'])}; results: {output}")
    except BaseException as error:
        report.update(status="failed", error=str(error), scores=None)
        raise
    finally:
        output.write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
