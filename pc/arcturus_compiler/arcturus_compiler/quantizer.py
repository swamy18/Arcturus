"""Quantization helpers for compiler input tensors."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Dict, Tuple

import numpy as np


@dataclass
class QuantizedTensor:
    name: str
    shape: Tuple[int, ...]
    bits: int
    group_size: int
    qvalues: np.ndarray
    scales: np.ndarray
    zero_points: np.ndarray

    @property
    def packed_size_bytes(self) -> int:
        value_bytes = int(np.ceil(self.qvalues.size / 2.0)) if self.bits == 4 else self.qvalues.nbytes
        return value_bytes + self.scales.nbytes + self.zero_points.nbytes


def _quantize_group(group: np.ndarray, bits: int) -> Tuple[np.ndarray, float, int]:
    levels = 1 << bits
    group = np.asarray(group, dtype=np.float32)
    mn = float(group.min())
    mx = float(group.max())
    if mx == mn:
        return np.zeros(group.shape, dtype=np.uint8), 1.0, 0

    scale = (mx - mn) / max(levels - 1, 1)
    if scale == 0:
        scale = 1.0
    zero_point = int(np.round(-mn / scale))
    zero_point = max(0, min(levels - 1, zero_point))

    q = np.round(group / scale + zero_point)
    q = np.clip(q, 0, levels - 1).astype(np.uint8)
    return q, float(scale), int(zero_point)


def quantize_nbit(weights: np.ndarray, bits: int = 4, group_size: int = 64) -> QuantizedTensor:
    """Quantize a tensor into n-bit group-wise affine codes."""

    if bits not in (4, 8, 16):
        raise ValueError("bits must be 4, 8, or 16")

    flat = np.asarray(weights, dtype=np.float32).reshape(-1)
    q_dtype = np.uint8 if bits <= 8 else np.uint16
    qvalues = np.empty(flat.shape, dtype=q_dtype)
    scales = []
    zero_points = []

    for start in range(0, flat.size, group_size):
        group = flat[start : start + group_size]
        q, scale, zero_point = _quantize_group(group, bits)
        qvalues[start : start + q.size] = q.astype(q_dtype, copy=False)
        scales.append(scale)
        zero_points.append(zero_point)

    return QuantizedTensor(
        name="tensor",
        shape=tuple(weights.shape),
        bits=bits,
        group_size=group_size,
        qvalues=qvalues,
        scales=np.asarray(scales, dtype=np.float32),
        zero_points=np.asarray(zero_points, dtype=np.int32),
    )


def quantize_4bit(weights: np.ndarray, group_size: int = 64) -> Tuple[np.ndarray, np.ndarray, np.ndarray]:
    """Compatibility wrapper returning qvalues, scales, zero_points."""

    qt = quantize_nbit(weights, bits=4, group_size=group_size)
    return qt.qvalues, qt.scales, qt.zero_points


def dequantize_tensor(qt: QuantizedTensor) -> np.ndarray:
    """Reconstruct a float tensor from quantized group-wise codes."""

    flat = np.empty(qt.qvalues.size, dtype=np.float32)
    levels = 1 << qt.bits
    group_count = len(qt.scales)

    for group_index in range(group_count):
        start = group_index * qt.group_size
        end = min(start + qt.group_size, qt.qvalues.size)
        scale = float(qt.scales[group_index])
        zero_point = int(qt.zero_points[group_index])
        codes = qt.qvalues[start:end].astype(np.float32)
        if levels == 1:
            flat[start:end] = 0.0
        else:
            flat[start:end] = (codes - zero_point) * scale

    return flat.reshape(qt.shape)


def quantize_state_dict(
    state_dict: Dict[str, np.ndarray],
    bits: int = 4,
    group_size: int = 64,
) -> Dict[str, QuantizedTensor]:
    """Quantize every tensor in a state dict."""

    quantized: Dict[str, QuantizedTensor] = {}
    for name, tensor in state_dict.items():
        q = quantize_nbit(tensor, bits=bits, group_size=group_size)
        q.name = name
        quantized[name] = q
    return quantized
