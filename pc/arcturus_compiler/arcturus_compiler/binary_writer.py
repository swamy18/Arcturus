"""Firmware-compatible Arcturus W-matrix binary writer."""

from __future__ import annotations

from dataclasses import asdict, is_dataclass
from pathlib import Path
from typing import Iterable, List, Sequence, Tuple
import json
import struct

from .w_mapper import SparseElement


def _normalize_element(element) -> Tuple[int, int, int, int]:
    if isinstance(element, SparseElement):
        return element.row, element.col, element.value_fixed, element.imag_fixed
    if is_dataclass(element):
        data = asdict(element)
        return (
            int(data.get("row")),
            int(data.get("col")),
            int(data.get("value_fixed", data.get("value", 0))),
            int(data.get("imag_fixed", data.get("im", 0))),
        )
    if len(element) == 3:
        row, col, value = element
        return int(row), int(col), int(value), 0
    if len(element) == 4:
        row, col, re_value, im_value = element
        return int(row), int(col), int(re_value), int(im_value)
    raise ValueError(f"Unsupported element format: {element!r}")


def serialize_awm(elements: Sequence, dimension: int = 10_000) -> bytes:
    """Serialize a sparse matrix in the firmware's current format.

    Layout:
    - dimension: u16
    - num_elements: u32
    - repeated entries of row: u16, col: u16, re: i32, im: i32
    """

    payload = bytearray()
    payload.extend(struct.pack("<H", int(dimension)))
    payload.extend(struct.pack("<I", len(elements)))
    for element in elements:
        row, col, re_value, im_value = _normalize_element(element)
        payload.extend(struct.pack("<HHii", row, col, int(re_value), int(im_value)))
    return bytes(payload)


def write_awm(filename: str | Path, elements: Sequence, dimension: int = 10_000) -> Path:
    path = Path(filename)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(serialize_awm(elements, dimension=dimension))
    return path


def write_awm_banks(
    output_path: str | Path,
    banks: Sequence[Sequence],
    dimension: int = 10_000,
) -> List[Path]:
    """Write one .awm file per bank and emit a manifest next to them."""

    path = Path(output_path)
    stem = path.with_suffix("") if path.suffix else path
    written: List[Path] = []
    manifest = []

    for index, bank_elements in enumerate(banks):
        bank_path = stem.parent / f"{stem.name}_bank{index:03d}.awm"
        written.append(write_awm(bank_path, bank_elements, dimension=dimension))
        manifest.append(
            {
                "bank": index,
                "file": bank_path.name,
                "elements": len(bank_elements),
                "bytes": bank_path.stat().st_size,
            }
        )

    manifest_path = stem.parent / f"{stem.name}.manifest.json"
    manifest_path.write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    written.append(manifest_path)
    return written
