import { readFileSync, writeFileSync, appendFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { createRequire } from 'node:module';
import { spawnSync } from 'node:child_process';
import { performance } from 'node:perf_hooks';
import { decodeEventLog, type Abi } from 'viem';

const require = createRequire(import.meta.url);
// eslint-disable-next-line @typescript-eslint/no-var-requires
const addon = require('../rust-napi') as {
	decodeFile(abiPath: string, eventName: string, inputPath: string): {
		decoded: number;
		elapsedMs: number;
	};
};

const mixed = process.env.MIXED === '1' || process.env.MIXED === 'true';
const inputPath = resolve(process.env.IN || 'data/logs.jsonl');
const abiPath = resolve(mixed ? 'abi/mixed.json' : 'abi/erc20.json');
const batchSize = Number(process.env.BATCH || '50000');
const iterations = Number(process.env.ITERS || '1');
const samplePath = resolve('data/logs_sample.jsonl');
const warmPath = resolve('data/logs_sample_warm.jsonl');
const cliBin = resolve('rust-cli/target/release/evm_rust_decoder');

const lines = readFileSync(inputPath, 'utf8').split('\n').filter(Boolean);
const small = lines.slice(0, batchSize);
writeFileSync(samplePath, small.join('\n'));

const warmCount = Math.min(5000, small.length);
if (warmCount > 0) writeFileSync(warmPath, small.slice(0, warmCount).join('\n'));

const abi = JSON.parse(readFileSync(abiPath, 'utf8')) as Abi;
const events = abi.filter((i: any) => i.type === 'event');
const transfer = events.find((i: any) => i.name === 'Transfer') as any;

function runViemOver(linesArr: string[], isMixed: boolean): void {
	for (const line of linesArr) {
		const { topics, data } = JSON.parse(line);
		if (isMixed) decodeEventLog({ abi: events as any, data, topics });
		else decodeEventLog({ abi: [transfer], data, topics });
	}
}

function runViemMs(): number {
	const t0 = performance.now();
	runViemOver(small, mixed);
	return performance.now() - t0;
}

function runNapiMs(): number {
	const res = addon.decodeFile(abiPath, mixed ? '' : 'Transfer', samplePath);
	return res.elapsedMs;
}

function runCliMs(): number {
	const t0 = performance.now();
	const args = mixed
		? ['--abi', abiPath, '--input', samplePath]
		: ['--abi', abiPath, '--event', 'Transfer', '--input', samplePath];
	const out = spawnSync(cliBin, args, { encoding: 'utf8' });
	if (out.error) throw out.error;
	return performance.now() - t0;
}

function warmup(): void {
	if (warmCount === 0) return;
	runViemOver(small.slice(0, warmCount), mixed);
	addon.decodeFile(abiPath, mixed ? '' : 'Transfer', warmPath);
	const args = mixed
		? ['--abi', abiPath, '--input', warmPath]
		: ['--abi', abiPath, '--event', 'Transfer', '--input', warmPath];
	spawnSync(cliBin, args, { encoding: 'utf8' });
}

function avg(nums: number[]): number {
	return nums.reduce((a, b) => a + b, 0) / Math.max(nums.length, 1);
}

async function main() {
	warmup();

	const viemRuns: number[] = [];
	const napiRuns: number[] = [];
	const cliRuns: number[] = [];

	for (let i = 0; i < iterations; i++) {
		viemRuns.push(runViemMs());
		napiRuns.push(runNapiMs());
		cliRuns.push(runCliMs());
	}

	const viemMs = avg(viemRuns);
	const napiMs = avg(napiRuns);
	const cliMs = avg(cliRuns);
	const toLps = (ms: number) => (batchSize / (ms / 1000)).toFixed(0);

	const summary = {
		mode: mixed ? 'mixed' : 'single',
		batch: batchSize,
		iters: iterations,
		warmup: true,
		results: {
			viem: { overall_ms: Number(viemMs.toFixed(3)), lps: Number(toLps(viemMs)), runs_ms: viemRuns.map(v => Number(v.toFixed(3))) },
			napi: { overall_ms: Number(napiMs.toFixed(3)), lps: Number(toLps(napiMs)), runs_ms: napiRuns.map(v => Number(v.toFixed(3))) },
			cli: { overall_ms: Number(cliMs.toFixed(3)), lps: Number(toLps(cliMs)), runs_ms: cliRuns.map(v => Number(v.toFixed(3))) },
		},
		ts: new Date().toISOString(),
	};

	console.log('summary:', JSON.stringify(summary));
	try {
		appendFileSync(resolve('data/bench_results.jsonl'), JSON.stringify(summary) + '\n');
	} catch {}

	console.log(`overall_ms viem=${viemMs.toFixed(3)} lps=${toLps(viemMs)}`);
	console.log(`overall_ms napi=${napiMs.toFixed(3)} lps=${toLps(napiMs)}`);
	console.log(`overall_ms cli=${cliMs.toFixed(3)} lps=${toLps(cliMs)}`);
}

main().catch((e) => {
	console.error(e);
	process.exit(1);
});
