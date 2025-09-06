use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::time::Instant;

use anyhow::{anyhow, Context};
use ethereum_types::H256;
use ethabi::{Event, EventParam, ParamType, RawLog};
use hex::FromHex;
use napi::bindgen_prelude::*;
use napi_derive::napi;
use serde_json::Value;

#[napi(object)]
pub struct DecodeResult {
	pub decoded: u32,
	pub elapsedMs: f64,
}

#[napi]
pub fn decode_file(abi_path: String, event_name: String, input_path: String) -> Result<DecodeResult> {
	let abi_path = PathBuf::from(abi_path);
	let (selected_event, events) = load_event(&abi_path, if event_name.is_empty() { "" } else { &event_name })
		.map_err(|e| Error::from_reason(e.to_string()))?;

	let mut topic0_to_event: Option<HashMap<H256, Event>> = None;
	if event_name.is_empty() {
		let mut map = HashMap::new();
		for ev in &events { map.insert(ev.signature(), ev.clone()); }
		topic0_to_event = Some(map);
	}

	let file = File::open(&input_path)
		.with_context(|| format!("Cannot open input file: {}", input_path))
		.map_err(|e| Error::from_reason(e.to_string()))?;
	let reader = BufReader::new(file);

	let start = Instant::now();
	let mut total: u32 = 0;
	for line in reader.lines() {
		let line = line.map_err(|e| Error::from_reason(e.to_string()))?;
		if line.is_empty() { continue; }
		let v: Value = serde_json::from_str(&line).map_err(|e| Error::from_reason(e.to_string()))?;
		let topics_v = v.get("topics").and_then(|t| t.as_array()).ok_or_else(|| Error::from_reason("no topics".to_string()))?;
		let data_s = v.get("data").and_then(|d| d.as_str()).ok_or_else(|| Error::from_reason("no data".to_string()))?;

		let topics: Vec<H256> = topics_v
			.iter()
			.map(|tv| tv.as_str().ok_or_else(|| Error::from_reason("topic not string".to_string())) )
			.collect::<std::result::Result<Vec<&str>, Error>>()?
			.into_iter()
			.map(|s| parse_h256(s))
			.collect::<anyhow::Result<Vec<H256>>>()
			.map_err(|e| Error::from_reason(e.to_string()))?;
		if topics.is_empty() { continue; }

		let event = if let Some(map) = &topic0_to_event {
			match map.get(&topics[0]) { Some(ev) => ev.clone(), None => return Err(Error::from_reason("unknown topic0".to_string())) }
		} else { selected_event.clone() };

		let data = parse_hex_bytes(data_s).map_err(|e| Error::from_reason(e.to_string()))?;
		let raw = RawLog { topics, data };
		let _ = event.parse_log(raw).map_err(|e| Error::from_reason(e.to_string()))?;
		total = total.saturating_add(1);
	}
	let elapsed = start.elapsed();

	Ok(DecodeResult { decoded: total, elapsedMs: elapsed.as_secs_f64() * 1000.0 })
}

fn load_event(path: &PathBuf, event_name: &str) -> anyhow::Result<(Event, Vec<Event>)> {
	let file = File::open(path).with_context(|| format!("Cannot open ABI file: {:?}", path))?;
	let json_value: Value = serde_json::from_reader(file)?;

	let events: Vec<Event> = if json_value.is_array() {
		let arr = json_value.as_array().unwrap();
		arr.iter().filter_map(|v| parse_event_from_value(v)).collect()
	} else if json_value.is_object() {
		if let Some(arr) = json_value.get("abi").and_then(|v| v.as_array()) {
			arr.iter().filter_map(|v| parse_event_from_value(v)).collect()
		} else if let Some(arr) = json_value.get("events").and_then(|v| v.as_array()) {
			arr.iter().filter_map(|v| parse_event_from_value(v)).collect()
		} else { return Err(anyhow!("Unsupported ABI JSON structure")); }
	} else { return Err(anyhow!("Unsupported ABI JSON structure")); };

	let event = if event_name.is_empty() {
		events.first().cloned().ok_or_else(|| anyhow!("No events in ABI"))?
	} else {
		events.iter().find(|e| e.name == event_name).cloned().ok_or_else(|| anyhow!("Event not found"))?
	};
	Ok((event, events))
}

fn parse_event_from_value(v: &Value) -> Option<Event> {
	if v.get("type").and_then(|t| t.as_str()) != Some("event") { return None; }
	let name = v.get("name")?.as_str()?.to_string();
	let inputs_v = v.get("inputs")?.as_array()?.clone();
	let mut inputs: Vec<EventParam> = Vec::with_capacity(inputs_v.len());
	for i in inputs_v {
		let name_i = i.get("name").and_then(|s| s.as_str()).unwrap_or("").to_string();
		let indexed = i.get("indexed").and_then(|b| b.as_bool()).unwrap_or(false);
		let type_str = i.get("type").and_then(|s| s.as_str()).unwrap_or("");
		let param_type = match parse_param_type(type_str) { Some(t) => t, None => return None };
		inputs.push(EventParam { name: name_i, kind: param_type, indexed });
	}
	Some(Event { name, inputs, anonymous: false })
}

fn parse_param_type(s: &str) -> Option<ParamType> {
	match s {
		"address" => Some(ParamType::Address),
		"bool" => Some(ParamType::Bool),
		"string" => Some(ParamType::String),
		"bytes" => Some(ParamType::Bytes),
		_ if s.starts_with("bytes") => { let n: usize = s[5..].parse().ok()?; Some(ParamType::FixedBytes(n)) }
		_ if s.starts_with("uint") => { let n: usize = s[4..].parse().unwrap_or(256); Some(ParamType::Uint(n)) }
		_ if s.starts_with("int") => { let n: usize = s[3..].parse().unwrap_or(256); Some(ParamType::Int(n)) }
		_ if s.ends_with("[]") => { let inner = &s[..s.len()-2]; let inner_t = parse_param_type(inner)?; Some(ParamType::Array(Box::new(inner_t))) }
		_ => None,
	}
}

fn parse_h256(s: &str) -> anyhow::Result<H256> {
	let s = s.strip_prefix("0x").unwrap_or(s);
	let bytes = <[u8; 32]>::from_hex(s)?;
	Ok(H256::from(bytes))
}

fn parse_hex_bytes(s: &str) -> anyhow::Result<Vec<u8>> {
	let s = s.strip_prefix("0x").unwrap_or(s);
	let bytes = Vec::from_hex(s)?;
	Ok(bytes)
}
