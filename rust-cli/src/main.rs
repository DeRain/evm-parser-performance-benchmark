use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::PathBuf;
use std::time::Instant;

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use ethereum_types::H256;
use ethabi::{Event, EventParam, ParamType, RawLog, Token};
use hex::FromHex;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Parser, Debug)]
#[command(author, version, about = "EVM log decoder using ethabi", long_about = None)]
struct CliArgs {
    /// Path to ABI JSON file (array or object containing events)
    #[arg(long)]
    abi: PathBuf,

    /// Event name to decode (e.g. Transfer). If omitted, all events in ABI are supported via topic0.
    #[arg(long)]
    event: Option<String>,

    /// Read input from file (JSONL with {"topics":[...],"data":"0x..."}), default stdin
    #[arg(long)]
    input: Option<PathBuf>,

    /// Print decoded JSON per line to stdout. If not set, decoding is performed silently.
    #[arg(long, default_value_t = false)]
    print: bool,
}

#[derive(Deserialize)]
struct LogLine {
    topics: Vec<String>,
    data: String,
}

fn main() -> Result<()> {
    let args = CliArgs::parse();

    let (selected_event, all_events) = load_event(&args.abi, args.event.as_deref().unwrap_or(""))
        .with_context(|| format!("Failed to load event(s) from {:?}", args.abi))?;

    let mut topic0_to_event: Option<HashMap<H256, Event>> = None;
    if args.event.is_none() {
        let mut map = HashMap::new();
        for ev in &all_events {
            let sig: H256 = ev.signature();
            map.insert(sig, ev.clone());
        }
        topic0_to_event = Some(map);
    }

    let reader: Box<dyn BufRead> = match &args.input {
        Some(path) => Box::new(BufReader::new(File::open(path)?)),
        None => Box::new(BufReader::new(io::stdin())),
    };

    let start = Instant::now();
    let mut total: usize = 0;

    for line in reader.lines() {
        let line = line?;
        if line.is_empty() { continue; }
        let parsed: LogLine = serde_json::from_str(&line)
            .with_context(|| format!("Invalid JSON line: {}", line))?;

        let parsed_topics: Vec<H256> = parsed
            .topics
            .iter()
            .map(|t| parse_h256(t))
            .collect::<Result<Vec<H256>>>()?;
        if parsed_topics.is_empty() { continue; }

        let event = if let Some(map) = &topic0_to_event {
            map.get(&parsed_topics[0])
                .cloned()
                .ok_or_else(|| anyhow!("Unknown topic0 for provided ABI"))?
        } else {
            selected_event.clone()
        };

        let raw_log = RawLog {
            topics: parsed_topics,
            data: parse_hex_bytes(&parsed.data)?,
        };

        let log = event
            .parse_log(raw_log)
            .with_context(|| "Failed to parse log with ethabi")?;

        total += 1;

        if args.print {
            let values = log.params.iter().map(|p| p.value.clone()).collect();
            let value = tokens_to_json(&event.inputs, &values);
            println!("{}", serde_json::to_string(&value)?);
        }
    }

    let elapsed = start.elapsed();
    eprintln!(
        "decoded={} elapsed_ms={:.3} throughput_lps={:.0}",
        total,
        elapsed.as_secs_f64() * 1000.0,
        if elapsed.as_secs_f64() > 0.0 { (total as f64 / elapsed.as_secs_f64()).round() } else { 0.0 }
    );

    Ok(())
}

fn load_event(path: &PathBuf, event_name: &str) -> Result<(Event, Vec<Event>)> {
    let file = File::open(path).with_context(|| format!("Cannot open ABI file: {:?}", path))?;
    let json_value: Value = serde_json::from_reader(file)?;

    // ABI can be an array or an object with `abi` or `events`
    let events: Vec<Event> = if json_value.is_array() {
        let arr = json_value.as_array().unwrap();
        arr.iter()
            .filter_map(|v| parse_event_from_value(v))
            .collect::<Vec<Event>>()
    } else if json_value.is_object() {
        if let Some(arr) = json_value.get("abi").and_then(|v| v.as_array()) {
            arr.iter()
                .filter_map(|v| parse_event_from_value(v))
                .collect::<Vec<Event>>()
        } else if let Some(arr) = json_value.get("events").and_then(|v| v.as_array()) {
            arr.iter()
                .filter_map(|v| parse_event_from_value(v))
                .collect::<Vec<Event>>()
        } else {
            return Err(anyhow!("Unsupported ABI JSON structure"));
        }
    } else {
        return Err(anyhow!("Unsupported ABI JSON structure"));
    };

    let event = if event_name.is_empty() {
        events
            .first()
            .cloned()
            .ok_or_else(|| anyhow!("No events found in ABI"))?
    } else {
        events
            .iter()
            .find(|e| e.name == event_name)
            .cloned()
            .ok_or_else(|| anyhow!("Event '{}' not found in ABI", event_name))?
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

fn parse_h256(s: &str) -> Result<H256> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    let bytes = <[u8; 32]>::from_hex(s).with_context(|| format!("Invalid H256 hex: {}", s))?;
    Ok(H256::from(bytes))
}

fn parse_hex_bytes(s: &str) -> Result<Vec<u8>> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    let bytes = Vec::from_hex(s).with_context(|| format!("Invalid hex bytes: {}", s))?;
    Ok(bytes)
}

fn token_to_json(token: &Token) -> Value {
    match token {
        Token::Address(addr) => json!(format!("0x{}", hex::encode(addr.as_bytes()))),
        Token::Uint(uint) => json!(uint.to_string()),
        Token::Int(int) => json!(int.to_string()),
        Token::Bool(b) => json!(*b),
        Token::FixedBytes(b) | Token::Bytes(b) => json!(format!("0x{}", hex::encode(b))),
        Token::String(s) => json!(s),
        Token::Array(arr) => Value::Array(arr.iter().map(token_to_json).collect()),
        Token::Tuple(arr) => Value::Array(arr.iter().map(token_to_json).collect()),
        Token::FixedArray(arr) => Value::Array(arr.iter().map(token_to_json).collect()),
    }
}

fn tokens_to_json(inputs: &[EventParam], tokens: &Vec<Token>) -> Value {
    let mut obj = serde_json::Map::new();
    for (i, token) in tokens.iter().enumerate() {
        let name = inputs.get(i).map(|p| p.name.as_str()).unwrap_or("");
        let key = if name.is_empty() { format!("arg{}", i) } else { name.to_string() };
        obj.insert(key, token_to_json(token));
    }
    Value::Object(obj)
}
