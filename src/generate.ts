import { createWriteStream, mkdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { randomBytes } from 'node:crypto';
import { keccak256, toHex } from 'viem';

const TRANSFER_SIG = 'Transfer(address,address,uint256)';
const APPROVAL_SIG = 'Approval(address,address,uint256)';
const TRANSFER_SINGLE_SIG = 'TransferSingle(address,address,address,uint256,uint256)';

const TRANSFER_TOPIC0 = keccak256(toHex(TRANSFER_SIG));
const APPROVAL_TOPIC0 = keccak256(toHex(APPROVAL_SIG));
const TRANSFER_SINGLE_TOPIC0 = keccak256(toHex(TRANSFER_SINGLE_SIG));

function randomHex(bytes: number): string {
  const b = randomBytes(bytes);
  return '0x' + b.toString('hex');
}

function pad32(hex: string): string {
  const s = hex.replace(/^0x/, '');
  return '0x' + s.padStart(64, '0');
}

function addressToTopic(addr: string): string {
  return pad32(addr);
}

function encodeUint256BigEndian(value: bigint): string {
  const hex = value.toString(16);
  return pad32('0x' + hex);
}

function generateErc20Transfer(): string {
  const from = randomHex(20);
  const to = randomHex(20);
  const value = BigInt.asUintN(256, BigInt(Math.floor(Math.random() * 1e9)));
  const topics = [TRANSFER_TOPIC0, addressToTopic(from), addressToTopic(to)];
  const data = encodeUint256BigEndian(value);
  return JSON.stringify({ topics, data });
}

function generateErc20Approval(): string {
  const owner = randomHex(20);
  const spender = randomHex(20);
  const value = BigInt.asUintN(256, BigInt(Math.floor(Math.random() * 1e9)));
  const topics = [APPROVAL_TOPIC0, addressToTopic(owner), addressToTopic(spender)];
  const data = encodeUint256BigEndian(value);
  return JSON.stringify({ topics, data });
}

function generateErc1155TransferSingle(): string {
  const operator = randomHex(20);
  const from = randomHex(20);
  const to = randomHex(20);
  const id = BigInt.asUintN(256, BigInt(Math.floor(Math.random() * 1e6)));
  const value = BigInt.asUintN(256, BigInt(Math.floor(Math.random() * 1e6)));
  const topics = [
    TRANSFER_SINGLE_TOPIC0,
    addressToTopic(operator),
    addressToTopic(from),
    addressToTopic(to),
  ];
  const idHex = encodeUint256BigEndian(id).slice(2);
  const valueHex = encodeUint256BigEndian(value).slice(2);
  const data = '0x' + idHex + valueHex;
  return JSON.stringify({ topics, data });
}

async function main() {
  const count = Number(process.env.COUNT || '200000');
  const outPath = resolve(process.env.OUT || 'data/logs.jsonl');
  const mixed = process.env.MIXED === '1' || process.env.MIXED === 'true';
  mkdirSync(dirname(outPath), { recursive: true });
  const ws = createWriteStream(outPath);

  console.log(
    `Generating ${count} ${mixed ? 'MIXED (ERC20/Approval/ERC1155.Single)' : 'ERC20 Transfer'} logs to ${outPath}...`
  );
  for (let i = 0; i < count; i++) {
    let line: string;
    if (mixed) {
      const r = Math.random();
      if (r < 0.5) line = generateErc20Transfer();
      else if (r < 0.8) line = generateErc20Approval();
      else line = generateErc1155TransferSingle();
    } else {
      line = generateErc20Transfer();
    }
    ws.write(line + '\n');
  }
  await new Promise((r) => ws.end(r));
  console.log('Done.');
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
