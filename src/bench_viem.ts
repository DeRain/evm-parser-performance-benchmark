import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { performance } from 'node:perf_hooks';
import { decodeEventLog, type Abi } from 'viem';

const ERC20_ABI = JSON.parse(
  readFileSync(resolve('abi/erc20.json'), 'utf8')
) as Abi;

const transfer = ERC20_ABI.find(
  (i: any) => i.type === 'event' && i.name === 'Transfer'
) as any;

function run() {
  const inputPath = resolve(process.env.IN || 'data/logs.jsonl');
  const lines = readFileSync(inputPath, 'utf8')
    .split('\n')
    .filter(Boolean);

  let decoded = 0;
  const t0 = performance.now();
  for (const line of lines) {
    const { topics, data } = JSON.parse(line);
    decodeEventLog({ abi: [transfer], data, topics });
    decoded++;
  }
  const t1 = performance.now();
  const ms = t1 - t0;
  const lps = (decoded / (ms / 1000)).toFixed(0);
  console.log(
    `js_viem decoded=${decoded} elapsed_ms=${ms.toFixed(3)} throughput_lps=${lps}`
  );
}

run();
