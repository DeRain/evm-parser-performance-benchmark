import { resolve } from 'node:path';
import { performance } from 'node:perf_hooks';
import { spawnSync } from 'node:child_process';

function run() {
  const inputPath = resolve(process.env.IN || 'data/logs.jsonl');
  const abiPath = resolve('abi/erc20.json');
  const bin = resolve('rust-cli/target/release/evm_rust_decoder');

  const t0 = performance.now();
  const out = spawnSync(bin, ['--abi', abiPath, '--event', 'Transfer', '--input', inputPath], {
    encoding: 'utf8',
  });
  const t1 = performance.now();

  if (out.error) {
    console.error(out.error);
    process.exit(1);
  }
  const stderr = out.stderr.trim();
  const ms = t1 - t0;
  console.log(stderr || '');
  console.log(`rust_cli wrapped_elapsed_ms=${ms.toFixed(3)}`);
}

run();
