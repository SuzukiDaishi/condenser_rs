"""Smoke tests for the Condenser Rs VST3 using Pedalboard."""

from __future__ import annotations

from pathlib import Path

import numpy as np
from pedalboard import Pedalboard, load_plugin


def load_condenser() -> "pedalboard.VST3Plugin":
    plugin_path = Path(__file__).resolve().parents[1] / "target/bundled/Condenser Rs.vst3"
    if not plugin_path.exists():
        raise FileNotFoundError(f"Plugin not found at {plugin_path}")
    plugin = load_plugin(str(plugin_path))
    print(f"Loaded plugin: {plugin}")
    return plugin


def test_basic_process(plugin) -> None:
    sr = 48_000
    dummy = np.zeros((sr, 2), dtype=np.float32)
    board = Pedalboard([plugin])
    out = board.process(dummy, sr)
    assert out.shape == dummy.shape


def test_bypass(plugin) -> None:
    sr = 48_000
    noise = np.random.randn(sr, 2).astype(np.float32)
    plugin.dry_wet = 0.0
    plugin.loop_mode = False
    plugin.threshold_db = -80.0
    plugin.reset()
    board = Pedalboard([plugin])
    out = board.process(noise, sr)
    assert np.allclose(out, noise, atol=1e-6)


def test_loop_record_playback(plugin) -> None:
    sr = 48_000
    t = np.arange(sr, dtype=np.float32)
    tone = np.sin(2 * np.pi * 440 * t / sr).astype(np.float32)
    stereo = np.stack((tone, tone), axis=1)

    plugin.threshold_db = -80.0
    plugin.dry_wet = 1.0
    plugin.loop_length = 1
    plugin.loop_mode = False
    plugin.reset()
    board = Pedalboard([plugin])

    # Process a 2s buffer: first second of tone, second of silence
    block = np.concatenate([stereo, np.zeros_like(stereo)])
    out = board.process(block, sr)
    assert out.shape == block.shape
    # The second half should contain the looped tone, not silence
    assert not np.allclose(out[sr:], 0.0)


def main() -> None:
    plugin = load_condenser()
    test_basic_process(plugin)
    test_bypass(plugin)
    test_loop_record_playback(plugin)
    print("All VST tests passed")


if __name__ == "__main__":
    main()
