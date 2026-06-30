# Arcturus Compiler

`arcturus_compiler` converts PyTorch language models into the sparse `W` matrix
format used by the Arcturus firmware.

## Install

```bash
pip install -r requirements.txt
pip install -e .
```

## Usage

Single-file export:

```bash
python -m arcturus_compiler --model gpt2 --output gpt2.awm
```

Hardware upload workflow:

```bash
python -m arcturus_compiler --model meta-llama/Llama-2-7b --output llama2.awm --split-banks
arcturus-cli --port COM3 upload --bank 5 --file llama2_bank005.awm
```

## Notes

- The emitted binary matches the firmware's current `SparseWMatrix::serialize()`
  layout:
  - `dimension: u16`
  - `num_elements: u32`
  - repeated `[row: u16, col: u16, re: i32, im: i32]`
- The compiler uses group-wise 4-bit quantization internally and blockwise
  sparse projection to keep element counts manageable for PSRAM banking.
