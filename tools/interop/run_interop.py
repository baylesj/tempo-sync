#!/usr/bin/env python3
"""Interop test runner between Ableton Link C++ oracle and tempo-sync peers."""

import argparse
import json
import os
import queue
import shlex
import subprocess
import sys
import threading
import time
from typing import Any, Callable, Dict, Optional


class PeerProcess:
    """Manages a Link peer process communicating over stdin/stdout with JSON lines."""

    def __init__(self, name: str, cmd: list[str], cwd: Optional[str] = None, verbose: bool = False):
        self.name = name
        self.cmd = cmd
        self.cwd = cwd
        self.verbose = verbose
        self.proc: Optional[subprocess.Popen] = None
        self.latest_state: Optional[Dict[str, Any]] = None
        self.state_lock = threading.Lock()
        self.ack_queue: queue.Queue[Dict[str, Any]] = queue.Queue()
        self.stdout_thread: Optional[threading.Thread] = None
        self.stderr_thread: Optional[threading.Thread] = None
        self.running = False

    def start(self) -> None:
        if self.verbose:
            print(f"[{self.name}] Spawning: {' '.join(self.cmd)}")
        self.proc = subprocess.Popen(
            self.cmd,
            cwd=self.cwd,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1,
        )
        self.running = True
        self.stdout_thread = threading.Thread(target=self._read_stdout, daemon=True)
        self.stderr_thread = threading.Thread(target=self._read_stderr, daemon=True)
        self.stdout_thread.start()
        self.stderr_thread.start()

    def _read_stdout(self) -> None:
        assert self.proc is not None and self.proc.stdout is not None
        for raw_line in self.proc.stdout:
            line = raw_line.strip()
            if not line:
                continue
            try:
                data = json.loads(line)
            except json.JSONDecodeError:
                if self.verbose:
                    print(f"[{self.name}] stdout (raw): {line}")
                continue

            if "ack" in data:
                if self.verbose:
                    print(f"[{self.name}] recv ack: {line}")
                self.ack_queue.put(data)
            else:
                with self.state_lock:
                    self.latest_state = data

        self.running = False

    def _read_stderr(self) -> None:
        assert self.proc is not None and self.proc.stderr is not None
        for raw_line in self.proc.stderr:
            line = raw_line.strip()
            if line:
                print(f"[{self.name}] stderr: {line}", file=sys.stderr)

    def get_latest_state(self) -> Optional[Dict[str, Any]]:
        with self.state_lock:
            if self.latest_state is None:
                return None
            return dict(self.latest_state)

    def send_command(self, cmd_line: str, wait_ack: Optional[str] = None, timeout: float = 3.0) -> None:
        assert self.proc is not None and self.proc.stdin is not None
        if self.verbose:
            print(f"[{self.name}] send: {cmd_line}")
        self.proc.stdin.write(cmd_line + "\n")
        self.proc.stdin.flush()

        if wait_ack is not None:
            deadline = time.time() + timeout
            while time.time() < deadline:
                remaining = max(0.05, deadline - time.time())
                try:
                    ack = self.ack_queue.get(timeout=remaining)
                    if ack.get("ack") == wait_ack:
                        return
                except queue.Empty:
                    break
            raise TimeoutError(f"[{self.name}] Timed out waiting for ack '{wait_ack}'")

    def wait_for_condition(
        self,
        predicate: Callable[[Dict[str, Any]], bool],
        timeout: float,
        description: str,
    ) -> Dict[str, Any]:
        deadline = time.time() + timeout
        last_state = None
        while time.time() < deadline:
            state = self.get_latest_state()
            if state is not None:
                last_state = state
                if predicate(state):
                    return state
            time.sleep(0.02)
        raise TimeoutError(
            f"[{self.name}] Timed out after {timeout}s waiting for {description}. Last state: {last_state}"
        )

    def stop(self, timeout: float = 3.0) -> None:
        if self.proc is None:
            return
        if self.proc.poll() is None:
            try:
                self.send_command("quit", wait_ack="quit", timeout=timeout)
            except Exception:
                pass

        if self.proc.poll() is None:
            try:
                assert self.proc.stdin is not None
                self.proc.stdin.close()
            except Exception:
                pass

        try:
            self.proc.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            if self.verbose:
                print(f"[{self.name}] Process did not exit cleanly, terminating...")
            self.proc.terminate()
            try:
                self.proc.wait(timeout=2.0)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.wait()


def map_beat_to_time(beat: float, tempo: float, from_time_us: int, to_time_us: int) -> float:
    """Project a beat value at from_time_us forward or backward to to_time_us."""
    time_delta_us = to_time_us - from_time_us
    beats_per_microsecond = tempo / 60_000_000.0
    return beat + time_delta_us * beats_per_microsecond


def run_test(
    peer1_cmd: list[str],
    peer2_cmd: list[str],
    timeout: float,
    quantum: float,
    verbose: bool,
) -> None:
    p1 = PeerProcess("Peer1 (Oracle)", peer1_cmd, verbose=verbose)
    p2 = PeerProcess("Peer2", peer2_cmd, verbose=verbose)

    try:
        print("Starting peers...")
        p1.start()
        p2.start()

        # Step 1: Wait for both peers to discover each other.
        print("Step 1: Waiting for peer discovery (num_peers >= 1)...")
        p1.wait_for_condition(
            lambda s: s.get("num_peers", 0) >= 1,
            timeout=timeout,
            description="Peer1 num_peers >= 1",
        )
        p2.wait_for_condition(
            lambda s: s.get("num_peers", 0) >= 1,
            timeout=timeout,
            description="Peer2 num_peers >= 1",
        )
        print("  Both peers discovered each other successfully.")

        # Step 2: Check tempo equality and beat alignment.
        print("Step 2: Checking tempo equality and beat grid alignment...")
        s1 = p1.wait_for_condition(lambda s: "tempo" in s, timeout=timeout, description="Peer1 valid state")
        s2 = p2.wait_for_condition(lambda s: "tempo" in s, timeout=timeout, description="Peer2 valid state")

        tempo1 = s1["tempo"]
        tempo2 = s2["tempo"]
        tempo_diff = abs(tempo1 - tempo2)
        print(f"  Peer1 tempo: {tempo1:.3f} BPM, Peer2 tempo: {tempo2:.3f} BPM (diff: {tempo_diff:.4f})")
        if tempo_diff > 0.01:
            raise AssertionError(f"Tempos differ by more than 0.01 BPM: {tempo1} vs {tempo2}")

        # Map Peer1's beat to Peer2's timestamp and compare.
        t1 = s1["host_time_us"]
        t2 = s2["host_time_us"]
        b1 = s1["beat"]
        b2 = s2["beat"]
        b1_projected = map_beat_to_time(b1, tempo1, t1, t2)
        beat_diff = abs(b1_projected - b2)

        # Also evaluate phase alignment within quantum.
        phase1_projected = b1_projected % quantum
        phase2 = b2 % quantum
        phase_diff = abs((phase1_projected - phase2 + quantum / 2.0) % quantum - quantum / 2.0)

        print(
            f"  Beat check: Peer1 projected={b1_projected:.4f}, Peer2={b2:.4f}, "
            f"diff={beat_diff:.4f} beats (phase diff={phase_diff:.4f})"
        )
        if beat_diff > 0.1 and phase_diff > 0.05:
            raise AssertionError(
                f"Beats do not agree within tolerance: projected {b1_projected} vs {b2} (diff {beat_diff})"
            )
        print("  Tempo and beat grid verified.")

        # Step 3: Peer1 changes tempo to 136.0 BPM.
        print("Step 3: Peer1 sets tempo to 136.0 BPM...")
        p1.send_command("tempo 136.0", wait_ack="tempo", timeout=timeout)
        p2.wait_for_condition(
            lambda s: abs(s.get("tempo", 0.0) - 136.0) < 0.01,
            timeout=timeout,
            description="Peer2 tempo follows Peer1 to 136.0 BPM",
        )
        print("  Peer2 successfully synchronized tempo to 136.0 BPM.")

        # Step 4: Peer2 changes tempo to 114.0 BPM.
        print("Step 4: Peer2 sets tempo to 114.0 BPM...")
        p2.send_command("tempo 114.0", wait_ack="tempo", timeout=timeout)
        p1.wait_for_condition(
            lambda s: abs(s.get("tempo", 0.0) - 114.0) < 0.01,
            timeout=timeout,
            description="Peer1 tempo follows Peer2 to 114.0 BPM",
        )
        print("  Peer1 successfully synchronized tempo to 114.0 BPM.")

        # Step 5: Peer1 starts transport (play).
        print("Step 5: Peer1 sends 'play' command...")
        p1.send_command("play", wait_ack="play", timeout=timeout)
        p2.wait_for_condition(
            lambda s: s.get("is_playing") is True,
            timeout=timeout,
            description="Peer2 observes is_playing == True",
        )
        print("  Peer2 successfully observed start transport.")

        # Step 6: Peer2 stops transport (stop).
        print("Step 6: Peer2 sends 'stop' command...")
        p2.send_command("stop", wait_ack="stop", timeout=timeout)
        p1.wait_for_condition(
            lambda s: s.get("is_playing") is False,
            timeout=timeout,
            description="Peer1 observes is_playing == False",
        )
        print("  Peer1 successfully observed stop transport.")

        print("\nAll interop assertions passed successfully!")

    finally:
        print("Cleaning up peer processes...")
        p1.stop()
        p2.stop()


def main() -> int:
    parser = argparse.ArgumentParser(description="Tempo sync interop test harness.")
    parser.add_argument(
        "--skip-rust",
        action="store_true",
        help="Run two C++ linkpeers against each other (for testing without Rust tempohut).",
    )
    parser.add_argument(
        "--linkpeer-bin",
        default=os.path.join(os.path.dirname(__file__), "..", "linkpeer", "linkpeer"),
        help="Path to the C++ linkpeer executable.",
    )
    parser.add_argument(
        "--tempohut-cmd",
        default="cargo run -p tempohut -- --json",
        help="Command to run the Rust tempohut peer.",
    )
    parser.add_argument(
        "--timeout",
        type=float,
        default=10.0,
        help="Timeout in seconds for synchronization assertions.",
    )
    parser.add_argument(
        "--quantum",
        type=float,
        default=4.0,
        help="Beat quantum for phase calculations.",
    )
    parser.add_argument(
        "-v",
        "--verbose",
        action="store_true",
        help="Verbose logging of stdout/stderr messages.",
    )
    args = parser.parse_args()

    linkpeer_path = os.path.abspath(args.linkpeer_bin)
    if not os.path.isfile(linkpeer_path) or not os.access(linkpeer_path, os.X_OK):
        print(f"Error: linkpeer executable not found at {linkpeer_path}", file=sys.stderr)
        print("Build it first using tools/linkpeer/build.sh", file=sys.stderr)
        return 1

    peer1_cmd = [
        linkpeer_path,
        "--tempo",
        "120.0",
        "--quantum",
        str(args.quantum),
        "--interval-ms",
        "50",
    ]

    if args.skip_rust:
        peer2_cmd = [
            linkpeer_path,
            "--tempo",
            "120.0",
            "--quantum",
            str(args.quantum),
            "--interval-ms",
            "50",
        ]
    else:
        peer2_cmd = shlex.split(args.tempohut_cmd)

    try:
        run_test(
            peer1_cmd=peer1_cmd,
            peer2_cmd=peer2_cmd,
            timeout=args.timeout,
            quantum=args.quantum,
            verbose=args.verbose,
        )
        return 0
    except Exception as exc:
        print(f"\nInterop test failed: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
