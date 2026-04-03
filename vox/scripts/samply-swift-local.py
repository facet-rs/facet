#!/usr/bin/env python3

import argparse
import gzip
import bisect
import json
import os
import signal
import socket
import stat
import subprocess
import sys
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
DEFAULT_ADDR = "local:///tmp/bench.vox"
DEFAULT_COUNT = 500_000
DEFAULT_PAYLOAD = 128
DEFAULT_IN_FLIGHT = 64
DEFAULT_DURATION = 12
DEFAULT_PREFIX = "profile.swift-local"
XCODE_TOOLCHAIN = Path(
    "/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib"
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Record the Swift subject with samply, demangle the sidecar, and load it."
    )
    parser.add_argument("--addr", default=DEFAULT_ADDR)
    parser.add_argument("--count", type=int, default=DEFAULT_COUNT)
    parser.add_argument("--payload-size", type=int, default=DEFAULT_PAYLOAD)
    parser.add_argument("--in-flight", type=int, default=DEFAULT_IN_FLIGHT)
    parser.add_argument("--duration", type=int, default=DEFAULT_DURATION)
    parser.add_argument("--prefix", default=DEFAULT_PREFIX)
    parser.add_argument("--port", type=int, default=3000)
    parser.add_argument("--no-open", action="store_true")
    return parser.parse_args()


def require_exists(path: Path, label: str) -> None:
    if not path.exists():
        raise SystemExit(f"{label} not found: {path}")


def wait_for_unix_socket(path: Path, timeout_s: float) -> None:
    deadline = time.monotonic() + timeout_s
    while time.monotonic() < deadline:
        if path.exists():
            try:
                mode = path.stat().st_mode
                if stat.S_ISSOCK(mode):
                    return
            except FileNotFoundError:
                pass
        time.sleep(0.05)
    raise SystemExit(f"timed out waiting for unix socket: {path}")


def wait_for_path(path: Path, timeout_s: float) -> None:
    deadline = time.monotonic() + timeout_s
    while time.monotonic() < deadline:
        if path.exists():
            return
        time.sleep(0.05)
    raise SystemExit(f"timed out waiting for file: {path}")


def run_checked(args: list[str], **kwargs) -> subprocess.CompletedProcess:
    return subprocess.run(args, check=True, text=True, **kwargs)


def demangle_strings(values: list[str]) -> dict[str, str]:
    if not values:
        return {}
    proc = run_checked(
        ["xcrun", "swift-demangle"],
        input="".join(f"{value}\n" for value in values),
        capture_output=True,
    )
    lines = proc.stdout.splitlines()
    if len(lines) != len(values):
        raise SystemExit(
            f"swift-demangle output mismatch: expected {len(values)}, got {len(lines)}"
    )
    return dict(zip(values, lines))


def cxx_demangle_strings(values: list[str]) -> dict[str, str]:
    if not values:
        return {}
    proc = run_checked(
        ["xcrun", "llvm-cxxfilt"],
        input="".join(f"{value}\n" for value in values),
        capture_output=True,
    )
    lines = proc.stdout.splitlines()
    if len(lines) != len(values):
        raise SystemExit(
            f"llvm-cxxfilt output mismatch: expected {len(values)}, got {len(lines)}"
        )
    return dict(zip(values, lines))


def demangle_sidecar(sidecar_path: Path) -> int:
    with sidecar_path.open("r", encoding="utf-8") as f:
        data = json.load(f)

    string_table = data.get("string_table")
    if not isinstance(string_table, list):
        raise SystemExit(f"unexpected sidecar shape in {sidecar_path}")

    unique_values = []
    seen = set()
    for value in string_table:
        if isinstance(value, str) and "$s" in value and value not in seen:
            seen.add(value)
            unique_values.append(value)

    if not unique_values:
        return 0

    replacements = demangle_strings(unique_values)
    rewritten = 0
    for i, value in enumerate(string_table):
        if not isinstance(value, str):
            continue
        replacement = replacements.get(value)
        if replacement and replacement != value:
            string_table[i] = replacement
            rewritten += 1

    with sidecar_path.open("w", encoding="utf-8") as f:
        json.dump(data, f, separators=(",", ":"))

    return rewritten


def parse_nm_symbols(path: Path) -> list[tuple[int, str]]:
    proc = run_checked(
        ["nm", "-arch", "arm64", "-n", str(path)],
        capture_output=True,
    )
    symbols: list[tuple[int, str]] = []
    pending_swift: list[str] = []
    pending_cpp: list[str] = []
    for line in proc.stdout.splitlines():
        parts = line.split()
        if len(parts) < 3:
            continue
        addr_text, kind, name = parts[0], parts[1], parts[2]
        if kind.upper() == "U":
            continue
        try:
            addr = int(addr_text, 16)
        except ValueError:
            continue
        if name.startswith("_$s"):
            pending_swift.append(name)
            symbols.append((addr, name))
        elif kind in {"T", "t"}:
            if name.startswith("_Z") or name.startswith("__Z"):
                pending_cpp.append(name)
            symbols.append((addr, name))

    swift_demangled = demangle_strings(pending_swift) if pending_swift else {}
    cpp_demangled = cxx_demangle_strings(pending_cpp) if pending_cpp else {}
    if swift_demangled or cpp_demangled:
        symbols = [
            (addr, swift_demangled.get(name, cpp_demangled.get(name, name)))
            for addr, name in symbols
        ]
    return symbols


def parse_dyld_exports(path: str, arch: str) -> list[tuple[int, str]]:
    proc = run_checked(
        ["xcrun", "dyld_info", "-arch", arch, "-exports", path],
        capture_output=True,
    )
    symbols: list[tuple[int, str]] = []
    pending_cpp: list[str] = []
    for line in proc.stdout.splitlines():
        parts = line.split(maxsplit=1)
        if len(parts) != 2 or not parts[0].startswith("0x"):
            continue
        try:
            addr = int(parts[0], 16)
        except ValueError:
            continue
        name = parts[1]
        if name.startswith("$ld$"):
            continue
        if name.startswith("_Z") or name.startswith("__Z"):
            pending_cpp.append(name)
        symbols.append((addr, name))
    if pending_cpp:
        cpp_demangled = cxx_demangle_strings(pending_cpp)
        symbols = [(addr, cpp_demangled.get(name, name)) for addr, name in symbols]
    symbols.sort(key=lambda item: item[0])
    return symbols


def find_swift_runtime_image(debug_name: str) -> Path | None:
    candidates = sorted(XCODE_TOOLCHAIN.glob(f"swift-*/macosx/{debug_name}"))
    if candidates:
        return candidates[-1]
    return None


def build_resolvers(libs: list[dict]) -> dict[str, callable]:
    resolvers: dict[str, callable] = {}
    for lib in libs:
        debug_name = lib.get("debugName")
        path = lib.get("debugPath") or lib.get("path")
        arch = lib.get("arch")
        if not isinstance(debug_name, str) or debug_name in resolvers:
            continue

        symbols: list[tuple[int, str]] = []
        if debug_name == "libswift_Concurrency.dylib":
            image = find_swift_runtime_image(debug_name)
            if image is not None:
                symbols = parse_nm_symbols(image)
        elif debug_name == "libsystem_trace.dylib" and isinstance(path, str) and arch:
            try:
                symbols = parse_dyld_exports(path, arch)
            except subprocess.CalledProcessError:
                symbols = []

        if not symbols:
            continue

        addrs = [addr for addr, _ in symbols]
        names = [name for _, name in symbols]

        def resolver(address: int, addrs=addrs, names=names) -> str | None:
            idx = bisect.bisect_right(addrs, address) - 1
            if idx < 0:
                return None
            base = addrs[idx]
            name = names[idx]
            delta = address - base
            if delta == 0:
                return name
            return f"{name} + 0x{delta:x}"

        resolvers[debug_name] = resolver

    return resolvers


def rewrite_runtime_placeholders(raw_profile: Path) -> int:
    with gzip.open(raw_profile, "rt", encoding="utf-8") as f:
        profile = json.load(f)

    resolvers = build_resolvers(profile.get("libs", []))
    if not resolvers:
        return 0

    rewritten = 0
    for thread in profile.get("threads", []):
        string_array = thread["stringArray"]
        resource_table = thread["resourceTable"]
        func_table = thread["funcTable"]
        frame_table = thread["frameTable"]

        address_by_func: dict[int, int] = {}
        for frame_idx in range(frame_table["length"]):
            func_idx = frame_table["func"][frame_idx]
            if func_idx is not None and func_idx not in address_by_func:
                address_by_func[func_idx] = frame_table["address"][frame_idx]

        for func_idx in range(func_table["length"]):
            name_idx = func_table["name"][func_idx]
            resource_idx = func_table["resource"][func_idx]
            if name_idx is None or resource_idx is None:
                continue
            if resource_table["lib"][resource_idx] is None:
                continue
            lib_idx = resource_table["lib"][resource_idx]
            lib_name = profile["libs"][lib_idx]["debugName"]
            resolver = resolvers.get(lib_name)
            if resolver is None:
                continue

            raw_name = string_array[name_idx]
            if not isinstance(raw_name, str) or not raw_name.startswith("0x"):
                continue

            address = address_by_func.get(func_idx)
            if address is None:
                try:
                    address = int(raw_name, 16)
                except ValueError:
                    continue

            resolved = resolver(address)
            if not resolved or resolved == raw_name:
                continue

            string_array.append(resolved)
            func_table["name"][func_idx] = len(string_array) - 1
            rewritten += 1

    with gzip.open(raw_profile, "wt", encoding="utf-8") as f:
        json.dump(profile, f, separators=(",", ":"))

    return rewritten


def terminate(proc: subprocess.Popen | None) -> None:
    if proc is None:
        return
    if proc.poll() is not None:
        return
    proc.terminate()
    try:
        proc.wait(timeout=2)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait(timeout=2)


def main() -> int:
    args = parse_args()

    bench_bin = ROOT / "target/release/examples/bench_client"
    subject_bin = ROOT / "swift/subject/.build/release/subject-swift"
    require_exists(bench_bin, "bench client")
    require_exists(subject_bin, "subject-swift")

    if not args.addr.startswith("local://"):
        raise SystemExit("this helper currently only supports local:// addresses")

    socket_path = Path(args.addr.removeprefix("local://"))
    raw_profile = ROOT / f"{args.prefix}.json.gz"
    sidecar = ROOT / f"{args.prefix}.json.syms.json"
    bench_log = Path("/tmp/bench-samply-helper.log")
    swift_log = Path("/tmp/swift-samply-helper.log")

    for path in [socket_path, raw_profile, sidecar, bench_log, swift_log]:
        try:
            path.unlink()
        except FileNotFoundError:
            pass

    subprocess.run(["pkill", "-f", "samply|bench_client|subject-swift"], check=False)

    bench_proc = None
    try:
        with bench_log.open("w", encoding="utf-8") as bench_out, swift_log.open(
            "w", encoding="utf-8"
        ) as swift_out:
            bench_proc = subprocess.Popen(
                [
                    str(bench_bin),
                    "--count",
                    str(args.count),
                    "--addr",
                    args.addr,
                    "--payload-sizes",
                    str(args.payload_size),
                    "--in-flights",
                    str(args.in_flight),
                ],
                cwd=ROOT,
                stdout=bench_out,
                stderr=subprocess.STDOUT,
                text=True,
            )

            wait_for_unix_socket(socket_path, 10.0)

            env = os.environ.copy()
            env["SUBJECT_MODE"] = "server"
            env["PEER_ADDR"] = args.addr
            run_checked(
                [
                    "samply",
                    "record",
                    "--save-only",
                    "--unstable-presymbolicate",
                    "--duration",
                    str(args.duration),
                    "-o",
                    str(raw_profile),
                    str(subject_bin),
                ],
                cwd=ROOT,
                env=env,
                stdout=swift_out,
                stderr=subprocess.STDOUT,
            )

        wait_for_path(raw_profile, 5.0)
        wait_for_path(sidecar, 5.0)

        bench_rc = bench_proc.wait(timeout=max(args.duration, 1) + 30)
        if bench_rc != 0:
            raise SystemExit(f"bench client failed; see {bench_log}")

        rewritten = demangle_sidecar(sidecar)
        print(f"demangled {rewritten} sidecar strings", file=sys.stderr)
        rewritten_runtime = rewrite_runtime_placeholders(raw_profile)
        print(f"rewrote {rewritten_runtime} runtime placeholders", file=sys.stderr)

        load_cmd = ["samply", "load", "-P", str(args.port)]
        if args.no_open:
            load_cmd.append("--no-open")
        load_cmd.append(str(raw_profile))
        os.execvp(load_cmd[0], load_cmd)
    finally:
        terminate(bench_proc)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except KeyboardInterrupt:
        raise SystemExit(130)
