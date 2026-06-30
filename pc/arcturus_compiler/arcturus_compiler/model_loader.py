"""Model loading helpers for HuggingFace and local checkpoints."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Dict, Mapping

import numpy as np


@dataclass
class LoadedModel:
    name: str
    state_dict: Dict[str, np.ndarray]

    @property
    def param_count(self) -> int:
        return int(sum(array.size for array in self.state_dict.values()))


def _tensor_to_numpy(value) -> np.ndarray:
    if hasattr(value, "detach"):
        value = value.detach()
    if hasattr(value, "cpu"):
        value = value.cpu()
    if hasattr(value, "float"):
        try:
            value = value.float()
        except TypeError:
            pass
    if hasattr(value, "numpy"):
        return np.asarray(value.numpy(), dtype=np.float32)
    return np.asarray(value, dtype=np.float32)


def _load_hf_model(model_name: str, device: str = "cpu") -> Mapping[str, np.ndarray]:
    import torch
    from transformers import AutoModelForCausalLM

    model = AutoModelForCausalLM.from_pretrained(model_name)
    model.to(device)
    model.eval()
    state_dict = model.state_dict()
    return {name: _tensor_to_numpy(tensor) for name, tensor in state_dict.items()}


def _load_torch_checkpoint(path: Path) -> Mapping[str, np.ndarray]:
    import torch

    checkpoint = torch.load(path, map_location="cpu")
    if isinstance(checkpoint, dict) and "state_dict" in checkpoint:
        checkpoint = checkpoint["state_dict"]
    if not isinstance(checkpoint, dict):
        raise ValueError(f"Unsupported checkpoint format: {path}")
    return {name: _tensor_to_numpy(tensor) for name, tensor in checkpoint.items()}


def _load_safetensors(path: Path) -> Mapping[str, np.ndarray]:
    from safetensors.numpy import load_file

    tensors = load_file(str(path))
    return {name: np.asarray(tensor, dtype=np.float32) for name, tensor in tensors.items()}


def load_model(model_name: str, device: str = "cpu") -> LoadedModel:
    """Load a HuggingFace model or a local checkpoint.

    Returns a plain numpy state dict so the rest of the compiler can stay
    framework-agnostic after extraction.
    """

    path = Path(model_name)
    if path.exists():
        if path.suffix == ".safetensors":
            state_dict = _load_safetensors(path)
        else:
            state_dict = _load_torch_checkpoint(path)
        return LoadedModel(name=str(path), state_dict=dict(state_dict))

    state_dict = _load_hf_model(model_name, device=device)
    return LoadedModel(name=model_name, state_dict=dict(state_dict))
