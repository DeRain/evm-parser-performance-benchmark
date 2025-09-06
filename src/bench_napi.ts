import { resolve } from 'node:path';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
// eslint-disable-next-line @typescript-eslint/no-var-requires
const addon = require('../rust-napi') as {
	decodeFile(abiPath: string, eventName: string, inputPath: string): {
		decoded: number;
		elapsedMs: number;
	};
};

function run() {
	const inputPath = resolve(process.env.IN || 'data/logs.jsonl');
	const abiPath = resolve('abi/erc20.json');
	const res = addon.decodeFile(abiPath, 'Transfer', inputPath);
	console.log(
		`napi_ethabi decoded=${res.decoded} elapsed_ms=${res.elapsedMs.toFixed(3)} throughput_lps=${(
			res.decoded / (res.elapsedMs / 1000)
		).toFixed(0)}`
	);
}

run();
