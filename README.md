# EVM Event Parsing Performance: Node vs Rust

This repo benchmarks decoding Ethereum logs using three approaches:
- Node.js (viem `decodeEventLog`)
- Rust CLI (`ethabi`) invoked from Node via a child process
- Rust N-API addon (`napi-rs` + `ethabi`) called in-process from Node

## Requirements
- Node.js 18+
- Rust toolchain via `rustup`

```bash
rustup default stable
```

## Install
```bash
npm install
```

## Build
- Build Rust CLI:
```bash
npm run build:rust
```
- Build Rust N-API addon:
```bash
npm run build:napi
```

## Generate Dataset
You can generate either a single-event dataset (ERC20 Transfer) or a mixed-event dataset (ERC20 Transfer, ERC20 Approval, ERC1155 TransferSingle).

- Single-event (default: ERC20 Transfer):
```bash
COUNT=1000000 npm run gen
```

- Mixed events (set `MIXED=1`):
```bash
MIXED=1 COUNT=1000000 npm run gen
```

Datasets are written to `data/logs.jsonl` (JSONL with `{ topics: string[], data: string }`).

## Quick Standalone Benchmarks
These read `data/logs.jsonl` and print total decoded, elapsed ms, and LPS.

- JS decoder (viem):
```bash
npm run bench:js
```

- Rust CLI (child process):
```bash
npm run bench:rust
```

- Rust N-API addon:
```bash
npm run bench:napi
```

## Suite (perf_hooks-based)
Runs all three approaches on the same sampled subset, reports overall time in ms and LPS. Uses Node `performance.now()` with a short warmup and optional iterations (ITERS).

- Run suite on a batch (default BATCH=50000):
```bash
BATCH=100000 npm run suite
```

- Mixed-mode suite (use combined ABI and topic0-based routing in Rust decoders):
```bash
MIXED=1 BATCH=100000 npm run suite
```

- Multiple iterations (averaged):
```bash
ITERS=3 BATCH=100000 npm run suite
```

### Suite behavior
- Samples first `BATCH` lines from `data/logs.jsonl` into `data/logs_sample.jsonl` and runs all three on this file.
- Always performs a short warmup on each approach before measuring.
- Appends JSONL summaries to `data/bench_results.jsonl` with per-approach run times, averages, LPS, and metadata.
- viem path:
  - Single-event: `decodeEventLog({ abi: [Transfer], data, topics })`
  - Mixed: `decodeEventLog({ abi: fullAbi, data, topics })`
- Rust paths:
  - Single-event: pass event name `Transfer`
  - Mixed: event name omitted; decoders select the event by `topic0` from the ABI

## Example Results
Single-event (BATCH=100k):
```
overall_ms viem=3350.593 lps=29845
overall_ms napi=391.593 lps=255367
overall_ms cli=371.403 lps=269249
```

Mixed events (BATCH=100k, MIXED=1):
```
overall_ms viem=4077.818 lps=24523
overall_ms napi=436.280 lps=229211
overall_ms cli=405.427 lps=246654
```

Mixed events (BATCH=1,000,000, MIXED=1):
```
overall_ms viem=41384.634 lps=24164
overall_ms napi=4258.124 lps=234845
overall_ms cli=4061.436 lps=246218
```

Single-event (BATCH=1,000,000):
```
overall_ms viem=36127.264 lps=27680
overall_ms napi=3979.142 lps=251310
overall_ms cli=3706.585 lps=269790
```

## Files of Interest
- ABIs: `abi/erc20.json`, `abi/mixed.json`
- Generator: `src/generate.ts` (supports `MIXED=1`)
- Suite: `src/suite.ts` (uses `BATCH`, `ITERS`)
- Standalone benches: `src/bench_viem.ts`, `src/bench_rust.ts`, `src/bench_napi.ts`
- Rust decoders:
  - CLI: `rust-cli/src/main.rs` (supports multi-event via topic0 when `--event` omitted)
  - N-API: `rust-napi/src/lib.rs` (exports `decodeFile` with same multi-event behavior)

## Troubleshooting
- If the suite is slow or you see timeouts, reduce `BATCH` or `ITERS`.
- After changing Rust code, rebuild:
```bash
npm run build:rust
npm run build:napi
```
- Ensure `data/logs.jsonl` exists (generate with `npm run gen`).

## Notes
- Mixed-mode topic0s are computed via `keccak256(eventSignature)` in the generator.
- The N-API path should be close to the CLI path; both are significantly faster than pure JS in these tests.
- Further optimization: using Neon (native Node addon framework) instead of N-API glue used here may reduce call overhead on some platforms and edge cases. If you’re pushing for maximum Node↔Rust bridge throughput, evaluating Neon is worthwhile.
