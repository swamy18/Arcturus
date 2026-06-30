"""CLI entry point for the Arcturus compiler."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Dict

import numpy as np

from .binary_writer import write_awm, write_awm_banks
from .model_loader import load_model
from .quantizer import dequantize_tensor, quantize_state_dict
from .validator import validate_compilation
from .w_mapper import WMatrixMapper


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="arcturus-compiler", description="Compile PyTorch models into Arcturus W matrices")
    parser.add_argument("--model", required=True, help="HuggingFace model id or local checkpoint path")
    parser.add_argument("--output", required=True, help="Output .awm file or output prefix")
    parser.add_argument("--quantize", type=int, default=4, choices=[4, 8, 16], help="Quantization bits")
    parser.add_argument("--group-size", type=int, default=64, help="Group size for quantization")
    parser.add_argument("--block-size", type=int, default=2048, help="Block size used for sparse projection")
    parser.add_argument("--max-elements", type=int, default=100_000_000, help="Maximum sparse elements to keep")
    parser.add_argument("--num-nodes", type=int, default=10_000, help="Arcturus node count")
    parser.add_argument("--bank-capacity", type=int, default=64 * 1024, help="PSRAM bank capacity in bytes")
    parser.add_argument("--split-banks", action="store_true", help="Emit one .awm file per bank")
    parser.add_argument("--device", default="cpu", help="Torch device used for model loading")
    parser.add_argument("--threshold", type=float, default=0.0, help="Skip blocks below this norm threshold")
    return parser


def _dequantize_state_dict(quantized: Dict[str, object]) -> Dict[str, np.ndarray]:
    from .quantizer import QuantizedTensor

    return {name: dequantize_tensor(qtensor) for name, qtensor in quantized.items()}


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)

    loaded = load_model(args.model, device=args.device)
    quantized = quantize_state_dict(loaded.state_dict, bits=args.quantize, group_size=args.group_size)
    dequantized = _dequantize_state_dict(quantized)

    mapper = WMatrixMapper(
        num_nodes=args.num_nodes,
        block_size=args.block_size,
        threshold=args.threshold,
        max_elements=args.max_elements,
    )
    elements = mapper.map_state_dict(dequantized, threshold=args.threshold)

    report = validate_compilation(
        model_name=loaded.name,
        state_dict=loaded.state_dict,
        quantized=quantized,
        elements=elements,
        num_nodes=args.num_nodes,
        bank_capacity_bytes=args.bank_capacity,
    )

    output = Path(args.output)
    if args.split_banks:
        banks = mapper.slice_to_banks(num_banks=1000, bank_capacity_bytes=args.bank_capacity)
        written = write_awm_banks(output, banks, dimension=args.num_nodes)
        print(f"Wrote {len(banks)} bank files")
        print(f"Manifest: {written[-1]}")
    else:
        written = write_awm(output, elements, dimension=args.num_nodes)
        print(f"Wrote {written}")

    print(json.dumps({
        "model": report.model_name,
        "params": report.param_count,
        "original_norm": report.original_norm,
        "quantized_norm": report.quantized_norm,
        "sparse_norm": report.sparse_norm,
        "quantization_drift": report.quantization_drift,
        "sparse_drift": report.sparse_drift,
        "elements": report.total_elements,
        "bytes": report.total_bytes,
        "fits_100m": report.fits_100m_elements,
        "fits_psram": report.fits_64mb_psram,
    }, indent=2))

    if report.quantization_drift > 0.01:
        print("warning: quantization drift exceeds 1%", flush=True)

    return 0
