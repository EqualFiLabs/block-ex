use anyhow::Result;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct TxJson {
    pub version: u64,
    pub vin: Vec<serde_json::Value>,
    pub vout: Vec<serde_json::Value>,
    pub extra: String,
    #[serde(default)]
    pub rct_signatures: serde_json::Value,
    #[serde(default)]
    pub rctsig_prunable: serde_json::Value,
    #[serde(default)]
    pub unlock_time: u64,
}

#[derive(Debug)]
pub struct TxAnalysis {
    pub version: u64,
    pub num_inputs: usize,
    pub num_outputs: usize,
    pub ring_sizes: Vec<usize>,
    pub bp_plus: bool,
    pub bp_total_bytes: usize,
    pub tx_extra_tags: Vec<TxExtraTag>,
}

#[derive(Debug, Clone)]
pub enum TxExtraTag {
    PubKey(String),
    Nonce(Vec<u8>),
    AdditionalPubKeys(usize),
    Unknown(u8, usize),
}

pub fn parse_tx_json(json_str: &str) -> Result<TxJson> {
    Ok(serde_json::from_str::<TxJson>(json_str)?)
}

pub fn analyze_tx(tx: &TxJson) -> Result<TxAnalysis> {
    let num_inputs = tx.vin.len();
    let num_outputs = tx.vout.len();
    let ring_sizes = extract_ring_sizes(&tx.vin);
    let (bp_plus, bp_total_bytes) = estimate_bp(&tx.rctsig_prunable)?;
    let tx_extra_tags = parse_tx_extra(&tx.extra)?;

    Ok(TxAnalysis {
        version: tx.version,
        num_inputs,
        num_outputs,
        ring_sizes,
        bp_plus,
        bp_total_bytes,
        tx_extra_tags,
    })
}

fn extract_ring_sizes(vin: &Vec<serde_json::Value>) -> Vec<usize> {
    vin.iter()
        .map(|v| {
            v.get("key")
                .and_then(|k| k.get("key_offsets"))
                .and_then(|ko| ko.as_array())
                .map(|a| a.len())
                .unwrap_or(0)
        })
        .collect()
}

fn estimate_bp(prunable: &serde_json::Value) -> Result<(bool, usize)> {
    if prunable.is_null() {
        return Ok((true, 0));
    }

    if let Some(arr) = prunable.get("bp") {
        let bytes = arr.to_string().as_bytes().len();
        return Ok((true, bytes));
    }

    if let Some(arr) = prunable.get("bp_plus") {
        let bytes = arr.to_string().as_bytes().len();
        return Ok((true, bytes));
    }

    Ok((true, 0))
}

pub fn parse_tx_extra(hex_str: &str) -> Result<Vec<TxExtraTag>> {
    let bytes = hex::decode(hex_str)?;
    let mut tags = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        let tag = bytes[i];
        i += 1;
        match tag {
            0x00 => {}
            0x01 => {
                if i + 32 > bytes.len() {
                    break;
                }
                let pk = hex::encode(&bytes[i..i + 32]);
                tags.push(TxExtraTag::PubKey(pk));
                i += 32;
            }
            0x02 => {
                if i >= bytes.len() {
                    break;
                }
                let len = bytes[i] as usize;
                i += 1;
                if i + len > bytes.len() {
                    break;
                }
                tags.push(TxExtraTag::Nonce(bytes[i..i + len].to_vec()));
                i += len;
            }
            0x04 => {
                if i >= bytes.len() {
                    break;
                }
                let len = bytes[i] as usize;
                i += 1;
                if i + len > bytes.len() {
                    break;
                }
                let count = len / 32;
                tags.push(TxExtraTag::AdditionalPubKeys(count));
                i += len;
            }
            other => {
                if i < bytes.len() {
                    let len = bytes[i] as usize;
                    i += 1;
                    if i + len > bytes.len() {
                        break;
                    }
                    tags.push(TxExtraTag::Unknown(other, len));
                    i += len;
                } else {
                    tags.push(TxExtraTag::Unknown(other, 0));
                }
            }
        }
    }
    Ok(tags)
}
