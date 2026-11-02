#!/usr/bin/env python3
"""Compare real terminal editor sessions; use --help for benchmark/profile commands."""
import argparse
import errno
import hashlib
import json
import os
from pathlib import Path
import platform
import pty
import random
import re
import select
import shutil
import signal
import statistics
import subprocess
import sys
import tempfile
import termios
import time
from dataclasses import dataclass
from datetime import datetime, timezone

ROOT = Path(__file__).resolve().parents[1]
READY = b"BENCH_READY"
INSERT = b"x" * 256
RUNE_CONFIG = """mouse_enabled = false
show_line_numbers = false
tab_width = 4
word_wrap = {wrap}
auto_indent = false
show_whitespace = false
constant_cursor_position = false
backup_on_save = false
check_for_updates = false
"""


@dataclass
class Case:
    name: str
    action: str
    data: bytes
    expected: bytes
    wrap: bool = False


def cases(line_counts):
    result = []
    for count in line_counts:
        data = READY + b"\n" + b"".join(
            f"record {i:08d}  alpha beta gamma delta epsilon 0123456789\n".encode()
            for i in range(count)
        ) + b"@ unique target at EOF\n"
        position = sum(map(len, data.splitlines(keepends=True)[:256]))
        result.extend([
            Case(f"type_{count}", "type", data, INSERT + data),
            Case(f"navigate_{count}", "navigate", data, data[:position] + b"X" + data[position:]),
            Case(f"find_{count}", "find", data, data.replace(b"@", b"X@")),
            Case(f"save_{count}", "save", data, data),
        ])
    data = READY + b"\n" + b"a" * 1_000_000 + b"\n"
    result.append(Case("longline_append", "longline", data, data[:-1] + INSERT + b"\n"))
    data = READY + b"\n" + ("界e\u0301 ".encode() * 50 + b"\n") * 500
    result.append(Case("unicode_wrap", "unicode", data, "界".encode() * 256 + data, True))
    return result


def keys(editor, case, cycle=False):
    text = "界".encode() * 256 if case.action == "unicode" else INSERT
    if editor == "vim":
        normal = b"\x1c\x0e"  # Ctrl-\ Ctrl-N avoids Escape's mapping timeout.
        start = b"gg0"
        if case.action in ("type", "unicode"):
            edit = b"i" + text + (b"\x7f" * 256 if cycle else b"") + normal
        elif case.action == "navigate":
            edit = b"j" * 256 + (b"k" * 256 if cycle else b"iX" + normal)
        elif case.action == "find":
            edit = b"/@\r" + (start if cycle else b"iX" + normal)
        elif case.action == "longline":
            edit = b"jA" + text + (b"\x7f" * 256 if cycle else b"") + normal
            if cycle:
                edit += start
        else:
            edit = b":w\r" if cycle else b""
        return edit if cycle else edit + b":wq\r"
    down, up, end, start = (
        (b"\x1b[B", b"\x1b[A", b"\x1b[F", b"\x1b[1;5H") if editor == "rune"
        else (b"\x0e", b"\x10", b"\x05", b"\x1b\\")
    )
    if case.action in ("type", "unicode"):
        edit = text + (b"\x7f" * 256 if cycle else b"")
    elif case.action == "navigate":
        edit = down * 256 + (up * 256 if cycle else b"X")
    elif case.action == "find":
        edit = (b"\x06" if editor == "rune" else b"\x17") + b"@\r"
        edit += start if cycle else b"X"
    elif case.action == "longline":
        edit = down + end + text + (b"\x7f" * 256 + start if cycle else b"")
    else:
        edit = b"\x13" if cycle else b""
    return edit if cycle else edit + b"\x13" + (b"\x11" if editor == "rune" else b"\x18")


def launch(editor, binary, case, fixture, directory):
    if editor == "rune":
        config = directory / "rune.toml"
        config.write_text(RUNE_CONFIG.format(wrap=str(case.wrap).lower()))
        return [str(binary), "--config", str(config), str(fixture)]
    if editor == "nano":
        return [str(binary), "--ignorercfiles", "--nowrap", "--tabsize=4",
                *(["--softwrap"] if case.wrap else []), str(fixture)]
    settings = (
        "set nocompatible nomodeline noswapfile nobackup nowritebackup noundofile "
        "noautoindent nosmartindent noshowmatch nonumber norelativenumber nolist "
        "textwidth=0 tabstop=4 backspace=indent,eol,start ttimeout ttimeoutlen=10 "
        "t_RV= t_u7= t_RK= t_RF= t_RB= " + ("wrap" if case.wrap else "nowrap")
    )
    return [str(binary), "-u", "NONE", "-U", "NONE", "-i", "NONE", "--noplugin",
            "-n", "-N", "--cmd", settings, str(fixture)]


class Session:
    def __init__(self, command, width, height, timeout):
        self.command = command
        self.timeout = timeout
        self.tail = bytearray()
        self.output_bytes = 0
        self.reaped = False
        self.ready_seen = False
        self.sync_active = False
        self.sync_tail = b""
        self.started = time.perf_counter()
        self.pid, self.fd = pty.fork()
        if self.pid == 0:
            try:
                termios.tcsetwinsize(0, (height, width))
                env = os.environ.copy()
                env.update(TERM="xterm-256color", LC_ALL="en_US.UTF-8")
                os.execve(command[0], command, env)
            except BaseException as error:
                os.write(2, str(error).encode())
                os._exit(127)
        os.set_blocking(self.fd, False)

    def read(self):
        try:
            chunk = os.read(self.fd, 65536)
        except OSError as error:
            if error.errno == errno.EAGAIN:
                return True
            if error.errno != errno.EIO:
                raise
            chunk = b""
        self.output_bytes += len(chunk)
        sync = self.sync_tail + chunk
        for match in re.finditer(rb"\x1b\[\?2026([hl])", sync):
            self.sync_active = match[1] == b"h"
        self.sync_tail = sync[-7:]
        self.tail.extend(chunk)
        self.ready_seen |= READY in self.tail
        del self.tail[:-32768]
        return bool(chunk)

    def observe(self, payload, marker):
        """Time input through marker receipt and completion of any synchronized update."""
        self.tail.clear()
        outgoing = memoryview(payload)
        began = time.perf_counter()
        initial_bytes = self.output_bytes
        while time.perf_counter() - began < self.timeout:
            readable, writable, _ = select.select(
                [self.fd], [self.fd] if outgoing else [], [], 0.05
            )
            if writable:
                try:
                    outgoing = outgoing[os.write(self.fd, outgoing[:4096]):]
                except BlockingIOError:
                    pass
            if readable:
                if not self.read():
                    raise RuntimeError("editor exited before rendering the marker")
                if not outgoing and marker in self.tail and not self.sync_active:
                    return dict(task_ms=(time.perf_counter() - began) * 1000,
                                task_output_bytes=self.output_bytes - initial_bytes)
        raise TimeoutError("completed marker frame did not arrive")

    def ready(self):
        deadline = self.started + self.timeout
        while time.perf_counter() < deadline:
            if select.select([self.fd], [], [], 0.05)[0]:
                if not self.read():
                    raise RuntimeError("editor exited before showing the fixture")
                if self.ready_seen:
                    return (time.perf_counter() - self.started) * 1000
        raise TimeoutError("fixture did not appear before the startup deadline")

    def settle(self, seconds):
        deadline = time.perf_counter() + seconds
        while (remaining := deadline - time.perf_counter()) > 0:
            if select.select([self.fd], [], [], remaining)[0] and not self.read():
                raise RuntimeError("editor exited during startup settling")

    def finish(self, payload, cycle=b"", seconds=0):
        began = time.perf_counter()
        deadline = began + self.timeout + seconds
        repeat_until = began + seconds
        outgoing = memoryview(cycle or payload)
        final_sent = not cycle
        while time.perf_counter() < deadline:
            if not outgoing and not final_sent:
                repeating = time.perf_counter() < repeat_until
                outgoing = memoryview(cycle if repeating else payload)
                final_sent = not repeating
            readable, writable, _ = select.select(
                [self.fd], [self.fd] if outgoing else [], [], 0.05
            )
            if writable:
                try:
                    outgoing = outgoing[os.write(self.fd, outgoing[:4096]):]
                except BlockingIOError:
                    pass
            if readable and not self.read():
                _, status, usage = os.wait4(self.pid, 0)
                self.reaped = True
                elapsed = (time.perf_counter() - began) * 1000
                if os.waitstatus_to_exitcode(status) != 0:
                    raise RuntimeError(f"editor exit status {status}")
                if outgoing or not final_sent:
                    raise RuntimeError("editor exited before all task input was sent")
                rss_bytes = usage.ru_maxrss * (1 if sys.platform == "darwin" else 1024)
                return dict(task_save_exit_ms=elapsed,
                            child_cpu_ms=(usage.ru_utime + usage.ru_stime) * 1000,
                            peak_rss_mib=rss_bytes / 1024**2, output_bytes=self.output_bytes)
        raise TimeoutError("editor did not finish the task before its deadline")

    def close(self):
        if not self.reaped:
            try:
                os.kill(self.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            os.waitpid(self.pid, 0)
        os.close(self.fd)


def run(editor, binary, case, args, profile=None):
    with tempfile.TemporaryDirectory(prefix="rune-editor-compare-") as scratch:
        directory = Path(scratch)
        fixture = directory / "fixture.txt"
        fixture.write_bytes(case.data)
        previous_mtime = fixture.stat().st_mtime_ns
        command = launch(editor, binary, case, fixture, directory)
        session = Session(command, args.width, args.height, args.timeout)
        profiler = None
        try:
            startup = session.ready()
            session.settle(args.settle_ms / 1000)
            if profile:
                if sys.platform == "darwin":
                    profiler_command = ["sample", str(session.pid), str(args.profile_seconds),
                                        "1", "-file", str(profile)]
                else:
                    profiler_command = ["perf", "record", "-g", "-p", str(session.pid),
                                        "-o", str(profile), "--", "sleep", str(args.profile_seconds)]
                profiler = subprocess.Popen(profiler_command, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            result = session.finish(keys(editor, case), keys(editor, case, True) if profile else b"",
                                    args.profile_seconds if profile else 0)
            if profiler:
                _, stderr = profiler.communicate(timeout=10)
                if profiler.returncode:
                    raise RuntimeError(f"profiler failed: {stderr.decode(errors='replace')}")
            actual = fixture.read_bytes()
            if actual != case.expected:
                mismatch = next((i for i, (a, b) in enumerate(zip(actual, case.expected)) if a != b),
                                min(len(actual), len(case.expected)))
                raise RuntimeError(f"saved text differs at byte {mismatch}: got {len(actual)} bytes, expected {len(case.expected)}")
            if fixture.stat().st_mtime_ns == previous_mtime:
                raise RuntimeError("the save command did not write the file")
            return dict(editor=editor, case=case.name, startup_first_fixture_ms=startup,
                        fixture_bytes=len(case.data), fixture_sha256=hashlib.sha256(case.data).hexdigest(),
                        verified=True, command=command, **result)
        except BaseException:
            log = args.output.with_suffix(f".{editor}-{case.name}.failure.ansi")
            log.write_bytes(session.tail)
            print(f"Terminal failure log: {log}", file=sys.stderr)
            raise
        finally:
            session.close()
            if profiler and profiler.poll() is None:
                profiler.kill()
                profiler.communicate()


def editor_binaries(args):
    binaries = {"rune": args.rune, "nano": args.nano, "vim": args.vim}
    result = {}
    for name, candidate in binaries.items():
        if args.profile and name != args.profile:
            continue
        resolved = shutil.which(str(candidate))
        if not resolved:
            raise ValueError(f"{name} executable not found: {candidate}")
        binary = Path(resolved).resolve()
        if name == "nano" and binary.name == "pico":
            raise ValueError("This nano is Pico. Install GNU nano and pass --nano /path/to/nano.")
        version = subprocess.run([str(binary), "--version"], stdin=subprocess.DEVNULL,
                                 capture_output=True, text=True, timeout=3, check=True).stdout
        if name == "nano" and "GNU nano" not in version:
            raise ValueError("--nano must point to GNU nano")
        result[name] = dict(path=binary, version=version.strip(),
                            sha256=hashlib.sha256(binary.read_bytes()).hexdigest())
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rune", default=str(ROOT / "target/release/rune"))
    parser.add_argument("--nano", default=shutil.which("nano") or "nano")
    parser.add_argument("--vim", default=shutil.which("vim") or "vim")
    parser.add_argument("--lines", nargs="+", type=int, default=[1000, 100000])
    parser.add_argument("--runs", type=int, default=7)
    parser.add_argument("--warmups", type=int, default=1)
    parser.add_argument("--case", action="append", help="case name to run (repeatable)")
    parser.add_argument("--width", type=int, default=120)
    parser.add_argument("--height", type=int, default=40)
    parser.add_argument("--settle-ms", type=float, default=30)
    parser.add_argument("--timeout", type=float, default=30)
    parser.add_argument("--seed", type=int, default=2026)
    parser.add_argument("--output", type=Path, default=ROOT / "target/editor-compare/latest.json")
    parser.add_argument("--profile", choices=["rune", "nano", "vim"])
    parser.add_argument("--profile-seconds", type=int, default=5)
    args = parser.parse_args()
    if min(args.lines) < 256 or args.runs < 1 or args.warmups < 0 or args.settle_ms < 0 or args.timeout <= 0 or args.profile_seconds < 1:
        parser.error("need >=256 lines, >=1 run, nonnegative warmups/settling, positive timeouts")
    if not (20 <= args.width <= 65535 and 6 <= args.height <= 65535):
        parser.error("terminal must be at least 20 columns by 6 rows, at most 65535 each")
    selected = [case for case in cases(args.lines) if not args.case or case.name in args.case]
    if not selected or (args.case and set(args.case) - {case.name for case in selected}):
        parser.error("unknown case; choose type_N, navigate_N, find_N, save_N, longline_append, unicode_wrap")
    if args.profile and len(selected) != 1:
        parser.error("--profile needs exactly one --case")
    args.output = args.output.resolve()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    binaries = editor_binaries(args)
    cpu = subprocess.check_output(["sysctl", "-n", "machdep.cpu.brand_string"], text=True).strip() if sys.platform == "darwin" else platform.processor()
    revision = subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT, capture_output=True, text=True).stdout.strip()
    dirty = subprocess.run(["git", "status", "--porcelain"], cwd=ROOT, capture_output=True, text=True).stdout
    metadata = dict(schema=1, cpu=cpu, cpu_count=os.cpu_count(), git_revision=revision, git_dirty=bool(dirty), invocation=sys.argv, timestamp=datetime.now(timezone.utc).isoformat(),
                    platform=platform.platform(), machine=platform.machine(), python=sys.version,
                    terminal=dict(term="xterm-256color", width=args.width, height=args.height),
                    settle_ms=args.settle_ms, seed=args.seed, runs=args.runs, warmups=args.warmups,
                    harness_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                    profiled=bool(args.profile), editors={name: {**info, "path": str(info["path"])} for name, info in binaries.items()})
    samples = []
    if args.profile:
        suffix = "sample.txt" if sys.platform == "darwin" else "perf.data"
        profile = args.output.with_suffix(f".{args.profile}-{selected[0].name}.{suffix}")
        samples.append(run(args.profile, binaries[args.profile]["path"], selected[0], args, profile))
        metadata["profile_artifact"] = str(profile)
        print(f"Profile: {profile}")
    else:
        rng = random.Random(args.seed)
        jobs = [(name, case) for name in binaries for case in selected]
        for iteration in range(args.warmups + args.runs):
            rng.shuffle(jobs)
            for name, case in jobs:
                samples.append(dict(warmup=iteration < args.warmups, iteration=iteration,
                                    **run(name, binaries[name]["path"], case, args)))
            print(f"Completed round {iteration + 1}/{args.warmups + args.runs}", file=sys.stderr)
    args.output.write_text(json.dumps(dict(metadata=metadata, samples=samples), indent=2) + "\n")
    if not args.profile:
        print("Case | Editor | First fixture ms | Task+save+exit ms | CPU ms* | Peak MiB*")
        print("--- | --- | ---: | ---: | ---: | ---:")
        for case in selected:
            for name in binaries:
                rows = [s for s in samples if s["case"] == case.name and s["editor"] == name and not s["warmup"]]
                values = [statistics.median(s[key] for s in rows) for key in
                          ("startup_first_fixture_ms", "task_save_exit_ms", "child_cpu_ms", "peak_rss_mib")]
                print(f"{case.name} | {name} | " + " | ".join(f"{v:.3f}" for v in values))
        print("* CPU and peak RSS cover the entire child lifetime. Times are medians; raw samples include warmups.")
    print(f"Results: {args.output}")


if __name__ == "__main__":
    main()
