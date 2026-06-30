"""Validation helpers for the Arcturus compiler."""

from __future__ import annotations

from dataclasses import asdict, dataclass
from math import sqrt
from pathlib import Path
from typing import Dict, Iterable, List, Sequence

import numpy as np

from .binary_writer import serialize_awm
from .quantizer import QuantizedTensor, dequantize_tensor
from .w_mapper import SparseElement


FIXED_NORM_SCALE = 256.0


@dataclass
class ValidationReport:
    model_name: str
    param_count: int
    original_norm: float
    quantized_norm: float
    sparse_norm: float
    quantization_drift: float
    sparse_drift: float
    total_elements: int
    total_bytes: int
    fits_100m_elements: bool
    fits_64mb_psram: bool


def tensor_frobenius_norm(array: np.ndarray) -> float:
    array = np.asarray(array, dtype=np.float32)
    return float(np.linalg.norm(array))


def validate_compilation(
    model_name: str,
    state_dict: Dict[str, np.ndarray],
    quantized: Dict[str, QuantizedTensor],
    elements: Sequence[SparseElement],
    num_nodes: int = 10_000,
    bank_capacity_bytes: int = 64 * 1024,
) -> ValidationReport:
    """Compute high-level validation metrics."""

    original_norm_sq = 0.0
    quantized_norm_sq = 0.0
    for name in sorted(state_dict):
        original = np.asarray(state_dict[name], dtype=np.float32)
        dequantized = dequantize_tensor(quantized[name])
        original_norm_sq += float(np.sum(np.square(original)))
        quantized_norm_sq += float(np.sum(np.square(dequantized)))

    sparse_norm_sq = 0.0
    for element in elements:
        sparse_norm_sq += float((element.value_fixed * element.value_fixed) >> 16)
        sparse_norm_sq += float((element.imag_fixed * element.imag_fixed) >> 16)

    original_norm = sqrt(original_norm_sq)
    quantized_norm = sqrt(quantized_norm_sq)
    sparse_norm = sqrt(sparse_norm_sq) / FIXED_NORM_SCALE

    quantization_drift = 0.0 if original_norm == 0 else abs(quantized_norm - original_norm) / original_norm
    sparse_drift = 0.0 if quantized_norm == 0 else abs(sparse_norm - quantized_norm) / quantized_norm

    total_bytes = 6 + len(elements) * 12
    max_total_elements = num_nodes * num_nodes
    fits_64mb_psram = total_bytes <= 64 * 1024 * 1024

    return ValidationReport(
        model_name=model_name,
        param_count=int(sum(arr.size for arr in state_dict.values())),
        original_norm=float(original_norm),
        quantized_norm=float(quantized_norm),
        sparse_norm=float(sparse_norm),
        quantization_drift=float(quantization_drift),
        sparse_drift=float(sparse_drift),
        total_elements=len(elements),
        total_bytes=total_bytes,
        fits_100m_elements=len(elements) <= max_total_elements,
        fits_64mb_psram=fits_64mb_psram,
    )
