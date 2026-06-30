"""Map model weights into the sparse Arcturus W-matrix format."""

from __future__ import annotations

from dataclasses import dataclass
from math import ceil
from typing import Dict, Iterable, List, Sequence, Tuple

import numpy as np


FIXED_SCALE = 1 << 16


@dataclass(frozen=True)
class SparseElement:
    row: int
    col: int
    value_fixed: int
    imag_fixed: int = 0


def float_to_fixed(value: float) -> int:
    return int(np.clip(np.round(value * FIXED_SCALE), -2**31, 2**31 - 1))


class WMatrixMapper:
    """Project dense tensors into sparse Arcturus W-matrix elements.

    The mapper uses blockwise L2 energy preservation: each block becomes one
    sparse element whose magnitude equals the block Frobenius norm. That keeps
    the sparse matrix norm close to the source tensor norm while dramatically
    reducing element count.
    """

    def __init__(
        self,
        num_nodes: int = 10_000,
        block_size: int = 2048,
        threshold: float = 0.0,
        max_elements: int = 100_000_000,
    ) -> None:
        self.num_nodes = int(num_nodes)
        self.block_size = int(block_size)
        self.threshold = float(threshold)
        self.max_elements = int(max_elements)
        self.elements: List[SparseElement] = []
        self.tensor_slices: List[Tuple[str, int, int]] = []

    @property
    def capacity(self) -> int:
        return self.num_nodes * self.num_nodes

    def clear(self) -> None:
        self.elements.clear()
        self.tensor_slices.clear()

    def _append_block(self, linear_index: int, value: float) -> None:
        if linear_index >= self.capacity:
            raise ValueError(
                f"Model exceeds Arcturus node capacity: {linear_index} >= {self.capacity}"
            )
        row = linear_index // self.num_nodes
        col = linear_index % self.num_nodes
        fixed_value = float_to_fixed(value)
        self.elements.append(SparseElement(row=row, col=col, value_fixed=fixed_value))

    def map_dense_to_sparse(self, weights: np.ndarray, threshold: float | None = None) -> List[SparseElement]:
        """Map a dense tensor to sparse elements using blockwise energy.

        Each contiguous block contributes one sparse cell:
        - the magnitude is the block Frobenius norm
        - the sign follows the block mean
        """

        flat = np.asarray(weights, dtype=np.float32).reshape(-1)
        block_threshold = self.threshold if threshold is None else float(threshold)
        self.clear()

        for block_number, start in enumerate(range(0, flat.size, self.block_size)):
            block = flat[start : start + self.block_size]
            if block.size == 0:
                continue
            block_norm = float(np.linalg.norm(block))
            if block_norm <= block_threshold:
                continue
            sign = -1.0 if float(block.mean()) < 0.0 else 1.0
            self._append_block(block_number, sign * block_norm)

        return self.elements

    def map_state_dict(self, state_dict: Dict[str, np.ndarray], threshold: float | None = None) -> List[SparseElement]:
        """Map every tensor in a state dict into the sparse W matrix."""

        self.clear()
        linear_index = 0
        block_threshold = self.threshold if threshold is None else float(threshold)

        for name in sorted(state_dict):
            tensor = np.asarray(state_dict[name], dtype=np.float32)
            flat = tensor.reshape(-1)
            block_count = ceil(flat.size / self.block_size)
            self.tensor_slices.append((name, linear_index, block_count))

            for start in range(0, flat.size, self.block_size):
                block = flat[start : start + self.block_size]
                if block.size == 0:
                    continue
                block_norm = float(np.linalg.norm(block))
                if block_norm <= block_threshold:
                    linear_index += 1
                    continue
                sign = -1.0 if float(block.mean()) < 0.0 else 1.0
                self._append_block(linear_index, sign * block_norm)
                linear_index += 1

        if len(self.elements) > self.max_elements:
            self.elements = self._keep_top_k(self.elements, self.max_elements)

        return self.elements

    @staticmethod
    def _keep_top_k(elements: Sequence[SparseElement], k: int) -> List[SparseElement]:
        ranked = sorted(elements, key=lambda e: abs(e.value_fixed), reverse=True)
        return list(ranked[:k])

    def slice_to_banks(
        self,
        num_banks: int = 1000,
        bank_capacity_bytes: int = 64 * 1024,
    ) -> List[List[SparseElement]]:
        """Split sparse elements into bank-sized chunks."""

        if not self.elements:
            return [[]]

        bytes_per_element = 12
        header_bytes = 6
        max_elements_per_bank = max(1, (bank_capacity_bytes - header_bytes) // bytes_per_element)
        target_bank_count = min(num_banks, ceil(len(self.elements) / max_elements_per_bank))
        if target_bank_count <= 0:
            target_bank_count = 1

        banks: List[List[SparseElement]] = []
        start = 0
        remaining = len(self.elements)
        remaining_banks = target_bank_count

        while remaining > 0 and remaining_banks > 0:
            chunk_size = min(max_elements_per_bank, ceil(remaining / remaining_banks))
            chunk = list(self.elements[start : start + chunk_size])
            banks.append(chunk)
            start += chunk_size
            remaining -= chunk_size
            remaining_banks -= 1

        return banks

    def total_bytes(self) -> int:
        return 6 + len(self.elements) * 12

    def sparse_norm_sqr(self) -> int:
        total = 0
        for element in self.elements:
            total += (element.value_fixed * element.value_fixed) >> 16
            total += (element.imag_fixed * element.imag_fixed) >> 16
        return int(total)
